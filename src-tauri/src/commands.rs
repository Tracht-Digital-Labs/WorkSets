use crate::desktop::{CompatibilityInfo, DesktopBroker, DesktopSnapshot, BROKER_PROTOCOL_VERSION};
use crate::domain::{
    migrate_preset_value, DesktopPolicy, DesktopSpec, LaunchItem, LaunchTarget, LifecyclePolicy,
    MonitorRef, Notice, NoticeSeverity, PresetV1, RectI32, Settings, WindowState,
    PRESET_SCHEMA_VERSION,
};
use crate::error::{ApiError, ApiResult};
use crate::planner::{
    ApplyOptions, ApplyPlan, ApplyReport, ApplyStatus, ApplyStep, ApplyStepResult, ItemApplyResult,
    ItemApplyStatus, StepStatus, WindowReference,
};
use crate::store::{
    OwnedWindow, RuntimeOwnership, Store, TelemetryApplyStatus, TelemetryEvent, TelemetryEventKind,
};
use crate::window::{
    CapturedWindow, CloseOutcome, OwnedWindowProof, WindowManager, WindowWaitRequest,
};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use parking_lot::Mutex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_autostart::ManagerExt as AutostartManagerExt;
use tauri_plugin_global_shortcut::GlobalShortcutExt;
use uuid::Uuid;

pub struct AppRuntime {
    pub store: Store,
    pub broker: DesktopBroker,
    busy: AtomicBool,
    cancelled: AtomicBool,
    experimental_desktop_acknowledged: AtomicBool,
    ownership: Mutex<RuntimeOwnership>,
    update_cache: Mutex<Option<UpdateInfo>>,
}

impl AppRuntime {
    pub fn initialize() -> ApiResult<Self> {
        let store = Store::open_default()?;
        let session_id = Uuid::new_v4();
        let ownership = RuntimeOwnership {
            session_id,
            windows: Vec::new(),
        };
        store.save_runtime_ownership(&ownership)?;
        let update_cache = store.load_update_cache().unwrap_or(None);
        let settings = store.load_settings()?;
        let _ = store.record_telemetry(
            settings.telemetry_opt_in,
            TelemetryEvent::new(TelemetryEventKind::AppStarted),
        );
        Ok(Self {
            store,
            broker: DesktopBroker::new(),
            busy: AtomicBool::new(false),
            cancelled: AtomicBool::new(false),
            experimental_desktop_acknowledged: AtomicBool::new(false),
            ownership: Mutex::new(ownership),
            update_cache: Mutex::new(update_cache),
        })
    }

    pub fn is_busy(&self) -> bool {
        self.busy.load(Ordering::SeqCst)
    }
}

struct BusyGuard<'a>(&'a AppRuntime);

impl<'a> BusyGuard<'a> {
    fn acquire(runtime: &'a AppRuntime) -> ApiResult<Self> {
        runtime
            .busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| {
                ApiError::new(
                    "applyAlreadyRunning",
                    "Another preset switch is already running",
                )
            })?;
        runtime.cancelled.store(false, Ordering::SeqCst);
        Ok(Self(runtime))
    }
}

impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        self.0.busy.store(false, Ordering::SeqCst);
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AppState {
    pub version: String,
    pub settings: Settings,
    pub presets: Vec<PresetV1>,
    pub compatibility: CompatibilityInfo,
    pub busy: bool,
    pub data_directory: String,
    pub last_apply_report: Option<ApplyReport>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CaptureResult {
    pub preset: PresetV1,
    pub captured_windows: Vec<CapturedWindow>,
    pub warnings: Vec<Notice>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub preset: PresetV1,
    pub committed: bool,
    pub warnings: Vec<Notice>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub checked_at: DateTime<Utc>,
    pub current_version: String,
    pub latest_version: Option<String>,
    pub available: bool,
    pub download_url: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostics {
    pub version: String,
    pub compatibility: CompatibilityInfo,
    pub preset_count: u32,
    pub busy: bool,
    pub data_directory: String,
    pub last_apply_status: Option<ApplyStatus>,
    pub broker_protocol_version: u32,
    pub privacy: DiagnosticsPrivacy,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsPrivacy {
    pub telemetry_transport_enabled: bool,
    pub telemetry_contains_names_paths_urls_or_titles: bool,
}

#[tauri::command]
pub fn get_app_state(runtime: State<'_, Arc<AppRuntime>>) -> ApiResult<AppState> {
    Ok(AppState {
        version: env!("CARGO_PKG_VERSION").to_string(),
        settings: runtime.store.load_settings()?,
        presets: runtime.store.list_presets()?,
        compatibility: runtime.broker.compatibility(),
        busy: runtime.is_busy(),
        data_directory: runtime.store.root().to_string_lossy().into_owned(),
        last_apply_report: runtime.store.last_report()?,
    })
}

#[tauri::command]
pub fn list_presets(runtime: State<'_, Arc<AppRuntime>>) -> ApiResult<Vec<PresetV1>> {
    runtime.store.list_presets()
}

#[tauri::command]
pub async fn capture_workspace(
    runtime: State<'_, Arc<AppRuntime>>,
    name: Option<String>,
) -> ApiResult<CaptureResult> {
    let runtime = Arc::clone(runtime.inner());
    run_blocking(move || capture_workspace_blocking(&runtime, name)).await
}

fn capture_workspace_blocking(
    runtime: &AppRuntime,
    name: Option<String>,
) -> ApiResult<CaptureResult> {
    let snapshot = WindowManager::capture(&runtime.broker)?;
    let mut warnings = Vec::new();
    if snapshot
        .monitors
        .iter()
        .any(|monitor| monitor.id.eq_ignore_ascii_case(&monitor.device_name))
    {
        warnings.push(Notice::warning(
            "monitorIdentityFallback",
            "Windows did not expose a stable identity for every monitor; device names are used as a fallback",
        ));
    }
    let desktop_snapshot = runtime.broker.snapshot().ok();
    if desktop_snapshot.is_none() {
        warnings.push(Notice::warning(
            "desktopCaptureUnavailable",
            "Window positions were captured, but virtual desktop metadata is unavailable",
        ));
    }
    let inferred_count = snapshot
        .windows
        .iter()
        .filter_map(|window| window.desktop_index)
        .max()
        .map_or(1, |index| index + 1);
    let desktop_count = desktop_snapshot
        .map(|desktop| desktop.count)
        .unwrap_or(inferred_count)
        .max(1);
    let desktops: Vec<_> = (0..desktop_count)
        .map(|index| DesktopSpec {
            id: Uuid::new_v4(),
            name: format!("Desktop {}", index + 1),
            index,
        })
        .collect();
    let current_index = desktop_snapshot
        .map(|desktop| desktop.current_index)
        .unwrap_or_default()
        .min(desktop_count - 1);
    let mut instance_counts: HashMap<(String, String), u32> = HashMap::new();
    let mut items = Vec::new();
    for window in &snapshot.windows {
        let Some(path) = &window.executable_path else {
            warnings.push(Notice::warning(
                "windowLaunchTargetUnavailable",
                format!("'{}' cannot be launched automatically", window.application),
            ));
            continue;
        };
        if window.elevated {
            warnings.push(Notice::warning(
                "elevatedWindowCaptured",
                format!(
                    "'{}' is elevated and can only be recorded",
                    window.application
                ),
            ));
        }
        let key = (
            path.to_ascii_lowercase(),
            window.class_name.to_ascii_lowercase(),
        );
        let instance_index = *instance_counts.entry(key).or_default();
        *instance_counts
            .entry((
                path.to_ascii_lowercase(),
                window.class_name.to_ascii_lowercase(),
            ))
            .or_default() += 1;
        let desktop_index = window
            .desktop_index
            .unwrap_or(current_index)
            .min(desktop_count - 1);
        let launch_target = if window
            .aumid
            .as_ref()
            .is_some_and(|aumid| aumid.contains('!'))
        {
            LaunchTarget::PackagedApp {
                aumid: window.aumid.clone().expect("AUMID checked above"),
                args: Vec::new(),
            }
        } else {
            LaunchTarget::Executable {
                path: path.clone(),
                args: Vec::new(),
                working_directory: None,
            }
        };
        items.push(LaunchItem {
            id: Uuid::new_v4(),
            display_name: window.application.clone(),
            desktop_id: desktops[desktop_index as usize].id,
            launch_target,
            selector: window.as_selector(instance_index),
            placement: window.placement.clone(),
            lifecycle: LifecyclePolicy::Reuse,
            enabled: true,
        });
    }
    let mut preset = PresetV1::empty(
        name.unwrap_or_else(|| "Mein Arbeitsbereich".to_string())
            .trim()
            .to_string(),
    );
    preset.desktops = desktops;
    preset.monitors = snapshot.monitors;
    preset.items = items;
    preset.validate()?;
    let result = CaptureResult {
        preset,
        captured_windows: snapshot.windows,
        warnings,
    };
    let settings = runtime.store.load_settings()?;
    let _ = runtime.store.record_telemetry(
        settings.telemetry_opt_in,
        TelemetryEvent::new(TelemetryEventKind::CaptureCompleted {
            window_count: result.captured_windows.len() as u32,
            desktop_count: result.preset.desktops.len() as u32,
            monitor_count: result.preset.monitors.len() as u32,
            warning_count: result.warnings.len() as u32,
        }),
    );
    Ok(result)
}

#[tauri::command]
pub fn save_preset(
    app: AppHandle,
    runtime: State<'_, Arc<AppRuntime>>,
    mut preset: PresetV1,
) -> ApiResult<PresetV1> {
    let now = Utc::now();
    if preset.id.is_nil() {
        preset.id = Uuid::new_v4();
        preset.created_at = now;
    }
    preset.schema_version = PRESET_SCHEMA_VERSION;
    preset.name = preset.name.trim().to_string();
    preset.updated_at = now;
    preset.validate()?;
    save_preset_internal(&app, &runtime, &preset)?;
    Ok(preset)
}

fn save_preset_internal(app: &AppHandle, runtime: &AppRuntime, preset: &PresetV1) -> ApiResult<()> {
    let previous_settings = effective_settings(runtime)?;
    let mut settings = previous_settings.clone();
    if let Some(hotkey) = preset.hotkey.clone() {
        settings.hotkeys.insert(preset.id, hotkey);
    } else {
        settings.hotkeys.remove(&preset.id);
    }
    settings.validate()?;
    register_shortcuts(app, &settings, &previous_settings)?;
    if let Err(error) = runtime.store.save_settings(&settings) {
        let _ = restore_shortcuts(app, &previous_settings);
        return Err(error);
    }
    if let Err(error) = runtime.store.save_preset(preset) {
        let _ = runtime.store.save_settings(&previous_settings);
        let _ = restore_shortcuts(app, &previous_settings);
        return Err(error);
    }
    let _ = crate::refresh_tray(app);
    Ok(())
}

#[tauri::command]
pub fn delete_preset(
    app: AppHandle,
    runtime: State<'_, Arc<AppRuntime>>,
    preset_id: String,
) -> ApiResult<()> {
    let preset_id = parse_id(&preset_id)?;
    let previous_settings = effective_settings(&runtime)?;
    let mut settings = previous_settings.clone();
    settings.hotkeys.remove(&preset_id);
    register_shortcuts(&app, &settings, &previous_settings)?;
    if let Err(error) = runtime.store.save_settings(&settings) {
        let _ = restore_shortcuts(&app, &previous_settings);
        return Err(error);
    }
    if let Err(error) = runtime.store.delete_preset(preset_id) {
        let _ = runtime.store.save_settings(&previous_settings);
        let _ = restore_shortcuts(&app, &previous_settings);
        return Err(error);
    }
    let _ = crate::refresh_tray(&app);
    Ok(())
}

fn effective_settings(runtime: &AppRuntime) -> ApiResult<Settings> {
    let mut settings = runtime.store.load_settings()?;
    let presets = runtime.store.list_presets()?;
    let preset_ids: HashSet<_> = presets.iter().map(|preset| preset.id).collect();
    settings.hotkeys.retain(|id, _| preset_ids.contains(id));
    for preset in presets {
        if let Some(hotkey) = preset.hotkey {
            settings.hotkeys.insert(preset.id, hotkey);
        }
    }
    Ok(settings)
}

fn configured_shortcuts(settings: &Settings) -> impl Iterator<Item = &str> {
    std::iter::once(settings.capture_hotkey.as_str())
        .chain(settings.hotkeys.values().map(String::as_str))
}

fn restore_shortcuts(app: &AppHandle, settings: &Settings) -> ApiResult<()> {
    let manager = app.global_shortcut();
    manager
        .unregister_all()
        .map_err(|error| ApiError::new("hotkeyUpdateFailed", error.to_string()))?;
    for shortcut in configured_shortcuts(settings) {
        manager
            .register(shortcut)
            .map_err(|error| ApiError::new("hotkeyConflict", error.to_string()))?;
    }
    Ok(())
}

fn register_shortcuts(app: &AppHandle, settings: &Settings, previous: &Settings) -> ApiResult<()> {
    settings.validate()?;
    if let Err(error) = restore_shortcuts(app, settings) {
        let _ = restore_shortcuts(app, previous);
        return Err(error);
    }
    Ok(())
}

#[tauri::command]
pub async fn plan_preset_apply(
    runtime: State<'_, Arc<AppRuntime>>,
    preset_id: String,
) -> ApiResult<ApplyPlan> {
    let runtime = Arc::clone(runtime.inner());
    run_blocking(move || {
        let preset = runtime.store.load_preset(parse_id(&preset_id)?)?;
        build_apply_plan(&runtime, &preset)
    })
    .await
}

#[tauri::command]
pub async fn apply_preset(
    app: AppHandle,
    runtime: State<'_, Arc<AppRuntime>>,
    preset_id: String,
    options: Option<ApplyOptions>,
) -> ApiResult<ApplyReport> {
    let runtime = Arc::clone(runtime.inner());
    let worker_app = app.clone();
    let result =
        run_blocking(move || apply_preset_blocking(&worker_app, runtime, preset_id, options)).await;
    // BusyGuard has dropped before the worker resolves, so the tray returns to its normal state.
    let _ = crate::refresh_tray(&app);
    result
}

fn apply_preset_blocking(
    app: &AppHandle,
    runtime: Arc<AppRuntime>,
    preset_id: String,
    options: Option<ApplyOptions>,
) -> ApiResult<ApplyReport> {
    let _guard = BusyGuard::acquire(&runtime)?;
    let _ = crate::refresh_tray(app);
    let preset = runtime.store.load_preset(parse_id(&preset_id)?)?;
    let options = options.unwrap_or_default();
    let mut report = ApplyReport::new(preset.id);

    emit_progress(app, preset.id, ApplyStep::Preflight, "running", None);
    let preflight_started = Instant::now();
    let plan = build_apply_plan(&runtime, &preset)?;
    report.notices.extend(plan.warnings.clone());

    if plan
        .warnings
        .iter()
        .any(|notice| notice.severity == NoticeSeverity::Error)
    {
        report.status = ApplyStatus::NeedsAttention;
        report.steps.push(step_result(
            ApplyStep::Preflight,
            StepStatus::Failed,
            preflight_started,
            Some("Preflight found a blocking target problem".to_string()),
        ));
        emit_progress(
            app,
            preset.id,
            ApplyStep::Preflight,
            "failed",
            Some("Preflight found a blocking target problem"),
        );
        return finish_report(&runtime, report);
    }

    if plan.requires_confirmation && !options.confirm_desktop_changes {
        report.status = ApplyStatus::NeedsAttention;
        let (code, message) = if plan.foreign_windows.is_empty()
            && runtime.broker.compatibility().virtual_desktops.experimental
        {
            (
                "experimentalDesktopProviderNeedsConfirmation",
                "Confirm the one-time experimental desktop provider warning for this session",
            )
        } else {
            (
                "desktopRemovalNeedsConfirmation",
                "Confirm moving unmanaged windows before extra desktops are removed",
            )
        };
        report.notices.push(Notice::warning(code, message));
        report.steps.push(step_result(
            ApplyStep::Preflight,
            StepStatus::Warning,
            preflight_started,
            Some("Desktop removal requires confirmation".to_string()),
        ));
        emit_progress(
            app,
            preset.id,
            ApplyStep::Preflight,
            "warning",
            Some("Desktop removal requires confirmation"),
        );
        return finish_report(&runtime, report);
    }
    if options.confirm_desktop_changes
        && runtime.broker.compatibility().virtual_desktops.experimental
    {
        runtime
            .experimental_desktop_acknowledged
            .store(true, Ordering::SeqCst);
    }

    let mut snapshot = WindowManager::capture(&runtime.broker)?;
    report.steps.push(step_result(
        ApplyStep::Preflight,
        StepStatus::Success,
        preflight_started,
        None,
    ));
    emit_progress(app, preset.id, ApplyStep::Preflight, "success", None);

    if cancelled(&runtime, &mut report) {
        return finish_report(&runtime, report);
    }

    emit_progress(app, preset.id, ApplyStep::Desktops, "running", None);
    let desktop_started = Instant::now();
    let desktop_status = if plan.current_desktop_count < plan.target_desktop_count {
        match runtime.broker.ensure_desktop_count(
            plan.target_desktop_count,
            preset.desktop_policy,
            false,
        ) {
            Ok(_) => match runtime.broker.snapshot() {
                Ok(desktop)
                    if desktop_count_matches_policy(
                        preset.desktop_policy,
                        desktop.count,
                        plan.target_desktop_count,
                    ) =>
                {
                    StepStatus::Success
                }
                Ok(desktop) => {
                    report.notices.push(Notice::error(
                        "desktopCountVerificationFailed",
                        format!(
                            "Windows reports {} desktops after creating the target of {}",
                            desktop.count, plan.target_desktop_count
                        ),
                    ));
                    StepStatus::Failed
                }
                Err(error) => {
                    report.notices.push(Notice::error(
                        "desktopCountVerificationFailed",
                        format!(
                            "The created desktop count could not be verified: {}",
                            error.message
                        ),
                    ));
                    StepStatus::Failed
                }
            },
            Err(error) => {
                report
                    .notices
                    .push(Notice::error(error.code, error.message));
                StepStatus::Failed
            }
        }
    } else {
        StepStatus::Skipped
    };
    report.steps.push(step_result(
        ApplyStep::Desktops,
        desktop_status,
        desktop_started,
        None,
    ));
    emit_progress(
        app,
        preset.id,
        ApplyStep::Desktops,
        progress_status(desktop_status),
        None,
    );
    if desktop_status == StepStatus::Failed {
        report.status = ApplyStatus::NeedsAttention;
        return finish_report(&runtime, report);
    }

    let desktop_indices: HashMap<_, _> = preset
        .desktops
        .iter()
        .map(|desktop| (desktop.id, desktop.index))
        .collect();
    let mut used_windows = HashSet::new();
    let mut leave_open_native_ids = HashSet::new();
    let mut new_owned = Vec::new();
    let prior_owned = runtime.ownership.lock().windows.clone();
    let mut verification_targets = Vec::new();
    let mut reuse_duration = Duration::ZERO;
    let mut launch_duration = Duration::ZERO;
    let mut placement_duration = Duration::ZERO;
    let mut saw_reuse = false;
    let mut saw_launch = false;

    emit_progress(app, preset.id, ApplyStep::Reuse, "running", None);
    emit_progress(app, preset.id, ApplyStep::Launch, "running", None);
    emit_progress(app, preset.id, ApplyStep::Placement, "running", None);

    for item in preset.items.iter().filter(|item| item.enabled) {
        if cancelled(&runtime, &mut report) {
            break;
        }

        let reuse_started = Instant::now();
        let existing_match =
            WindowManager::find_match(item, &snapshot.windows, &used_windows).cloned();
        if item.lifecycle == LifecyclePolicy::LeaveOpen {
            reuse_duration += reuse_started.elapsed();
            if let Some(window) = existing_match {
                saw_reuse = true;
                used_windows.insert(window.native_id.clone());
                // LeaveOpen explicitly surrenders WorkSets ownership. The native ID protects the
                // window during this apply, while omitting it from new_owned protects it forever
                // on subsequent preset switches unless it is explicitly reused again.
                leave_open_native_ids.insert(window.native_id.clone());
                report.reused_count += 1;
                report.items.push(ItemApplyResult {
                    item_id: item.id,
                    display_name: item.display_name.clone(),
                    status: ItemApplyStatus::Reused,
                    native_window_id: Some(window.native_id),
                    message: Some("Existing window was left open unchanged".to_string()),
                });
            } else {
                report.items.push(ItemApplyResult {
                    item_id: item.id,
                    display_name: item.display_name.clone(),
                    status: ItemApplyStatus::Skipped,
                    native_window_id: None,
                    message: Some("No matching open window was found".to_string()),
                });
            }
            continue;
        }
        let mut matched = existing_match;
        if item.lifecycle == LifecyclePolicy::Restart {
            matched = None;
        }
        reuse_duration += reuse_started.elapsed();

        let (window, launched, launched_process_id) = if let Some(window) = matched {
            saw_reuse = true;
            (Some(window), false, None)
        } else if item.lifecycle == LifecyclePolicy::LeaveOpen {
            (None, false, None)
        } else {
            saw_launch = true;
            let launch_started = Instant::now();
            let prelaunch_native_ids: HashSet<String> = WindowManager::capture(&runtime.broker)
                .map(|current| {
                    current
                        .windows
                        .into_iter()
                        .map(|window| window.native_id)
                        .collect()
                })
                .unwrap_or_else(|_| {
                    snapshot
                        .windows
                        .iter()
                        .map(|window| window.native_id.clone())
                        .collect()
                });
            // Arm before launch so a fast single-instance app cannot emit SHOW/CREATE in the
            // gap between activation and the wait loop.
            let event_watcher = WindowManager::arm_window_events();
            let process = match WindowManager::launch(item) {
                Ok(process) => process,
                Err(error) => {
                    launch_duration += launch_started.elapsed();
                    report.items.push(ItemApplyResult {
                        item_id: item.id,
                        display_name: item.display_name.clone(),
                        status: ItemApplyStatus::Failed,
                        native_window_id: None,
                        message: Some(error.message),
                    });
                    continue;
                }
            };
            let process_id = process.process_id;
            let found = WindowManager::wait_for_window(
                item,
                &runtime.broker,
                WindowWaitRequest {
                    process_id,
                    timeout: Duration::from_millis(options.startup_timeout_ms.clamp(500, 60_000)),
                    cancelled: &runtime.cancelled,
                    excluded_native_ids: &used_windows,
                    prelaunch_native_ids: &prelaunch_native_ids,
                    event_watcher: &event_watcher,
                },
            );
            process.detach();
            launch_duration += launch_started.elapsed();
            match found {
                Ok(window) => (window, true, process_id),
                Err(error) => {
                    report.items.push(ItemApplyResult {
                        item_id: item.id,
                        display_name: item.display_name.clone(),
                        status: ItemApplyStatus::Failed,
                        native_window_id: None,
                        message: Some(error.message),
                    });
                    continue;
                }
            }
        };

        let Some(window) = window else {
            report.items.push(ItemApplyResult {
                item_id: item.id,
                display_name: item.display_name.clone(),
                status: ItemApplyStatus::Skipped,
                native_window_id: None,
                message: Some("No matching open window was found".to_string()),
            });
            continue;
        };

        if !window.manageable {
            used_windows.insert(window.native_id.clone());
            if let Some(previous) = retained_prior_ownership(&prior_owned, &window) {
                upsert_owned(&mut new_owned, previous);
            }
            report.items.push(ItemApplyResult {
                item_id: item.id,
                display_name: item.display_name.clone(),
                status: ItemApplyStatus::Skipped,
                native_window_id: Some(window.native_id),
                message: Some("The elevated or inaccessible window was left unchanged".to_string()),
            });
            continue;
        }

        used_windows.insert(window.native_id.clone());
        let expected_desktop = desktop_indices.get(&item.desktop_id).copied();
        if let (Some(native_id), Some(desktop_index)) = (window.native_value(), expected_desktop) {
            if let Err(error) = runtime.broker.move_window(native_id, desktop_index) {
                report
                    .notices
                    .push(Notice::warning(error.code, error.message));
            }
        }

        let place_started = Instant::now();
        let placement = WindowManager::place(&window, &item.placement, &snapshot.monitors);
        placement_duration += place_started.elapsed();
        let (status, message, expected_rectangle, topology_adjusted) = match placement {
            Ok(outcome) => {
                let status = if outcome.adjusted {
                    ItemApplyStatus::Adjusted
                } else if launched {
                    ItemApplyStatus::Launched
                } else {
                    ItemApplyStatus::Reused
                };
                (status, None, outcome.rectangle, outcome.adjusted)
            }
            Err(error) => (
                ItemApplyStatus::Failed,
                Some(error.message),
                crate::window::map_placement(&item.placement, &snapshot.monitors).0,
                false,
            ),
        };

        if status == ItemApplyStatus::Adjusted {
            report.adjusted_count += 1;
        }
        if launched {
            report.launched_count += 1;
        } else {
            report.reused_count += 1;
        }

        if item.lifecycle != LifecyclePolicy::LeaveOpen {
            let direct_process_launch = launched
                && launched_process_id == Some(window.process_id)
                && matches!(&item.launch_target, LaunchTarget::Executable { .. });
            let record = if launched {
                OwnedWindow {
                    native_id: window.native_id.clone(),
                    process_id: window.process_id,
                    preset_id: preset.id,
                    item_id: item.id,
                    executable_path: window.executable_path.clone(),
                    process_creation_time: window.process_creation_time,
                    started_by_worksets: direct_process_launch,
                }
            } else if let Some(mut previous) = retained_prior_ownership(&prior_owned, &window) {
                previous.preset_id = preset.id;
                previous.item_id = item.id;
                previous
            } else {
                OwnedWindow {
                    native_id: window.native_id.clone(),
                    process_id: window.process_id,
                    preset_id: preset.id,
                    item_id: item.id,
                    executable_path: window.executable_path.clone(),
                    process_creation_time: window.process_creation_time,
                    started_by_worksets: false,
                }
            };
            upsert_owned(&mut new_owned, record);
        }

        verification_targets.push(VerificationTarget {
            item_id: item.id,
            native_id: window.native_id.clone(),
            process_id: window.process_id,
            expected_rectangle,
            expected_state: item.placement.state,
            expected_desktop,
            same_topology: same_monitor_topology(&item.placement.monitor, &snapshot.monitors),
            topology_adjusted,
        });
        report.items.push(ItemApplyResult {
            item_id: item.id,
            display_name: item.display_name.clone(),
            status,
            native_window_id: Some(window.native_id),
            message,
        });
    }

    let reuse_status = if saw_reuse {
        StepStatus::Success
    } else {
        StepStatus::Skipped
    };
    let launch_status = if report
        .items
        .iter()
        .any(|item| item.status == ItemApplyStatus::Failed)
    {
        StepStatus::Warning
    } else if saw_launch {
        StepStatus::Success
    } else {
        StepStatus::Skipped
    };
    let placement_status = if report.items.iter().any(|item| {
        matches!(
            item.status,
            ItemApplyStatus::Failed | ItemApplyStatus::Adjusted
        )
    }) {
        StepStatus::Warning
    } else if report.items.is_empty() {
        StepStatus::Skipped
    } else {
        StepStatus::Success
    };
    report.steps.push(ApplyStepResult {
        step: ApplyStep::Reuse,
        status: reuse_status,
        duration_ms: duration_ms(reuse_duration),
        message: None,
    });
    report.steps.push(ApplyStepResult {
        step: ApplyStep::Launch,
        status: launch_status,
        duration_ms: duration_ms(launch_duration),
        message: None,
    });
    report.steps.push(ApplyStepResult {
        step: ApplyStep::Placement,
        status: placement_status,
        duration_ms: duration_ms(placement_duration),
        message: None,
    });
    emit_progress(
        app,
        preset.id,
        ApplyStep::Reuse,
        progress_status(reuse_status),
        None,
    );
    emit_progress(
        app,
        preset.id,
        ApplyStep::Launch,
        progress_status(launch_status),
        None,
    );
    emit_progress(
        app,
        preset.id,
        ApplyStep::Placement,
        progress_status(placement_status),
        None,
    );

    if cancelled(&runtime, &mut report) {
        merge_owned_excluding(&mut new_owned, prior_owned, &leave_open_native_ids);
        persist_ownership(&runtime, &mut report, new_owned);
        return finish_report(&runtime, report);
    }

    emit_progress(app, preset.id, ApplyStep::Verify, "running", None);
    let verify_started = Instant::now();
    snapshot = match WindowManager::capture(&runtime.broker) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            report
                .notices
                .push(Notice::error(error.code, error.message));
            report.steps.push(step_result(
                ApplyStep::Verify,
                StepStatus::Failed,
                verify_started,
                Some("The resulting window state could not be captured".to_string()),
            ));
            report.steps.push(ApplyStepResult {
                step: ApplyStep::CloseObsolete,
                status: StepStatus::Skipped,
                duration_ms: 0,
                message: Some("Closing was skipped because verification failed".to_string()),
            });
            emit_progress(
                app,
                preset.id,
                ApplyStep::Verify,
                "failed",
                Some("The resulting window state could not be captured"),
            );
            merge_owned_excluding(&mut new_owned, prior_owned, &leave_open_native_ids);
            persist_ownership(&runtime, &mut report, new_owned);
            return finish_report(&runtime, report);
        }
    };

    for target in &verification_targets {
        let Some(actual) = snapshot.windows.iter().find(|window| {
            window.native_id == target.native_id && window.process_id == target.process_id
        }) else {
            mark_item_failed(
                &mut report,
                target.item_id,
                "The target window disappeared during verification",
            );
            continue;
        };
        let differences = verification_differences(actual, target);
        if has_blocking_verification_difference(&differences) {
            mark_item_failed(
                &mut report,
                target.item_id,
                "The target window is on the wrong virtual desktop",
            );
        } else if target.topology_adjusted || !differences.is_empty() {
            let message = if differences.is_empty() {
                "Window was adapted to the available monitor topology".to_string()
            } else {
                format!("Window was adjusted: {}", differences.join(", "))
            };
            mark_item_adjusted(&mut report, target.item_id, &message);
        }
    }

    let verification_failed = report.items.iter().any(|item| {
        matches!(
            item.status,
            ItemApplyStatus::Failed | ItemApplyStatus::Skipped
        )
    });
    let verification_adjusted = report
        .items
        .iter()
        .any(|item| item.status == ItemApplyStatus::Adjusted);
    let verify_status = if verification_failed {
        StepStatus::Failed
    } else if verification_adjusted {
        StepStatus::Warning
    } else {
        StepStatus::Success
    };
    report.steps.push(step_result(
        ApplyStep::Verify,
        verify_status,
        verify_started,
        None,
    ));
    emit_progress(
        app,
        preset.id,
        ApplyStep::Verify,
        progress_status(verify_status),
        None,
    );

    emit_progress(app, preset.id, ApplyStep::CloseObsolete, "running", None);
    let close_started = Instant::now();
    let mut close_blocked = false;
    if verification_failed {
        merge_owned_excluding(&mut new_owned, prior_owned.clone(), &leave_open_native_ids);
        report.steps.push(step_result(
            ApplyStep::CloseObsolete,
            StepStatus::Skipped,
            close_started,
            Some("Closing was skipped until all target windows are verified".to_string()),
        ));
    } else {
        let retained: HashSet<String> = new_owned
            .iter()
            .map(|owned| owned.native_id.clone())
            .collect();
        for owned in prior_owned.iter().filter(|owned| {
            !retained.contains(owned.native_id.as_str())
                && !leave_open_native_ids.contains(owned.native_id.as_str())
        }) {
            if cancelled(&runtime, &mut report) {
                close_blocked = true;
                upsert_owned(&mut new_owned, owned.clone());
                continue;
            }
            let Some(window) = snapshot.windows.iter().find(|window| {
                window.native_id == owned.native_id && window.process_id == owned.process_id
            }) else {
                continue;
            };
            let force_is_unambiguous =
                force_close_is_unambiguous(owned, &snapshot.windows, &prior_owned);
            match close_managed_window(
                window,
                owned,
                options.force_close_managed && force_is_unambiguous,
                &runtime.cancelled,
            ) {
                Ok(ManagedCloseResult::Closed) => report.closed_count += 1,
                Ok(ManagedCloseResult::Blocked(message)) => {
                    close_blocked = true;
                    upsert_owned(&mut new_owned, owned.clone());
                    report
                        .notices
                        .push(Notice::warning("closeBlocked", message));
                }
                Err(error) => {
                    close_blocked = true;
                    upsert_owned(&mut new_owned, owned.clone());
                    report
                        .notices
                        .push(Notice::warning(error.code, error.message));
                }
            }
        }
        let status = if close_blocked {
            report.status = ApplyStatus::NeedsAttention;
            StepStatus::Warning
        } else {
            StepStatus::Success
        };
        report.steps.push(step_result(
            ApplyStep::CloseObsolete,
            status,
            close_started,
            None,
        ));
    }
    emit_progress(
        app,
        preset.id,
        ApplyStep::CloseObsolete,
        if verification_failed {
            "skipped"
        } else if close_blocked {
            "warning"
        } else {
            "success"
        },
        None,
    );

    if cancelled(&runtime, &mut report) {
        merge_owned_excluding(&mut new_owned, prior_owned, &leave_open_native_ids);
        persist_ownership(&runtime, &mut report, new_owned);
        return finish_report(&runtime, report);
    }

    let can_finalize_desktops = !verification_failed && !close_blocked;
    let desktop_removal_authorized =
        plan.foreign_windows.is_empty() || options.confirm_desktop_changes;
    if can_finalize_desktops
        && preset.desktop_policy == DesktopPolicy::ExactProtected
        && plan.current_desktop_count > plan.target_desktop_count
        && desktop_removal_authorized
    {
        let mut foreign_move_failed = false;
        for foreign in &plan.foreign_windows {
            if let Some(native_id) = parse_native_id(&foreign.native_id) {
                if let Err(error) = runtime
                    .broker
                    .move_window(native_id, plan.target_desktop_count.saturating_sub(1))
                {
                    foreign_move_failed = true;
                    report
                        .notices
                        .push(Notice::error(error.code, error.message));
                }
            } else {
                foreign_move_failed = true;
                report.notices.push(Notice::error(
                    "invalidWindowHandle",
                    "An unmanaged window could not be moved safely before desktop removal",
                ));
            }
        }
        let removal_result = if foreign_move_failed {
            Err(ApiError::new(
                "desktopRemovalSkipped",
                "Extra desktops were kept because an unmanaged window could not be moved",
            ))
        } else {
            runtime.broker.ensure_desktop_count(
                plan.target_desktop_count,
                preset.desktop_policy,
                true,
            )
        };
        match removal_result {
            Ok(_) => match runtime.broker.snapshot() {
                Ok(desktop) if desktop.count == plan.target_desktop_count => {}
                Ok(desktop) => report.notices.push(Notice::error(
                    "desktopCountVerificationFailed",
                    format!(
                        "Windows reports {} desktops after applying the exact target of {}",
                        desktop.count, plan.target_desktop_count
                    ),
                )),
                Err(error) => report.notices.push(Notice::error(
                    "desktopCountVerificationFailed",
                    format!(
                        "The exact desktop count could not be verified: {}",
                        error.message
                    ),
                )),
            },
            Err(error) => report
                .notices
                .push(Notice::error(error.code, error.message)),
        }
    }

    emit_progress(app, preset.id, ApplyStep::Activate, "running", None);
    let activate_started = Instant::now();
    let activate_status = match runtime.broker.switch_to(0) {
        Ok(()) => StepStatus::Success,
        Err(error) => {
            report
                .notices
                .push(Notice::warning(error.code, error.message));
            StepStatus::Warning
        }
    };
    report.steps.push(step_result(
        ApplyStep::Activate,
        activate_status,
        activate_started,
        None,
    ));
    emit_progress(
        app,
        preset.id,
        ApplyStep::Activate,
        progress_status(activate_status),
        None,
    );

    persist_ownership(&runtime, &mut report, new_owned);
    finish_report(&runtime, report)
}

#[tauri::command]
pub fn cancel_apply(runtime: State<'_, Arc<AppRuntime>>) -> ApiResult<()> {
    if !runtime.is_busy() {
        return Err(ApiError::new(
            "noApplyRunning",
            "There is no preset switch to cancel",
        ));
    }
    runtime.cancelled.store(true, Ordering::SeqCst);
    Ok(())
}

#[tauri::command]
pub fn get_settings(runtime: State<'_, Arc<AppRuntime>>) -> ApiResult<Settings> {
    runtime.store.load_settings()
}

#[tauri::command]
pub fn save_settings(
    app: AppHandle,
    runtime: State<'_, Arc<AppRuntime>>,
    settings: Settings,
) -> ApiResult<Settings> {
    settings.validate()?;
    let previous = effective_settings(&runtime)?;
    if !settings.telemetry_opt_in {
        // Delete first: if this fails, settings remain opted in and a retry will attempt the
        // deletion again instead of persisting a false state beside leftover queued data.
        runtime.store.clear_telemetry()?;
    }
    let autolaunch = app.autolaunch();
    if settings.run_at_startup {
        autolaunch
            .enable()
            .map_err(|error| ApiError::new("autostartUpdateFailed", error.to_string()))?;
    } else {
        autolaunch
            .disable()
            .map_err(|error| ApiError::new("autostartUpdateFailed", error.to_string()))?;
    }

    if let Err(error) = register_shortcuts(&app, &settings, &previous) {
        if previous.run_at_startup {
            let _ = autolaunch.enable();
        } else {
            let _ = autolaunch.disable();
        }
        return Err(error);
    }
    if let Err(error) = runtime.store.save_settings(&settings) {
        let _ = restore_shortcuts(&app, &previous);
        if previous.run_at_startup {
            let _ = autolaunch.enable();
        } else {
            let _ = autolaunch.disable();
        }
        return Err(error);
    }
    let _ = crate::refresh_tray(&app);
    Ok(settings)
}

#[tauri::command]
pub fn get_compatibility(runtime: State<'_, Arc<AppRuntime>>) -> CompatibilityInfo {
    runtime.broker.compatibility()
}

#[tauri::command]
pub fn get_diagnostics(runtime: State<'_, Arc<AppRuntime>>) -> ApiResult<Diagnostics> {
    Ok(Diagnostics {
        version: env!("CARGO_PKG_VERSION").to_string(),
        compatibility: runtime.broker.compatibility(),
        preset_count: runtime.store.list_summaries()?.len() as u32,
        busy: runtime.is_busy(),
        data_directory: runtime.store.root().to_string_lossy().into_owned(),
        last_apply_status: runtime.store.last_report()?.map(|report| report.status),
        broker_protocol_version: BROKER_PROTOCOL_VERSION,
        privacy: DiagnosticsPrivacy {
            telemetry_transport_enabled: false,
            telemetry_contains_names_paths_urls_or_titles: false,
        },
    })
}

#[tauri::command]
pub fn export_preset(runtime: State<'_, Arc<AppRuntime>>, preset_id: String) -> ApiResult<String> {
    let preset = runtime.store.load_preset(parse_id(&preset_id)?)?;
    serde_json::to_string_pretty(&preset)
        .map(|json| format!("{json}\n"))
        .map_err(|error| ApiError::new("presetExportFailed", error.to_string()))
}

#[tauri::command]
pub fn import_preset(
    app: AppHandle,
    runtime: State<'_, Arc<AppRuntime>>,
    json: String,
    commit: Option<bool>,
) -> ApiResult<ImportResult> {
    let (preset, warnings) = prepare_import(&runtime.store, &json)?;
    let committed = commit.unwrap_or(false);
    if committed {
        save_preset_internal(&app, &runtime, &preset)?;
    }
    Ok(ImportResult {
        preset,
        committed,
        warnings,
    })
}

fn prepare_import(store: &Store, json: &str) -> ApiResult<(PresetV1, Vec<Notice>)> {
    if json.len() > 2 * 1024 * 1024 {
        return Err(ApiError::new(
            "presetImportTooLarge",
            "Preset import cannot exceed 2 MiB",
        ));
    }
    let value: Value = serde_json::from_str(json)
        .map_err(|error| ApiError::new("invalidPresetJson", error.to_string()))?;
    let (value, mut warnings) = migrate_preset_value(value)?;
    let mut preset: PresetV1 = serde_json::from_value(value)
        .map_err(|error| ApiError::new("invalidPreset", error.to_string()))?;
    preset.validate()?;
    let existing = store.list_presets()?;
    if existing.iter().any(|candidate| candidate.id == preset.id) {
        warnings.push(Notice::warning(
            "presetIdReassigned",
            "A preset with this identifier already exists; a new identifier was assigned",
        ));
        preset.id = Uuid::new_v4();
        let now = Utc::now();
        preset.created_at = now;
        preset.updated_at = now;
    }
    Ok((preset, warnings))
}

#[tauri::command]
pub fn get_preset_schema() -> Value {
    serde_json::to_value(schemars::schema_for!(PresetV1)).unwrap_or(Value::Null)
}

#[tauri::command]
pub async fn check_for_updates(runtime: State<'_, Arc<AppRuntime>>) -> ApiResult<UpdateInfo> {
    let runtime = Arc::clone(runtime.inner());
    run_blocking(move || Ok(check_for_updates_blocking(&runtime))).await
}

fn check_for_updates_blocking(runtime: &AppRuntime) -> UpdateInfo {
    if let Some(cached) = runtime.update_cache.lock().clone() {
        if update_cache_is_fresh(cached.checked_at, Utc::now()) {
            return cached;
        }
    }
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    let mut info = UpdateInfo {
        checked_at: Utc::now(),
        current_version: current_version.clone(),
        latest_version: None,
        available: false,
        download_url: None,
        error: None,
    };
    let response = ureq::get(
        "https://api.github.com/repos/Tracht-Digital-Solutions/WorkSets/releases?per_page=10",
    )
    .timeout(Duration::from_secs(12))
    .set("Accept", "application/vnd.github+json")
    .set(
        "User-Agent",
        concat!("WorkSets/", env!("CARGO_PKG_VERSION")),
    )
    .call();
    match response {
        Ok(response) => match response.into_json::<Vec<GithubRelease>>() {
            Ok(releases) => {
                if let Some(release) = releases
                    .into_iter()
                    .find(|release| release.prerelease && !release.draft)
                {
                    let latest = release.tag_name.trim_start_matches('v').to_string();
                    let newer = match (
                        semver::Version::parse(&current_version),
                        semver::Version::parse(&latest),
                    ) {
                        (Ok(current), Ok(latest)) => latest > current,
                        _ => latest != current_version,
                    };
                    info.latest_version = Some(latest);
                    info.available = newer;
                    info.download_url = Some(release.html_url);
                }
            }
            Err(error) => info.error = Some(format!("Update response was invalid: {error}")),
        },
        Err(error) => info.error = Some(format!("Update check failed: {error}")),
    }
    if let Err(error) = runtime.store.save_update_cache(&info) {
        let persistence_error = format!("Update cache could not be saved: {}", error.message);
        match &mut info.error {
            Some(message) => {
                message.push_str("; ");
                message.push_str(&persistence_error);
            }
            None => info.error = Some(persistence_error),
        }
    }
    *runtime.update_cache.lock() = Some(info.clone());
    info
}

fn update_cache_is_fresh(checked_at: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    let age = now.signed_duration_since(checked_at);
    age >= ChronoDuration::zero() && age < ChronoDuration::days(1)
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: String,
    prerelease: bool,
    draft: bool,
}

#[derive(Debug)]
struct VerificationTarget {
    item_id: Uuid,
    native_id: String,
    process_id: u32,
    expected_rectangle: RectI32,
    expected_state: WindowState,
    expected_desktop: Option<u32>,
    same_topology: bool,
    topology_adjusted: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum ManagedCloseResult {
    Closed,
    Blocked(String),
}

fn verification_differences(
    actual: &CapturedWindow,
    target: &VerificationTarget,
) -> Vec<&'static str> {
    let tolerance = if target.same_topology { 2 } else { 16 };
    let expected = target.expected_rectangle;
    let observed = actual.placement.physical;
    let mut differences = Vec::new();
    if observed.x.abs_diff(expected.x) > tolerance
        || observed.y.abs_diff(expected.y) > tolerance
        || observed.width.abs_diff(expected.width) > tolerance
        || observed.height.abs_diff(expected.height) > tolerance
    {
        differences.push("geometry");
    }
    if actual.placement.state != target.expected_state {
        differences.push("window state");
    }
    if target
        .expected_desktop
        .is_some_and(|expected| actual.desktop_index != Some(expected))
    {
        differences.push("virtual desktop");
    }
    differences
}

fn has_blocking_verification_difference(differences: &[&str]) -> bool {
    differences.contains(&"virtual desktop")
}

fn same_monitor_topology(requested: &MonitorRef, monitors: &[MonitorRef]) -> bool {
    monitors.iter().any(|monitor| {
        monitor.id == requested.id
            && monitor.bounds == requested.bounds
            && monitor.work_area == requested.work_area
            && monitor.dpi_x == requested.dpi_x
            && monitor.dpi_y == requested.dpi_y
    })
}

fn mark_item_failed(report: &mut ApplyReport, item_id: Uuid, message: &str) {
    if let Some(item) = report.items.iter_mut().find(|item| item.item_id == item_id) {
        item.status = ItemApplyStatus::Failed;
        item.message = Some(message.to_string());
    }
    report
        .notices
        .push(Notice::error("windowVerificationFailed", message));
}

fn mark_item_adjusted(report: &mut ApplyReport, item_id: Uuid, message: &str) {
    if let Some(item) = report.items.iter_mut().find(|item| item.item_id == item_id) {
        if !matches!(
            item.status,
            ItemApplyStatus::Failed | ItemApplyStatus::Skipped
        ) {
            if item.status != ItemApplyStatus::Adjusted {
                report.adjusted_count += 1;
            }
            item.status = ItemApplyStatus::Adjusted;
            item.message = Some(message.to_string());
            report
                .notices
                .push(Notice::warning("windowPlacementAdjusted", message));
        }
    }
}

fn force_close_is_unambiguous(
    owned: &OwnedWindow,
    windows: &[CapturedWindow],
    managed: &[OwnedWindow],
) -> bool {
    owned.started_by_worksets
        && owned.executable_path.is_some()
        && owned.process_creation_time.is_some()
        && windows
            .iter()
            .filter(|window| window.process_id == owned.process_id)
            .all(|window| {
                managed.iter().any(|candidate| {
                    candidate.started_by_worksets
                        && candidate.process_id == window.process_id
                        && candidate.process_creation_time == window.process_creation_time
                        && candidate.native_id == window.native_id
                })
            })
}

fn close_managed_window(
    window: &CapturedWindow,
    owned: &OwnedWindow,
    allow_force: bool,
    cancelled: &AtomicBool,
) -> ApiResult<ManagedCloseResult> {
    let proof = OwnedWindowProof {
        native_id: owned.native_id.clone(),
        process_id: owned.process_id,
        executable_path: owned.executable_path.clone(),
        process_creation_time: owned.process_creation_time,
        started_by_worksets: owned.started_by_worksets,
    };
    match WindowManager::close_owned(window, &proof, false) {
        Ok(CloseOutcome::AlreadyClosed) => return Ok(ManagedCloseResult::Closed),
        Ok(CloseOutcome::Requested) => {
            if wait_until_closed(window, cancelled, Duration::from_millis(900)) {
                return Ok(ManagedCloseResult::Closed);
            }
        }
        Ok(CloseOutcome::Forced) => return Ok(ManagedCloseResult::Closed),
        Err(error) if error.code != "windowNotResponding" => return Err(error),
        Err(_) => {}
    }

    if !allow_force {
        return Ok(ManagedCloseResult::Blocked(
            "The managed window stayed open, possibly because it is asking to save work"
                .to_string(),
        ));
    }
    WindowManager::close_owned(window, &proof, true)?;
    if wait_until_closed(window, cancelled, Duration::from_millis(900)) {
        Ok(ManagedCloseResult::Closed)
    } else {
        Ok(ManagedCloseResult::Blocked(
            "The WorkSets-started process could not be closed".to_string(),
        ))
    }
}

fn wait_until_closed(window: &CapturedWindow, cancelled: &AtomicBool, timeout: Duration) -> bool {
    let started = Instant::now();
    while started.elapsed() < timeout && !cancelled.load(Ordering::SeqCst) {
        if !WindowManager::is_alive(window) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    !WindowManager::is_alive(window)
}

fn retained_prior_ownership(
    prior_owned: &[OwnedWindow],
    window: &CapturedWindow,
) -> Option<OwnedWindow> {
    prior_owned
        .iter()
        .find(|owned| owned.native_id == window.native_id && owned.process_id == window.process_id)
        .cloned()
}

fn upsert_owned(windows: &mut Vec<OwnedWindow>, record: OwnedWindow) {
    if let Some(existing) = windows.iter_mut().find(|existing| {
        existing.native_id == record.native_id && existing.process_id == record.process_id
    }) {
        *existing = record;
    } else {
        windows.push(record);
    }
}

fn merge_owned(target: &mut Vec<OwnedWindow>, source: Vec<OwnedWindow>) {
    for record in source {
        upsert_owned(target, record);
    }
}

fn merge_owned_excluding(
    target: &mut Vec<OwnedWindow>,
    source: Vec<OwnedWindow>,
    excluded_native_ids: &HashSet<String>,
) {
    merge_owned(
        target,
        source
            .into_iter()
            .filter(|owned| !excluded_native_ids.contains(&owned.native_id))
            .collect(),
    );
}

fn persist_ownership(runtime: &AppRuntime, report: &mut ApplyReport, windows: Vec<OwnedWindow>) {
    let mut ownership = runtime.ownership.lock();
    ownership.windows = windows;
    if let Err(error) = runtime.store.save_runtime_ownership(&ownership) {
        report
            .notices
            .push(Notice::warning(error.code, error.message));
    }
}

fn duration_ms(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}

fn desktop_count_matches_policy(policy: DesktopPolicy, actual: u32, target: u32) -> bool {
    match policy {
        DesktopPolicy::ExactProtected => actual == target,
        DesktopPolicy::ManagedOnly => actual >= target,
    }
}

fn progress_status(status: StepStatus) -> &'static str {
    match status {
        StepStatus::Success => "success",
        StepStatus::Skipped => "skipped",
        StepStatus::Warning => "warning",
        StepStatus::Failed => "failed",
    }
}

fn emit_progress(
    app: &AppHandle,
    preset_id: Uuid,
    step: ApplyStep,
    status: &'static str,
    message: Option<&str>,
) {
    let _ = app.emit(
        "worksets-apply-progress",
        serde_json::json!({
            "presetId": preset_id,
            "step": step,
            "status": status,
            "message": message,
        }),
    );
}

async fn run_blocking<T, F>(task: F) -> ApiResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> ApiResult<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(task)
        .await
        .map_err(|error| ApiError::new("workerFailed", error.to_string()))?
}

