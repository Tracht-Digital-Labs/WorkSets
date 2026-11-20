use crate::domain::{PresetSummary, PresetV1, Settings};
use crate::error::{ApiError, ApiResult};
use crate::planner::ApplyReport;
use atomic_write_file::AtomicWriteFile;
use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::VecDeque;
use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use uuid::Uuid;

const TELEMETRY_MAX_EVENTS: usize = 200;
const TELEMETRY_MAX_BYTES: usize = 256 * 1024;

#[derive(Debug)]
pub struct Store {
    root: PathBuf,
    io_lock: Mutex<()>,
}

impl Store {
    pub fn open_default() -> ApiResult<Self> {
        let local_app_data = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("APPDATA").map(PathBuf::from))
            .ok_or_else(|| {
                ApiError::new(
                    "localAppDataUnavailable",
                    "Windows local application data directory is unavailable",
                )
            })?;
        Self::open(
            local_app_data
                .join("Tracht Digital Solutions")
                .join("WorkSets"),
        )
    }

    pub fn open(root: PathBuf) -> ApiResult<Self> {
        for directory in [
            root.clone(),
            root.join("presets"),
            root.join("reports"),
            root.join("backups"),
        ] {
            fs::create_dir_all(&directory)
                .map_err(|error| ApiError::io("Could not create data directory", error))?;
        }
        Ok(Self {
            root,
            io_lock: Mutex::new(()),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn load_settings(&self) -> ApiResult<Settings> {
        let _guard = self.io_lock.lock();
        let path = self.root.join("settings.json");
        if !path.exists() {
            return Ok(Settings::default());
        }
        read_json(&path, "settings")
    }

    pub fn save_settings(&self, settings: &Settings) -> ApiResult<()> {
        settings.validate()?;
        let _guard = self.io_lock.lock();
        atomic_json(&self.root, &self.root.join("settings.json"), settings)
    }

    pub fn list_presets(&self) -> ApiResult<Vec<PresetV1>> {
        let _guard = self.io_lock.lock();
        let mut presets = Vec::new();
        let entries = fs::read_dir(self.root.join("presets"))
            .map_err(|error| ApiError::io("Could not list presets", error))?;
        for entry in entries {
            let entry =
                entry.map_err(|error| ApiError::io("Could not read preset entry", error))?;
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            match read_json::<PresetV1>(&path, "preset") {
                Ok(preset) if preset.validate().is_ok() => presets.push(preset),
                Ok(_) | Err(_) => {
                    // A corrupt file is preserved for recovery and intentionally omitted from UI.
                }
            }
        }
        presets.sort_by_key(|preset| std::cmp::Reverse(preset.updated_at));
        Ok(presets)
    }

    pub fn list_summaries(&self) -> ApiResult<Vec<PresetSummary>> {
        Ok(self
            .list_presets()?
            .into_iter()
            .map(|preset| preset.summary())
            .collect())
    }

    pub fn load_preset(&self, id: Uuid) -> ApiResult<PresetV1> {
        let _guard = self.io_lock.lock();
        let path = self.preset_path(id);
        if !path.exists() {
            return Err(ApiError::new(
                "presetNotFound",
                format!("Preset {id} does not exist"),
            ));
        }
        let preset: PresetV1 = read_json(&path, "preset")?;
        preset.validate()?;
        Ok(preset)
    }

    pub fn save_preset(&self, preset: &PresetV1) -> ApiResult<()> {
        preset.validate()?;
        let _guard = self.io_lock.lock();
        atomic_json(&self.root, &self.preset_path(preset.id), preset)
    }

    pub fn delete_preset(&self, id: Uuid) -> ApiResult<()> {
        let _guard = self.io_lock.lock();
        let path = self.preset_path(id);
        if !path.exists() {
            return Err(ApiError::new(
                "presetNotFound",
                format!("Preset {id} does not exist"),
            ));
        }
        fs::remove_file(path).map_err(|error| ApiError::io("Could not delete preset", error))
    }

    pub fn save_report(&self, report: &ApplyReport) -> ApiResult<()> {
        let _guard = self.io_lock.lock();
        let path = self
            .root
            .join("reports")
            .join(format!("{}.json", report.id));
        atomic_json(&self.root, &path, report)?;
        self.prune_reports(50)
    }

    pub fn last_report(&self) -> ApiResult<Option<ApplyReport>> {
        let _guard = self.io_lock.lock();
        let report_dir = self.root.join("reports");
        let mut candidates: Vec<_> = fs::read_dir(report_dir)
            .map_err(|error| ApiError::io("Could not list apply reports", error))?
            .filter_map(Result::ok)
            .filter(|entry| {
                entry.path().extension().and_then(|value| value.to_str()) == Some("json")
            })
            .collect();
        candidates.sort_by_key(|entry| {
            entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .ok()
        });
        let Some(entry) = candidates.last() else {
            return Ok(None);
        };
        read_json(&entry.path(), "apply report").map(Some)
    }

    pub fn save_runtime_ownership(&self, state: &RuntimeOwnership) -> ApiResult<()> {
        let _guard = self.io_lock.lock();
        atomic_json(&self.root, &self.root.join("runtime.json"), state)
    }

    pub fn load_update_cache<T: DeserializeOwned>(&self) -> ApiResult<Option<T>> {
        let _guard = self.io_lock.lock();
        let path = self.root.join("update-cache.json");
        if !path.exists() {
            return Ok(None);
        }
        read_json(&path, "update cache").map(Some)
    }

    pub fn save_update_cache<T: Serialize>(&self, value: &T) -> ApiResult<()> {
        let _guard = self.io_lock.lock();
        atomic_json(&self.root, &self.root.join("update-cache.json"), value)
    }

    /// Stores only a closed, non-sensitive event shape. There is deliberately no transport.
    pub fn record_telemetry(&self, enabled: bool, event: TelemetryEvent) -> ApiResult<()> {
        if !enabled {
            return Ok(());
        }
        let _guard = self.io_lock.lock();
        let path = self.root.join("telemetry.json");
        let mut queue = if path.exists() {
            read_json::<TelemetryQueue>(&path, "telemetry queue").unwrap_or_default()
        } else {
            TelemetryQueue::default()
        };
        queue.events.push_back(event);
        trim_telemetry_queue(&mut queue)?;
        atomic_json(&self.root, &path, &queue)
    }

    pub fn clear_telemetry(&self) -> ApiResult<()> {
        let _guard = self.io_lock.lock();
        for path in [
            self.root.join("telemetry.json"),
            self.root.join("backups").join("telemetry.json.bak"),
        ] {
            if path.exists() {
                fs::remove_file(path)
                    .map_err(|error| ApiError::io("Could not clear telemetry queue", error))?;
            }
        }
        Ok(())
    }

    fn preset_path(&self, id: Uuid) -> PathBuf {
        self.root.join("presets").join(format!("{id}.json"))
    }

    fn prune_reports(&self, keep: usize) -> ApiResult<()> {
        let mut entries: Vec<_> = fs::read_dir(self.root.join("reports"))
            .map_err(|error| ApiError::io("Could not list apply reports", error))?
            .filter_map(Result::ok)
            .filter(|entry| {
                entry.path().extension().and_then(|value| value.to_str()) == Some("json")
            })
            .collect();
        entries.sort_by_key(|entry| {
            entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .ok()
        });
        let remove_count = entries.len().saturating_sub(keep);
        for entry in entries.into_iter().take(remove_count) {
            fs::remove_file(entry.path())
                .map_err(|error| ApiError::io("Could not prune apply report", error))?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryEvent {
    pub occurred_at: DateTime<Utc>,
    #[serde(flatten)]
    pub kind: TelemetryEventKind,
}

impl TelemetryEvent {
    pub fn new(kind: TelemetryEventKind) -> Self {
        Self {
            occurred_at: Utc::now(),
            kind,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "event", rename_all = "camelCase")]
pub enum TelemetryEventKind {
    AppStarted,
    CaptureCompleted {
        window_count: u32,
        desktop_count: u32,
        monitor_count: u32,
        warning_count: u32,
    },
    ApplyCompleted {
        status: TelemetryApplyStatus,
        item_count: u32,
        notice_count: u32,
        duration_ms: u64,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TelemetryApplyStatus {
    Success,
    Partial,
    NeedsAttention,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct TelemetryQueue {
    #[serde(default)]
    events: VecDeque<TelemetryEvent>,
}

fn trim_telemetry_queue(queue: &mut TelemetryQueue) -> ApiResult<()> {
    while queue.events.len() > TELEMETRY_MAX_EVENTS {
        queue.events.pop_front();
    }
    while serialized_size(queue)? > TELEMETRY_MAX_BYTES && !queue.events.is_empty() {
        queue.events.pop_front();
    }
    Ok(())
}

fn serialized_size<T: Serialize>(value: &T) -> ApiResult<usize> {
    serde_json::to_vec(value)
        .map(|bytes| bytes.len())
        .map_err(|error| ApiError::new("jsonEncodingFailed", error.to_string()))
}

fn read_json<T: DeserializeOwned>(path: &Path, label: &str) -> ApiResult<T> {
    let bytes =
        fs::read(path).map_err(|error| ApiError::io(&format!("Could not read {label}"), error))?;
    serde_json::from_slice(&bytes).map_err(|error| {
        ApiError::new(
            "invalidStoredJson",
            format!("Stored {label} is invalid: {error}"),
        )
    })
}

fn atomic_json<T: Serialize>(root: &Path, path: &Path, value: &T) -> ApiResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| ApiError::io("Could not create storage directory", error))?;
    }
    if path.exists() {
        let mut backup_name: OsString = path.file_name().unwrap_or_default().to_os_string();
        backup_name.push(".bak");
        let backup_path = root.join("backups").join(backup_name);
        fs::copy(path, backup_path)
            .map_err(|error| ApiError::io("Could not create backup", error))?;
    }
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| ApiError::new("jsonEncodingFailed", error.to_string()))?;
    let mut file = AtomicWriteFile::open(path)
        .map_err(|error| ApiError::io("Could not open atomic storage file", error))?;
    file.write_all(&bytes)
        .and_then(|_| file.write_all(b"\n"))
        .and_then(|_| file.flush())
        .and_then(|_| file.as_file().sync_all())
        .map_err(|error| ApiError::io("Could not write storage file", error))?;
    file.commit()
        .map_err(|error| ApiError::io("Could not atomically replace storage file", error))
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeOwnership {
    pub session_id: Uuid,
    pub windows: Vec<OwnedWindow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnedWindow {
    pub native_id: String,
    pub process_id: u32,
    pub preset_id: Uuid,
    pub item_id: Uuid,
    pub executable_path: Option<String>,
    /// FILETIME creation marker prevents PID/HWND reuse from satisfying ownership proof.
    #[serde(default)]
    pub process_creation_time: Option<u64>,
    /// Graceful close is allowed for every explicitly managed window. Forced process
    /// termination additionally requires this launch provenance bit.
    #[serde(default)]
    pub started_by_worksets: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_preset_and_keeps_backup() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path().join("WorkSets")).unwrap();
        let mut preset = PresetV1::empty("Focus");
        store.save_preset(&preset).unwrap();
        preset.description = Some("Deep work".to_string());
        store.save_preset(&preset).unwrap();

        assert_eq!(store.load_preset(preset.id).unwrap(), preset);
        assert!(store
            .root()
            .join("backups")
            .join(format!("{}.json.bak", preset.id))
            .exists());
    }

    #[test]
    fn default_settings_are_returned_without_file() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path().join("WorkSets")).unwrap();
        assert_eq!(store.load_settings().unwrap(), Settings::default());
    }

    #[test]
    fn update_cache_survives_store_reopen() {
        #[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
        #[serde(rename_all = "camelCase")]
        struct CacheFixture {
            checked_at: DateTime<Utc>,
            available: bool,
        }

        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("WorkSets");
        let store = Store::open(root.clone()).unwrap();
        let expected = CacheFixture {
            checked_at: Utc::now(),
            available: true,
        };
        store.save_update_cache(&expected).unwrap();
        drop(store);

        let reopened = Store::open(root).unwrap();
        let actual: Option<CacheFixture> = reopened.load_update_cache().unwrap();
        assert_eq!(actual, Some(expected));
    }

    #[test]
    fn telemetry_is_typed_redacted_and_bounded() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path().join("WorkSets")).unwrap();
        for index in 0..250 {
            store
                .record_telemetry(
                    true,
                    TelemetryEvent::new(TelemetryEventKind::CaptureCompleted {
                        window_count: index,
                        desktop_count: 2,
                        monitor_count: 3,
                        warning_count: 0,
                    }),
                )
                .unwrap();
        }
        let bytes = fs::read(store.root().join("telemetry.json")).unwrap();
        let queue: TelemetryQueue = serde_json::from_slice(&bytes).unwrap();
        let json = String::from_utf8(bytes).unwrap().to_ascii_lowercase();
        assert_eq!(queue.events.len(), TELEMETRY_MAX_EVENTS);
        assert!(json.len() <= TELEMETRY_MAX_BYTES);
        for forbidden in ["title", "path", "url", "monitorid", "name"] {
            assert!(!json.contains(forbidden));
        }
    }

    #[test]
    fn telemetry_opt_out_writes_nothing_and_clear_removes_backup() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path().join("WorkSets")).unwrap();
        let event = TelemetryEvent::new(TelemetryEventKind::AppStarted);
        store.record_telemetry(false, event.clone()).unwrap();
        assert!(!store.root().join("telemetry.json").exists());
        store.record_telemetry(true, event.clone()).unwrap();
        store.record_telemetry(true, event).unwrap();
        assert!(store.root().join("backups/telemetry.json.bak").exists());
        store.clear_telemetry().unwrap();
        assert!(!store.root().join("telemetry.json").exists());
        assert!(!store.root().join("backups/telemetry.json.bak").exists());
    }
}