fn build_apply_plan(runtime: &AppRuntime, preset: &PresetV1) -> ApiResult<ApplyPlan> {
    let mut preflight_warnings = Vec::new();
    let desktop = match runtime.broker.snapshot() {
        Ok(snapshot) => snapshot,
        Err(error) => {
            preflight_warnings.push(Notice::warning(error.code, error.message));
            DesktopSnapshot {
                count: 1,
                current_index: 0,
            }
        }
    };
    let snapshot = WindowManager::capture(&runtime.broker)?;
    let mut used = HashSet::new();
    let mut matched_ids = Vec::new();
    let mut warned_monitors = HashSet::new();
    for item in preset.items.iter().filter(|item| item.enabled) {
        let matched = WindowManager::find_match(item, &snapshot.windows, &used);
        if let Some(window) = matched {
            if item.lifecycle != LifecyclePolicy::Restart {
                used.insert(window.native_id.clone());
            }
            matched_ids.push(item.id);
            if !window.manageable {
                preflight_warnings.push(Notice::warning(
                    "windowNotManageable",
                    format!(
                        "'{}' is elevated or otherwise inaccessible and will be skipped",
                        item.display_name
                    ),
                ));
            }
        }
        let needs_launch = item.lifecycle == LifecyclePolicy::Restart
            || (matched.is_none() && item.lifecycle != LifecyclePolicy::LeaveOpen);
        if needs_launch {
            if let Some(issue) = launch_target_issue(item) {
                preflight_warnings.push(issue);
            }
        }
        let monitor_available = snapshot.monitors.iter().any(|monitor| {
            monitor.id == item.placement.monitor.id
                || monitor
                    .device_name
                    .eq_ignore_ascii_case(&item.placement.monitor.device_name)
        });
        if !monitor_available && warned_monitors.insert(item.placement.monitor.id.clone()) {
            preflight_warnings.push(Notice::warning(
                "monitorFallbackRequired",
                format!(
                    "Monitor '{}' is unavailable; windows will be kept visible on the best fallback",
                    item.placement.monitor.device_name
                ),
            ));
        }
    }
    let target_count = preset.desktops.len() as u32;
    let foreign_windows =
        if preset.desktop_policy == DesktopPolicy::ExactProtected && desktop.count > target_count {
            snapshot
                .windows
                .iter()
                .filter(|window| {
                    window
                        .desktop_index
                        .is_some_and(|index| index >= target_count)
                        && !used.contains(&window.native_id)
                })
                .map(window_reference)
                .collect()
        } else {
            Vec::new()
        };
    let mut plan = ApplyPlan::build(preset, desktop.count, foreign_windows, &matched_ids);

    let compatibility = runtime.broker.compatibility();
    if !compatibility.supported {
        preflight_warnings.push(Notice::error(
            "unsupportedWindowsBuild",
            "This Windows build is outside the supported WorkSets range",
        ));
    }
    if compatibility.virtual_desktops.experimental {
        preflight_warnings.push(Notice::warning(
            "experimentalDesktopProvider",
            "Virtual desktop control is experimental on this Windows build",
        ));
        if !runtime
            .experimental_desktop_acknowledged
            .load(Ordering::SeqCst)
        {
            plan.requires_confirmation = true;
        }
    }
    if compatibility.virtual_desktops.mutations_disabled {
        preflight_warnings.push(Notice::warning(
            "desktopCircuitBreakerOpen",
            "Virtual desktop changes are disabled for this session after a provider error",
        ));
    }
    if desktop.count < plan.target_desktop_count && !compatibility.virtual_desktops.can_create {
        preflight_warnings.push(Notice::error(
            "desktopCreationUnavailable",
            "The required virtual desktops cannot be created on this Windows build",
        ));
    }
    if desktop.count > plan.target_desktop_count
        && preset.desktop_policy == DesktopPolicy::ExactProtected
        && !compatibility.virtual_desktops.can_remove
    {
        preflight_warnings.push(Notice::error(
            "desktopRemovalUnavailable",
            "Extra virtual desktops cannot be removed on this Windows build",
        ));
    }

    if let Some(hotkey) = preset.hotkey.as_deref() {
        let normalized = hotkey.trim().to_ascii_lowercase();
        let settings = effective_settings(runtime)?;
        let conflict = settings.capture_hotkey.trim().eq_ignore_ascii_case(hotkey)
            || settings.hotkeys.iter().any(|(id, candidate)| {
                *id != preset.id && candidate.trim().to_ascii_lowercase() == normalized
            });
        if conflict {
            preflight_warnings.push(Notice::warning(
                "hotkeyConflict",
                "The preset shortcut conflicts with another WorkSets shortcut",
            ));
        }
    }
    plan.warnings.extend(preflight_warnings);
    Ok(plan)
}

fn launch_target_issue(item: &LaunchItem) -> Option<Notice> {
    match &item.launch_target {
        LaunchTarget::Executable {
            path,
            working_directory,
            ..
        } => {
            if !std::path::Path::new(path).is_file() {
                Some(Notice::error(
                    "launchTargetMissing",
                    format!("Executable for '{}' is missing", item.display_name),
                ))
            } else if working_directory
                .as_deref()
                .is_some_and(|directory| !std::path::Path::new(directory).is_dir())
            {
                Some(Notice::error(
                    "workingDirectoryMissing",
                    format!("Working directory for '{}' is missing", item.display_name),
                ))
            } else {
                None
            }
        }
        LaunchTarget::File { path } if !std::path::Path::new(path).is_file() => {
            Some(Notice::error(
                "launchTargetMissing",
                format!("File for '{}' is missing", item.display_name),
            ))
        }
        LaunchTarget::Url { url }
            if url::Url::parse(url)
                .ok()
                .is_none_or(|url| !matches!(url.scheme(), "http" | "https")) =>
        {
            Some(Notice::error(
                "invalidUrl",
                format!("URL for '{}' is invalid", item.display_name),
            ))
        }
        _ => None,
    }
}

fn window_reference(window: &CapturedWindow) -> WindowReference {
    WindowReference {
        native_id: window.native_id.clone(),
        title: window.title.clone(),
        application: window.application.clone(),
        desktop_index: window.desktop_index,
    }
}

fn cancelled(runtime: &AppRuntime, report: &mut ApplyReport) -> bool {
    if runtime.cancelled.load(Ordering::SeqCst) {
        if report.status != ApplyStatus::Cancelled {
            report.status = ApplyStatus::Cancelled;
            report.notices.push(Notice::warning(
                "applyCancelled",
                "Preset switch was cancelled",
            ));
        }
        true
    } else {
        false
    }
}

fn finish_report(runtime: &AppRuntime, mut report: ApplyReport) -> ApiResult<ApplyReport> {
    report.finish();
    runtime.store.save_report(&report)?;
    // Local, opt-in diagnostics must never turn a completed apply into a command failure.
    if let Ok(settings) = runtime.store.load_settings() {
        let status = match report.status {
            ApplyStatus::Success => TelemetryApplyStatus::Success,
            ApplyStatus::Partial => TelemetryApplyStatus::Partial,
            ApplyStatus::NeedsAttention => TelemetryApplyStatus::NeedsAttention,
            ApplyStatus::Cancelled => TelemetryApplyStatus::Cancelled,
            ApplyStatus::Failed => TelemetryApplyStatus::Failed,
        };
        let elapsed = report
            .finished_at
            .signed_duration_since(report.started_at)
            .num_milliseconds()
            .max(0) as u64;
        let _ = runtime.store.record_telemetry(
            settings.telemetry_opt_in,
            TelemetryEvent::new(TelemetryEventKind::ApplyCompleted {
                status,
                item_count: report.items.len() as u32,
                notice_count: report.notices.len() as u32,
                duration_ms: elapsed,
            }),
        );
    }
    Ok(report)
}

fn step_result(
    step: ApplyStep,
    status: StepStatus,
    started: Instant,
    message: Option<String>,
) -> ApplyStepResult {
    ApplyStepResult {
        step,
        status,
        duration_ms: started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
        message,
    }
}

fn parse_id(value: &str) -> ApiResult<Uuid> {
    Uuid::parse_str(value).map_err(|_| ApiError::new("invalidPresetId", "Preset ID is invalid"))
}

fn parse_native_id(value: &str) -> Option<isize> {
    isize::from_str_radix(value.trim_start_matches("0x"), 16).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn telemetry_diagnostics_never_claim_transport_or_sensitive_fields() {
        let privacy = DiagnosticsPrivacy {
            telemetry_transport_enabled: false,
            telemetry_contains_names_paths_urls_or_titles: false,
        };
        assert!(!privacy.telemetry_transport_enabled);
        assert!(!privacy.telemetry_contains_names_paths_urls_or_titles);
    }

    #[test]
    fn app_state_uses_canonical_last_apply_report_field() {
        use crate::desktop::{
            DesktopCapability, DesktopProviderKind, OsBuildInfo, WindowCapability,
        };

        let state = AppState {
            version: "0.1.0-alpha.1".to_string(),
            settings: Settings::default(),
            presets: Vec::new(),
            compatibility: CompatibilityInfo {
                os: OsBuildInfo {
                    major: 10,
                    minor: 0,
                    build: 26200,
                    revision: 1,
                    architecture: "x86_64".to_string(),
                },
                supported: true,
                virtual_desktops: DesktopCapability {
                    provider: DesktopProviderKind::Windows11_25H2,
                    provider_version: "fixture".to_string(),
                    experimental: false,
                    can_read: true,
                    can_create: true,
                    can_remove: true,
                    can_move_windows: true,
                    mutations_disabled: false,
                    last_error: None,
                    isolated_process: true,
                },
                windows: WindowCapability {
                    can_capture: true,
                    can_launch: true,
                    can_place: true,
                    can_close_managed: true,
                },
                warnings: Vec::new(),
            },
            busy: false,
            data_directory: "fixture".to_string(),
            last_apply_report: Some(ApplyReport::new(Uuid::new_v4())),
        };
        let json = serde_json::to_value(state).unwrap();
        assert!(json.get("lastApplyReport").is_some());
        assert!(json.get("lastReport").is_none());
        assert!(json.get("last_apply_report").is_none());
    }

    #[test]
    fn persistent_update_cache_uses_strict_24_hour_window() {
        let now = Utc::now();
        assert!(update_cache_is_fresh(now - ChronoDuration::hours(23), now));
        assert!(!update_cache_is_fresh(now - ChronoDuration::hours(24), now));
        assert!(!update_cache_is_fresh(
            now + ChronoDuration::minutes(1),
            now
        ));
    }

    #[test]
    fn desktop_creation_verification_respects_policy() {
        assert!(desktop_count_matches_policy(
            DesktopPolicy::ExactProtected,
            3,
            3
        ));
        assert!(!desktop_count_matches_policy(
            DesktopPolicy::ExactProtected,
            4,
            3
        ));
        assert!(desktop_count_matches_policy(
            DesktopPolicy::ManagedOnly,
            4,
            3
        ));
        assert!(!desktop_count_matches_policy(
            DesktopPolicy::ManagedOnly,
            2,
            3
        ));
    }

    #[test]
    fn virtual_desktop_verification_mismatch_blocks_obsolete_close() {
        assert!(has_blocking_verification_difference(&[
            "geometry",
            "virtual desktop"
        ]));
        assert!(!has_blocking_verification_difference(&[
            "geometry",
            "window state"
        ]));
    }

    #[test]
    fn import_preview_does_not_commit() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path().join("WorkSets")).unwrap();
        let source = PresetV1::empty("Preview");
        let json = serde_json::to_string(&source).unwrap();
        let (preview, warnings) = prepare_import(&store, &json).unwrap();
        assert_eq!(preview.id, source.id);
        assert!(warnings.is_empty());
        assert!(store.list_presets().unwrap().is_empty());
    }

    #[test]
    fn import_preview_reassigns_conflicting_id() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path().join("WorkSets")).unwrap();
        let source = PresetV1::empty("Existing");
        store.save_preset(&source).unwrap();
        let json = serde_json::to_string(&source).unwrap();
        let (preview, warnings) = prepare_import(&store, &json).unwrap();
        assert_ne!(preview.id, source.id);
        assert!(warnings
            .iter()
            .any(|notice| notice.code == "presetIdReassigned"));
        assert_eq!(store.list_presets().unwrap().len(), 1);
    }

    #[test]
    fn managed_reuse_is_graceful_only_while_unmanaged_is_absent() {
        let preset_id = Uuid::new_v4();
        let managed_reuse = OwnedWindow {
            native_id: "0x10".to_string(),
            process_id: 42,
            preset_id,
            item_id: Uuid::new_v4(),
            executable_path: Some("C:\\Tools\\editor.exe".to_string()),
            process_creation_time: Some(123),
            started_by_worksets: false,
        };
        let unrelated_native_id = "0x20";
        let retained = HashSet::<String>::new();
        let obsolete: Vec<_> = [managed_reuse.clone()]
            .into_iter()
            .filter(|owned| !retained.contains(&owned.native_id))
            .collect();
        assert_eq!(obsolete.len(), 1, "explicit reuse is gracefully managed");
        assert!(
            !managed_reuse.started_by_worksets,
            "reuse can never be forced"
        );
        assert!(!obsolete
            .iter()
            .any(|owned| owned.native_id == unrelated_native_id));
    }

    #[test]
    fn leave_open_surrenders_ownership_across_future_switches() {
        let managed = OwnedWindow {
            native_id: "0x10".to_string(),
            process_id: 42,
            preset_id: Uuid::new_v4(),
            item_id: Uuid::new_v4(),
            executable_path: Some(r"C:\Tools\editor.exe".to_string()),
            process_creation_time: Some(123),
            started_by_worksets: true,
        };
        let leave_open_ids = [managed.native_id.clone()].into_iter().collect();
        let mut after_leave_open = Vec::new();
        merge_owned_excluding(
            &mut after_leave_open,
            vec![managed.clone()],
            &leave_open_ids,
        );
        assert!(after_leave_open.is_empty());

        let next_preset_ids = HashSet::<String>::new();
        let obsolete_on_next_switch: Vec<_> = after_leave_open
            .iter()
            .filter(|owned| !next_preset_ids.contains(&owned.native_id))
            .collect();
        assert!(obsolete_on_next_switch.is_empty());
    }
}
