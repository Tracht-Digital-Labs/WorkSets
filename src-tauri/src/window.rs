use crate::desktop::DesktopBroker;
use crate::domain::{
    LaunchItem, LaunchTarget, MonitorRef, NormalizedRect, RectI32, WindowPlacementSpec,
    WindowSelector, WindowState,
};
use crate::error::{ApiError, ApiResult};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSnapshot {
    pub monitors: Vec<MonitorRef>,
    pub windows: Vec<CapturedWindow>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CapturedWindow {
    pub native_id: String,
    pub process_id: u32,
    pub title: String,
    pub application: String,
    pub class_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executable_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aumid: Option<String>,
    pub placement: WindowPlacementSpec,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desktop_index: Option<u32>,
    pub elevated: bool,
    pub manageable: bool,
    /// Internal anti-reuse marker for ownership checks; never crosses the frontend boundary.
    #[serde(skip)]
    #[schemars(skip)]
    pub process_creation_time: Option<u64>,
}

impl CapturedWindow {
    pub fn native_value(&self) -> Option<isize> {
        isize::from_str_radix(self.native_id.trim_start_matches("0x"), 16).ok()
    }

    pub fn as_selector(&self, instance_index: u32) -> WindowSelector {
        WindowSelector {
            executable_path: self.executable_path.clone(),
            aumid: self.aumid.clone(),
            class_name: Some(self.class_name.clone()),
            title_pattern: None,
            instance_index,
        }
    }
}

#[derive(Debug)]
pub struct LaunchedProcess {
    pub process_id: Option<u32>,
}

impl LaunchedProcess {
    pub fn detach(self) {}
}

pub struct WindowManager;

pub struct WindowEventWatcher {
    inner: platform::WindowEventWatcher,
}

pub struct WindowWaitRequest<'a> {
    pub process_id: Option<u32>,
    pub timeout: Duration,
    pub cancelled: &'a AtomicBool,
    pub excluded_native_ids: &'a HashSet<String>,
    pub prelaunch_native_ids: &'a HashSet<String>,
    pub event_watcher: &'a WindowEventWatcher,
}

impl WindowManager {
    pub fn capture(broker: &DesktopBroker) -> ApiResult<WorkspaceSnapshot> {
        platform::capture(broker)
    }

    pub fn find_match<'a>(
        item: &LaunchItem,
        windows: &'a [CapturedWindow],
        used: &HashSet<String>,
    ) -> Option<&'a CapturedWindow> {
        let mut candidates: Vec<_> = windows
            .iter()
            .filter(|window| selector_matches(&item.selector, &item.launch_target, window))
            .collect();
        candidates.sort_by_key(|window| window.native_value().unwrap_or_default());
        candidates
            .get(item.selector.instance_index as usize)
            .copied()
            .filter(|window| !used.contains(&window.native_id))
    }

    pub fn launch(item: &LaunchItem) -> ApiResult<LaunchedProcess> {
        platform::launch(&item.launch_target, item.placement.state)
    }

    pub fn arm_window_events() -> WindowEventWatcher {
        WindowEventWatcher {
            inner: platform::WindowEventWatcher::arm(),
        }
    }

    pub fn wait_for_window(
        item: &LaunchItem,
        broker: &DesktopBroker,
        request: WindowWaitRequest<'_>,
    ) -> ApiResult<Option<CapturedWindow>> {
        let started = Instant::now();
        while started.elapsed() < request.timeout {
            if request.cancelled.load(Ordering::SeqCst) {
                return Ok(None);
            }
            let event_native_id = request
                .event_watcher
                .inner
                .wait_next(Duration::from_millis(250), request.cancelled)
                .map(|native_id| format!("0x{native_id:X}"));
            let snapshot = Self::capture(broker)?;
            // A direct process match is strongest. It must still be a newly appeared HWND.
            if let Some(window) = snapshot.windows.iter().find(|window| {
                !request.excluded_native_ids.contains(&window.native_id)
                    && !request.prelaunch_native_ids.contains(&window.native_id)
                    && request
                        .process_id
                        .is_some_and(|process_id| window.process_id == process_id)
            }) {
                return Ok(Some((*window).clone()));
            }
            // Selector fallback is limited to a new window or the exact existing HWND that
            // emitted a show/create event (single-instance activation). Empty selectors never
            // guess a file/URL handler window.
            if let Some(window) = snapshot.windows.into_iter().find(|window| {
                eligible_selector_fallback(
                    window,
                    request.excluded_native_ids,
                    request.prelaunch_native_ids,
                    event_native_id.as_deref(),
                ) && selector_matches(&item.selector, &item.launch_target, window)
            }) {
                return Ok(Some(window));
            }
        }
        Ok(None)
    }

    pub fn place(
        window: &CapturedWindow,
        placement: &WindowPlacementSpec,
        monitors: &[MonitorRef],
    ) -> ApiResult<PlacementOutcome> {
        let (rectangle, adjusted) = map_placement(placement, monitors);
        platform::place(window, rectangle, placement.state)?;
        Ok(PlacementOutcome {
            rectangle,
            adjusted,
        })
    }

    pub fn close_owned(
        window: &CapturedWindow,
        ownership: &OwnedWindowProof,
        force_if_hung: bool,
    ) -> ApiResult<CloseOutcome> {
        if ownership.native_id != window.native_id || ownership.process_id != window.process_id {
            return Err(ApiError::new(
                "ownershipProofMismatch",
                "The window no longer matches its WorkSets management record",
            ));
        }
        if matches!(
            (
                ownership.executable_path.as_deref(),
                window.executable_path.as_deref()
            ),
            (Some(expected), Some(actual)) if normalize_path(expected) != normalize_path(actual)
        ) || (ownership.executable_path.is_some() && window.executable_path.is_none())
        {
            return Err(ApiError::new(
                "ownershipProofMismatch",
                "The process executable no longer matches the WorkSets ownership record",
            ));
        }
        if ownership.process_creation_time.is_some()
            && ownership.process_creation_time != window.process_creation_time
        {
            return Err(ApiError::new(
                "ownershipProofMismatch",
                "The process creation marker no longer matches the WorkSets record",
            ));
        }
        if !window.manageable {
            return Err(ApiError::new(
                "windowNotManageable",
                "The owned window cannot be managed at the current integrity level",
            ));
        }
        if force_if_hung
            && (!ownership.started_by_worksets
                || ownership.executable_path.is_none()
                || ownership.process_creation_time.is_none())
        {
            return Err(ApiError::new(
                "forceCloseNotAllowed",
                "Only a process started by WorkSets can be force-closed",
            ));
        }
        platform::close_owned(window, force_if_hung)
    }

    pub fn is_alive(window: &CapturedWindow) -> bool {
        platform::is_alive(window)
    }
}

fn eligible_selector_fallback(
    window: &CapturedWindow,
    excluded_native_ids: &HashSet<String>,
    prelaunch_native_ids: &HashSet<String>,
    event_native_id: Option<&str>,
) -> bool {
    !excluded_native_ids.contains(&window.native_id)
        && (!prelaunch_native_ids.contains(&window.native_id)
            || event_native_id == Some(window.native_id.as_str()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlacementOutcome {
    pub rectangle: RectI32,
    pub adjusted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseOutcome {
    Requested,
    Forced,
    AlreadyClosed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedWindowProof {
    pub native_id: String,
    pub process_id: u32,
    pub executable_path: Option<String>,
    pub process_creation_time: Option<u64>,
    pub started_by_worksets: bool,
}

/// Requests Per-Monitor-V2 before Tauri creates its first HWND. A failure is intentionally
/// tolerated because Windows returns an error when a host already fixed the DPI context.
pub fn enable_per_monitor_v2_dpi() {
    platform::enable_per_monitor_v2_dpi();
}

pub fn ensure_process_unelevated() -> ApiResult<()> {
    platform::ensure_process_unelevated()
}

pub fn show_startup_error(message: &str) {
    platform::show_startup_error(message);
}

fn selector_matches(
    selector: &WindowSelector,
    launch_target: &LaunchTarget,
    window: &CapturedWindow,
) -> bool {
    let target_executable = selector.executable_path.as_deref().or({
        if let LaunchTarget::Executable { path, .. } = launch_target {
            Some(path.as_str())
        } else {
            None
        }
    });
    if let Some(path) = target_executable {
        let Some(actual) = window.executable_path.as_deref() else {
            return false;
        };
        if normalize_path(path) != normalize_path(actual) {
            return false;
        }
    }
    let target_aumid = selector.aumid.as_deref().or({
        if let LaunchTarget::PackagedApp { aumid, .. } = launch_target {
            Some(aumid.as_str())
        } else {
            None
        }
    });
    if let Some(aumid) = target_aumid {
        if window.aumid.as_deref() != Some(aumid) {
            return false;
        }
    }
    if let Some(class_name) = &selector.class_name {
        if !window.class_name.eq_ignore_ascii_case(class_name) {
            return false;
        }
    }
    if let Some(pattern) = &selector.title_pattern {
        let Ok(regex) = regex::RegexBuilder::new(pattern)
            .case_insensitive(true)
            .build()
        else {
            return false;
        };
        if !regex.is_match(&window.title) {
            return false;
        }
    }
    target_executable.is_some()
        || target_aumid.is_some()
        || selector.class_name.is_some()
        || selector.title_pattern.is_some()
}

fn normalize_path(value: &str) -> String {
    value.replace('/', "\\").trim().to_ascii_lowercase()
}

fn workspace_to_screen_rectangle(rectangle: RectI32, monitor: &MonitorRef) -> RectI32 {
    RectI32 {
        x: rectangle.x + (monitor.work_area.x - monitor.bounds.x),
        y: rectangle.y + (monitor.work_area.y - monitor.bounds.y),
        ..rectangle
    }
}

#[cfg(test)]
fn screen_to_workspace_rectangle(rectangle: RectI32, monitor: &MonitorRef) -> RectI32 {
    RectI32 {
        x: rectangle.x - (monitor.work_area.x - monitor.bounds.x),
        y: rectangle.y - (monitor.work_area.y - monitor.bounds.y),
        ..rectangle
    }
}

pub fn map_placement(requested: &WindowPlacementSpec, monitors: &[MonitorRef]) -> (RectI32, bool) {
    let selected = monitors
        .iter()
        .find(|monitor| monitor.id == requested.monitor.id)
        .or_else(|| {
            monitors.iter().find(|monitor| {
                monitor
                    .device_name
                    .eq_ignore_ascii_case(&requested.monitor.device_name)
            })
        })
        .or_else(|| monitors.iter().find(|monitor| monitor.primary))
        .or_else(|| monitors.first());
    let Some(monitor) = selected else {
        return (requested.physical, true);
    };

    let work = monitor.work_area;
    let normalized = requested.normalized;
    let mut width = (normalized.width * f64::from(work.width)).round() as i32;
    let mut height = (normalized.height * f64::from(work.height)).round() as i32;
    width = width.clamp(100, work.width.max(100));
    height = height.clamp(80, work.height.max(80));
    let requested_x = work.x + (normalized.x * f64::from(work.width)).round() as i32;
    let requested_y = work.y + (normalized.y * f64::from(work.height)).round() as i32;
    let max_x = work.x + work.width - width;
    let max_y = work.y + work.height - height;
    let rectangle = RectI32 {
        x: requested_x.clamp(work.x, max_x.max(work.x)),
        y: requested_y.clamp(work.y, max_y.max(work.y)),
        width,
        height,
    };
    let adjusted = monitor.id != requested.monitor.id
        || rectangle != requested.physical
        || monitor.dpi_x != requested.monitor.dpi_x
        || monitor.dpi_y != requested.monitor.dpi_y;
    (rectangle, adjusted)
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::collections::HashMap;
    use std::ffi::c_void;
    use std::mem::size_of;
    use windows::core::{GUID, PWSTR};
    use windows::Win32::Devices::Display::{
        DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QueryDisplayConfig,
        DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
        DISPLAYCONFIG_DEVICE_INFO_HEADER, DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO,
        DISPLAYCONFIG_SOURCE_DEVICE_NAME, DISPLAYCONFIG_TARGET_DEVICE_NAME, QDC_ONLY_ACTIVE_PATHS,
    };
    use windows::Win32::Foundation::{CloseHandle, BOOL, FILETIME, HANDLE, HWND, LPARAM, RECT};
    use windows::Win32::Foundation::{LPARAM as MESSAGE_LPARAM, WPARAM};
    use windows::Win32::Graphics::Gdi::{
        EnumDisplayMonitors, GetMonitorInfoW, MonitorFromWindow, HDC, HMONITOR, MONITORINFOEXW,
        MONITOR_DEFAULTTONEAREST,
    };
    use windows::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows::Win32::Storage::Packaging::Appx::GetApplicationUserModelId;
    use windows::Win32::System::Com::StructuredStorage::PropVariantToString;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_LOCAL_SERVER, COINIT_MULTITHREADED,
    };
    use windows::Win32::System::Threading::{
        CreateProcessW, GetCurrentProcess, GetProcessId, GetProcessTimes, OpenProcess,
        OpenProcessToken, QueryFullProcessImageNameW, TerminateProcess, PROCESS_CREATION_FLAGS,
        PROCESS_INFORMATION, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION,
        PROCESS_TERMINATE, STARTF_USESHOWWINDOW, STARTUPINFOW,
    };
    use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
    use windows::Win32::UI::HiDpi::{
        GetDpiForMonitor, SetProcessDpiAwarenessContext,
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, MDT_EFFECTIVE_DPI,
    };
    use windows::Win32::UI::Shell::PropertiesSystem::{
        IPropertyStore, SHGetPropertyStoreForWindow, PROPERTYKEY,
    };
    use windows::Win32::UI::Shell::{
        ApplicationActivationManager, IApplicationActivationManager, ShellExecuteExW, AO_NONE,
        SEE_MASK_FLAG_NO_UI, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, EnumWindows, GetClassNameW, GetWindowLongW, GetWindowPlacement,
        GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsHungAppWindow, IsIconic,
        IsWindow, IsWindowVisible, IsZoomed, MessageBoxW, PeekMessageW, PostMessageW, SetWindowPos,
        ShowWindowAsync, TranslateMessage, CHILDID_SELF, EVENT_OBJECT_CREATE, EVENT_OBJECT_SHOW,
        GWL_EXSTYLE, MB_ICONERROR, MB_OK, MONITORINFOF_PRIMARY, MSG, OBJID_WINDOW, PM_REMOVE,
        SWP_NOACTIVATE, SWP_NOZORDER, SW_RESTORE, SW_SHOWMAXIMIZED, SW_SHOWMINIMIZED,
        SW_SHOWNOACTIVATE, SW_SHOWNORMAL, WINDOWPLACEMENT, WINEVENT_OUTOFCONTEXT, WM_CLOSE,
        WS_EX_TOOLWINDOW,
    };

    const PKEY_APP_USER_MODEL_ID: PROPERTYKEY = PROPERTYKEY {
        fmtid: GUID::from_u128(0x9f4c2855_9f79_4b39_a8d0_e1d42de1d5f3),
        pid: 5,
    };

    struct ComApartment(bool);

    impl ComApartment {
        fn initialize() -> Self {
            let result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
            Self(result.is_ok())
        }
    }

    impl Drop for ComApartment {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() };
            }
        }
    }

    pub(super) fn enable_per_monitor_v2_dpi() {
        unsafe {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        }
    }

    pub(super) fn ensure_process_unelevated() -> ApiResult<()> {
        let elevated = unsafe { process_elevated(GetCurrentProcess()) }.unwrap_or(true);
        if elevated {
            Err(ApiError::new(
                "elevatedProcessNotAllowed",
                "WorkSets kann nicht als Administrator ausgeführt werden. Bitte normal starten.\n\nWorkSets cannot run as administrator. Please start it normally.",
            ))
        } else {
            Ok(())
        }
    }

    pub(super) fn show_startup_error(message: &str) {
        use std::os::windows::ffi::OsStrExt;
        use windows::core::PCWSTR;

        let text = std::ffi::OsStr::new(message)
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let title = std::ffi::OsStr::new("WorkSets")
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        unsafe {
            let _ = MessageBoxW(
                HWND::default(),
                PCWSTR(text.as_ptr()),
                PCWSTR(title.as_ptr()),
                MB_OK | MB_ICONERROR,
            );
        }
    }

    pub(super) fn capture(broker: &DesktopBroker) -> ApiResult<WorkspaceSnapshot> {
        let _com_apartment = ComApartment::initialize();
        let monitors = enumerate_monitors()?;
        let mut raw_windows: Vec<RawWindow> = Vec::new();
        unsafe {
            EnumWindows(
                Some(enum_window),
                LPARAM((&mut raw_windows as *mut Vec<RawWindow>) as isize),
            )
            .map_err(|error| ApiError::new("windowEnumerationFailed", error.to_string()))?;
        }
        let mut windows = Vec::new();
        for raw in raw_windows {
            if let Some(window) = capture_one(raw.hwnd, &monitors, broker) {
                windows.push(window);
            }
        }
        Ok(WorkspaceSnapshot { monitors, windows })
    }

    struct RawWindow {
        hwnd: HWND,
    }

    unsafe extern "system" fn enum_window(hwnd: HWND, data: LPARAM) -> BOOL {
        if IsWindowVisible(hwnd).as_bool() {
            let style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
            if style & WS_EX_TOOLWINDOW.0 == 0 {
                let windows = &mut *(data.0 as *mut Vec<RawWindow>);
                windows.push(RawWindow { hwnd });
            }
        }
        BOOL(1)
    }

    fn capture_one(
        hwnd: HWND,
        monitors: &[MonitorRef],
        broker: &DesktopBroker,
    ) -> Option<CapturedWindow> {
        let title = unsafe { window_text(hwnd) };
        if title.trim().is_empty() {
            return None;
        }
        let class_name = unsafe { class_name(hwnd) };
        if matches!(
            class_name.as_str(),
            "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
        ) {
            return None;
        }
        let mut process_id = 0;
        unsafe {
            GetWindowThreadProcessId(hwnd, Some(&mut process_id));
        }
        if process_id == 0 || process_id == std::process::id() {
            return None;
        }

        let (executable_path, process_aumid, target_elevated, accessible, creation_time) =
            unsafe { process_details(process_id) };
        let aumid = unsafe { window_aumid(hwnd) }.or(process_aumid);
        let monitor_handle = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
        let monitor_device_name =
            unsafe { monitor_device_name(monitor_handle) }.unwrap_or_default();
        let monitor = monitors
            .iter()
            .find(|monitor| {
                monitor
                    .device_name
                    .eq_ignore_ascii_case(&monitor_device_name)
            })
            .or_else(|| monitors.iter().find(|monitor| monitor.primary))
            .or_else(|| monitors.first())?
            .clone();
        let placement = unsafe { window_placement(hwnd, monitor.clone()) }?;
        let native_value = hwnd.0 as isize;
        let application = executable_path
            .as_deref()
            .and_then(|path| Path::new(path).file_stem())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| title.clone());
        Some(CapturedWindow {
            native_id: format!("0x{native_value:X}"),
            process_id,
            title,
            application,
            class_name,
            executable_path,
            aumid,
            placement,
            desktop_index: broker.window_desktop_index(native_value),
            elevated: target_elevated,
            // Elevated targets are never controlled, even if WorkSets itself was started at a
            // higher integrity level. This preserves the product's unelevated safety boundary.
            manageable: accessible && !target_elevated,
            process_creation_time: creation_time,
        })
    }

    unsafe fn window_text(hwnd: HWND) -> String {
        let length = GetWindowTextLengthW(hwnd).max(0) as usize;
        let mut buffer = vec![0_u16; length.saturating_add(1).max(2)];
        let copied = GetWindowTextW(hwnd, &mut buffer).max(0) as usize;
        String::from_utf16_lossy(&buffer[..copied])
    }

    unsafe fn class_name(hwnd: HWND) -> String {
        let mut buffer = vec![0_u16; 256];
        let copied = GetClassNameW(hwnd, &mut buffer).max(0) as usize;
        String::from_utf16_lossy(&buffer[..copied])
    }

    unsafe fn process_details(
        process_id: u32,
    ) -> (Option<String>, Option<String>, bool, bool, Option<u64>) {
        let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) else {
            return (None, None, true, false, None);
        };
        let path = process_path(process);
        let elevated = process_elevated(process).unwrap_or(true);
        let aumid = process_aumid(process);
        let creation_time = process_creation_time(process);
        let _ = CloseHandle(process);
        (path, aumid, elevated, true, creation_time)
    }

    unsafe fn process_path(process: HANDLE) -> Option<String> {
        let mut buffer = vec![0_u16; 32_768];
        let mut length = buffer.len() as u32;
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_FORMAT(0),
            PWSTR(buffer.as_mut_ptr()),
            &mut length,
        )
        .ok()
        .map(|_| String::from_utf16_lossy(&buffer[..length as usize]))
    }

    unsafe fn process_creation_time(process: HANDLE) -> Option<u64> {
        let mut creation = FILETIME::default();
        let mut exit = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        GetProcessTimes(process, &mut creation, &mut exit, &mut kernel, &mut user)
            .ok()
            .map(|_| (u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime))
    }

    unsafe fn process_aumid(process: HANDLE) -> Option<String> {
        let mut length = 0_u32;
        let _ = GetApplicationUserModelId(process, &mut length, PWSTR::null());
        if length <= 1 || length > 32_768 {
            return None;
        }
        let mut buffer = vec![0_u16; length as usize];
        let result = GetApplicationUserModelId(process, &mut length, PWSTR(buffer.as_mut_ptr()));
        if result.0 != 0 {
            return None;
        }
        let used = (length as usize).min(buffer.len());
        let used = used.saturating_sub(1);
        let value = String::from_utf16_lossy(&buffer[..used]);
        (!value.trim().is_empty()).then_some(value)
    }

    unsafe fn window_aumid(hwnd: HWND) -> Option<String> {
        let store: IPropertyStore = SHGetPropertyStoreForWindow(hwnd).ok()?;
        let value = store.GetValue(&PKEY_APP_USER_MODEL_ID).ok()?;
        let mut buffer = vec![0_u16; 512];
        PropVariantToString(&value, &mut buffer).ok()?;
        let used = buffer
            .iter()
            .position(|character| *character == 0)
            .unwrap_or(buffer.len());
        let value = String::from_utf16_lossy(&buffer[..used]);
        (!value.trim().is_empty()).then_some(value)
    }

    unsafe fn process_elevated(process: HANDLE) -> Option<bool> {
        let mut token = HANDLE::default();
        OpenProcessToken(process, TOKEN_QUERY, &mut token).ok()?;
        let mut elevation = TOKEN_ELEVATION::default();
        let mut returned = 0;
        let result = GetTokenInformation(
            token,
            TokenElevation,
            Some((&mut elevation as *mut TOKEN_ELEVATION).cast::<c_void>()),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        );
        let _ = CloseHandle(token);
        result.ok().map(|_| elevation.TokenIsElevated != 0)
    }

    unsafe fn window_placement(hwnd: HWND, monitor: MonitorRef) -> Option<WindowPlacementSpec> {
        let mut placement = WINDOWPLACEMENT {
            length: size_of::<WINDOWPLACEMENT>() as u32,
            ..WINDOWPLACEMENT::default()
        };
        GetWindowPlacement(hwnd, &mut placement).ok()?;
        let normal = placement.rcNormalPosition;
        // WINDOWPLACEMENT uses workspace coordinates for normal top-level windows, while
        // SetWindowPos and PresetV1 physical geometry use screen coordinates.
        let physical = workspace_to_screen_rectangle(rect_from_win32(normal), &monitor);
        let work = monitor.work_area;
        let normalized = if work.width > 0 && work.height > 0 {
            NormalizedRect {
                x: f64::from(physical.x - work.x) / f64::from(work.width),
                y: f64::from(physical.y - work.y) / f64::from(work.height),
                width: f64::from(physical.width) / f64::from(work.width),
                height: f64::from(physical.height) / f64::from(work.height),
            }
        } else {
            NormalizedRect {
                width: 1.0,
                height: 1.0,
                ..NormalizedRect::default()
            }
        };
        let state = if IsIconic(hwnd).as_bool() {
            WindowState::Minimized
        } else if IsZoomed(hwnd).as_bool() {
            WindowState::Maximized
        } else {
            WindowState::Normal
        };
        Some(WindowPlacementSpec {
            monitor,
            physical,
            normalized,
            state,
        })
    }

    fn enumerate_monitors() -> ApiResult<Vec<MonitorRef>> {
        let mut handles: Vec<HMONITOR> = Vec::new();
        unsafe {
            let _ = EnumDisplayMonitors(
                HDC::default(),
                None,
                Some(enum_monitor),
                LPARAM((&mut handles as *mut Vec<HMONITOR>) as isize),
            );
        }
        let stable_ids = active_monitor_ids();
        let monitors = handles
            .into_iter()
            .filter_map(|handle| unsafe { monitor_details(handle, &stable_ids) })
            .collect::<Vec<_>>();
        if monitors.is_empty() {
            Err(ApiError::new(
                "monitorEnumerationFailed",
                "Windows did not report any active monitors",
            ))
        } else {
            Ok(monitors)
        }
    }

    unsafe extern "system" fn enum_monitor(
        monitor: HMONITOR,
        _device_context: HDC,
        _rectangle: *mut RECT,
        data: LPARAM,
    ) -> BOOL {
        let handles = &mut *(data.0 as *mut Vec<HMONITOR>);
        handles.push(monitor);
        BOOL(1)
    }

    unsafe fn monitor_details(
        handle: HMONITOR,
        stable_ids: &HashMap<String, String>,
    ) -> Option<MonitorRef> {
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
        if !GetMonitorInfoW(handle, (&mut info as *mut MONITORINFOEXW).cast()).as_bool() {
            return None;
        }
        let device_length = info
            .szDevice
            .iter()
            .position(|character| *character == 0)
            .unwrap_or(info.szDevice.len());
        let device_name = String::from_utf16_lossy(&info.szDevice[..device_length]);
        let mut dpi_x = 96;
        let mut dpi_y = 96;
        let _ = GetDpiForMonitor(handle, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y);
        let id = stable_ids
            .get(&device_name.to_ascii_lowercase())
            .cloned()
            .unwrap_or_else(|| device_name.clone());
        Some(MonitorRef {
            id,
            device_name,
            bounds: rect_from_win32(info.monitorInfo.rcMonitor),
            work_area: rect_from_win32(info.monitorInfo.rcWork),
            dpi_x,
            dpi_y,
            primary: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
        })
    }

    unsafe fn monitor_device_name(handle: HMONITOR) -> Option<String> {
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
        if !GetMonitorInfoW(handle, (&mut info as *mut MONITORINFOEXW).cast()).as_bool() {
            return None;
        }
        Some(wide_string(&info.szDevice))
    }

    fn active_monitor_ids() -> HashMap<String, String> {
        unsafe {
            let mut path_count = 0;
            let mut mode_count = 0;
            if GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut path_count, &mut mode_count)
                .0
                != 0
            {
                return HashMap::new();
            }
            let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize];
            let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); mode_count as usize];
            if QueryDisplayConfig(
                QDC_ONLY_ACTIVE_PATHS,
                &mut path_count,
                paths.as_mut_ptr(),
                &mut mode_count,
                modes.as_mut_ptr(),
                None,
            )
            .0 != 0
            {
                return HashMap::new();
            }
            paths.truncate(path_count as usize);
            let mut result = HashMap::new();
            for path in paths {
                let mut source = DISPLAYCONFIG_SOURCE_DEVICE_NAME {
                    header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                        r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
                        size: size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32,
                        adapterId: path.sourceInfo.adapterId,
                        id: path.sourceInfo.id,
                    },
                    ..DISPLAYCONFIG_SOURCE_DEVICE_NAME::default()
                };
                let mut target = DISPLAYCONFIG_TARGET_DEVICE_NAME {
                    header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                        r#type: DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
                        size: size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32,
                        adapterId: path.targetInfo.adapterId,
                        id: path.targetInfo.id,
                    },
                    ..DISPLAYCONFIG_TARGET_DEVICE_NAME::default()
                };
                if DisplayConfigGetDeviceInfo(&mut source.header) != 0
                    || DisplayConfigGetDeviceInfo(&mut target.header) != 0
                {
                    continue;
                }
                let gdi_name = wide_string(&source.viewGdiDeviceName);
                let device_path = wide_string(&target.monitorDevicePath);
                if !gdi_name.is_empty() && !device_path.is_empty() {
                    result.insert(
                        gdi_name.to_ascii_lowercase(),
                        format!("monitor-{:016x}", stable_hash(&device_path)),
                    );
                }
            }
            result
        }
    }

    fn wide_string(value: &[u16]) -> String {
        let used = value
            .iter()
            .position(|character| *character == 0)
            .unwrap_or(value.len());
        String::from_utf16_lossy(&value[..used])
    }

    fn stable_hash(value: &str) -> u64 {
        value
            .bytes()
            .map(|byte| byte.to_ascii_lowercase())
            .fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
                (hash ^ u64::from(byte)).wrapping_mul(0x1000_0000_01b3)
            })
    }

    fn rect_from_win32(rectangle: RECT) -> RectI32 {
        RectI32 {
            x: rectangle.left,
            y: rectangle.top,
            width: rectangle.right - rectangle.left,
            height: rectangle.bottom - rectangle.top,
        }
    }

    pub(super) fn launch(
        target: &LaunchTarget,
        requested_state: WindowState,
    ) -> ApiResult<LaunchedProcess> {
        if let LaunchTarget::File { path } = target {
            if !Path::new(path).is_file() {
                return Err(ApiError::new(
                    "launchTargetMissing",
                    format!("File does not exist: {path}"),
                ));
            }
            return shell_execute(path, requested_state);
        }
        if let LaunchTarget::Url { url } = target {
            let parsed =
                url::Url::parse(url).map_err(|_| ApiError::new("invalidUrl", "URL is invalid"))?;
            if !matches!(parsed.scheme(), "http" | "https") {
                return Err(ApiError::new(
                    "invalidUrlScheme",
                    "Only HTTP and HTTPS URLs can be opened",
                ));
            }
            return shell_execute(url, requested_state);
        }
        if let LaunchTarget::PackagedApp { aumid, args } = target {
            return activate_packaged_app(aumid, args);
        }
        let LaunchTarget::Executable {
            path,
            args,
            working_directory,
        } = target
        else {
            unreachable!()
        };
        create_executable_process(path, args, working_directory.as_deref(), requested_state)
    }

    fn create_executable_process(
        path: &str,
        args: &[String],
        working_directory: Option<&str>,
        requested_state: WindowState,
    ) -> ApiResult<LaunchedProcess> {
        use std::os::windows::ffi::OsStrExt;
        use windows::core::{PCWSTR, PWSTR};

        if !Path::new(path).is_file() {
            return Err(ApiError::new(
                "launchTargetMissing",
                format!("Executable does not exist: {path}"),
            ));
        }
        if working_directory.is_some_and(|directory| !Path::new(directory).is_dir()) {
            return Err(ApiError::new(
                "workingDirectoryMissing",
                "Working directory does not exist",
            ));
        }
        let application = std::ffi::OsStr::new(path)
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let mut all_arguments = Vec::with_capacity(args.len() + 1);
        all_arguments.push(path.to_string());
        all_arguments.extend(args.iter().cloned());
        let command_line = quote_windows_arguments(&all_arguments);
        let mut command_line_wide = std::ffi::OsStr::new(&command_line)
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let working_directory_wide = working_directory.map(|directory| {
            std::ffi::OsStr::new(directory)
                .encode_wide()
                .chain(Some(0))
                .collect::<Vec<_>>()
        });
        let show = match requested_state {
            WindowState::Normal => SW_SHOWNORMAL,
            WindowState::Maximized => SW_SHOWMAXIMIZED,
            WindowState::Minimized => SW_SHOWMINIMIZED,
        };
        let startup = STARTUPINFOW {
            cb: size_of::<STARTUPINFOW>() as u32,
            dwFlags: STARTF_USESHOWWINDOW,
            wShowWindow: show.0 as u16,
            ..STARTUPINFOW::default()
        };
        let mut process = PROCESS_INFORMATION::default();
        unsafe {
            CreateProcessW(
                PCWSTR(application.as_ptr()),
                PWSTR(command_line_wide.as_mut_ptr()),
                None,
                None,
                false,
                PROCESS_CREATION_FLAGS(0),
                None,
                working_directory_wide
                    .as_ref()
                    .map_or(PCWSTR::null(), |directory| PCWSTR(directory.as_ptr())),
                &startup,
                &mut process,
            )
            .map_err(|error| ApiError::new("launchFailed", error.to_string()))?;
            let _ = CloseHandle(process.hThread);
            let _ = CloseHandle(process.hProcess);
        }
        Ok(LaunchedProcess {
            process_id: (process.dwProcessId != 0).then_some(process.dwProcessId),
        })
    }

    fn shell_execute(target: &str, requested_state: WindowState) -> ApiResult<LaunchedProcess> {
        use std::os::windows::ffi::OsStrExt;
        use windows::core::PCWSTR;

        let verb = std::ffi::OsStr::new("open")
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let file = std::ffi::OsStr::new(target)
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let show = match requested_state {
            WindowState::Normal => SW_SHOWNOACTIVATE,
            WindowState::Maximized => SW_SHOWMAXIMIZED,
            WindowState::Minimized => SW_SHOWMINIMIZED,
        };
        let mut execute = SHELLEXECUTEINFOW {
            cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_FLAG_NO_UI,
            lpVerb: PCWSTR(verb.as_ptr()),
            lpFile: PCWSTR(file.as_ptr()),
            nShow: show.0,
            ..SHELLEXECUTEINFOW::default()
        };
        unsafe {
            ShellExecuteExW(&mut execute)
                .map_err(|error| ApiError::new("launchFailed", error.to_string()))?;
            let process_id = if execute.hProcess.is_invalid() {
                None
            } else {
                let value = GetProcessId(execute.hProcess);
                let _ = CloseHandle(execute.hProcess);
                (value != 0).then_some(value)
            };
            Ok(LaunchedProcess { process_id })
        }
    }

    fn activate_packaged_app(aumid: &str, args: &[String]) -> ApiResult<LaunchedProcess> {
        use std::os::windows::ffi::OsStrExt;
        use windows::core::PCWSTR;

        if aumid.trim().is_empty() || aumid.contains('\0') {
            return Err(ApiError::new(
                "invalidPackagedApp",
                "Packaged application identifier is invalid",
            ));
        }
        let _com_apartment = ComApartment::initialize();
        let aumid_wide = std::ffi::OsStr::new(aumid)
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let arguments = quote_windows_arguments(args);
        let arguments_wide = std::ffi::OsStr::new(&arguments)
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        unsafe {
            let manager: IApplicationActivationManager =
                CoCreateInstance(&ApplicationActivationManager, None, CLSCTX_LOCAL_SERVER)
                    .map_err(|error| {
                        ApiError::new("packagedAppActivationFailed", error.to_string())
                    })?;
            let process_id = manager
                .ActivateApplication(
                    PCWSTR(aumid_wide.as_ptr()),
                    PCWSTR(arguments_wide.as_ptr()),
                    AO_NONE,
                )
                .map_err(|error| ApiError::new("packagedAppActivationFailed", error.to_string()))?;
            Ok(LaunchedProcess {
                process_id: (process_id != 0).then_some(process_id),
            })
        }
    }

    pub(super) fn quote_windows_arguments(arguments: &[String]) -> String {
        arguments
            .iter()
            .map(|argument| {
                if !argument.is_empty()
                    && !argument
                        .chars()
                        .any(|character| character.is_whitespace() || character == '"')
                {
                    return argument.clone();
                }
                let mut quoted = String::from('"');
                let mut backslashes = 0_usize;
                for character in argument.chars() {
                    if character == '\\' {
                        backslashes += 1;
                    } else if character == '"' {
                        quoted.extend(std::iter::repeat_n('\\', backslashes * 2 + 1));
                        quoted.push('"');
                        backslashes = 0;
                    } else {
                        quoted.extend(std::iter::repeat_n('\\', backslashes));
                        quoted.push(character);
                        backslashes = 0;
                    }
                }
                quoted.extend(std::iter::repeat_n('\\', backslashes * 2));
                quoted.push('"');
                quoted
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub(super) fn place(
        window: &CapturedWindow,
        rectangle: RectI32,
        state: WindowState,
    ) -> ApiResult<()> {
        if !window.manageable {
            return Err(ApiError::new(
                "elevatedWindow",
                "The window is elevated and cannot be managed without administrator rights",
            ));
        }
        let native_id = window
            .native_value()
            .ok_or_else(|| ApiError::new("invalidWindowHandle", "Window handle is invalid"))?;
        let hwnd = HWND(native_id as *mut _);
        unsafe {
            if !IsWindow(hwnd).as_bool() {
                return Err(ApiError::new(
                    "windowClosed",
                    "The target window is no longer open",
                ));
            }
            let _ = ShowWindowAsync(hwnd, SW_RESTORE);
            SetWindowPos(
                hwnd,
                HWND::default(),
                rectangle.x,
                rectangle.y,
                rectangle.width,
                rectangle.height,
                SWP_NOACTIVATE | SWP_NOZORDER,
            )
            .map_err(|error| ApiError::new("windowPlacementFailed", error.to_string()))?;
            let command = match state {
                WindowState::Normal => SW_SHOWNOACTIVATE,
                WindowState::Maximized => SW_SHOWMAXIMIZED,
                WindowState::Minimized => SW_SHOWMINIMIZED,
            };
            let _ = ShowWindowAsync(hwnd, command);
        }
        Ok(())
    }

    pub(super) fn close_owned(
        window: &CapturedWindow,
        force_if_hung: bool,
    ) -> ApiResult<CloseOutcome> {
        let Some(native_id) = window.native_value() else {
            return Err(ApiError::new(
                "invalidWindowHandle",
                "Window handle is invalid",
            ));
        };
        let hwnd = HWND(native_id as *mut _);
        unsafe {
            if !IsWindow(hwnd).as_bool() {
                return Ok(CloseOutcome::AlreadyClosed);
            }
            let mut current_process_id = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut current_process_id));
            if current_process_id != window.process_id {
                return Err(ApiError::new(
                    "ownershipProofMismatch",
                    "The window handle now belongs to a different process",
                ));
            }
            if force_if_hung {
                let process = OpenProcess(
                    PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION,
                    false,
                    window.process_id,
                )
                .map_err(|error| ApiError::new("processOpenFailed", error.to_string()))?;
                let current_path = process_path(process);
                let current_creation_time = process_creation_time(process);
                if current_path.as_deref().map(normalize_path)
                    != window.executable_path.as_deref().map(normalize_path)
                    || current_creation_time != window.process_creation_time
                {
                    let _ = CloseHandle(process);
                    return Err(ApiError::new(
                        "ownershipProofMismatch",
                        "The process executable changed before forced close",
                    ));
                }
                let result = TerminateProcess(process, 1);
                let _ = CloseHandle(process);
                result
                    .map_err(|error| ApiError::new("processTerminateFailed", error.to_string()))?;
                return Ok(CloseOutcome::Forced);
            }
            if IsHungAppWindow(hwnd).as_bool() {
                return Err(ApiError::new(
                    "windowNotResponding",
                    "The managed window is not responding",
                ));
            }
            PostMessageW(hwnd, WM_CLOSE, WPARAM(0), MESSAGE_LPARAM(0))
                .map_err(|error| ApiError::new("windowCloseFailed", error.to_string()))?;
        }
        Ok(CloseOutcome::Requested)
    }

    pub(super) fn is_alive(window: &CapturedWindow) -> bool {
        window.native_value().is_some_and(|native_id| unsafe {
            let hwnd = HWND(native_id as *mut _);
            if !IsWindow(hwnd).as_bool() {
                return false;
            }
            let mut process_id = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut process_id));
            process_id == window.process_id
        })
    }

    static CREATED_WINDOWS: std::sync::Mutex<std::collections::VecDeque<isize>> =
        std::sync::Mutex::new(std::collections::VecDeque::new());

    #[cfg(test)]
    pub(super) fn record_test_window_event(native_id: isize) {
        if let Ok(mut windows) = CREATED_WINDOWS.lock() {
            windows.push_back(native_id);
        }
    }

    unsafe extern "system" fn window_event_callback(
        _hook: HWINEVENTHOOK,
        _event: u32,
        hwnd: HWND,
        object_id: i32,
        child_id: i32,
        _event_thread: u32,
        _event_time: u32,
    ) {
        if object_id == OBJID_WINDOW.0 && child_id == CHILDID_SELF as i32 {
            if let Ok(mut windows) = CREATED_WINDOWS.lock() {
                if windows.len() == 128 {
                    windows.pop_front();
                }
                windows.push_back(hwnd.0 as isize);
            }
        }
    }

    pub(super) struct WindowEventWatcher {
        hook: HWINEVENTHOOK,
    }

    impl WindowEventWatcher {
        pub(super) fn arm() -> Self {
            if let Ok(mut windows) = CREATED_WINDOWS.lock() {
                windows.clear();
            }
            let hook = unsafe {
                SetWinEventHook(
                    EVENT_OBJECT_CREATE,
                    EVENT_OBJECT_SHOW,
                    None,
                    Some(window_event_callback),
                    0,
                    0,
                    WINEVENT_OUTOFCONTEXT,
                )
            };
            Self { hook }
        }

        pub(super) fn wait_next(&self, timeout: Duration, cancelled: &AtomicBool) -> Option<isize> {
            if self.hook.0.is_null() {
                std::thread::sleep(timeout.min(Duration::from_millis(125)));
                return None;
            }
            let started = Instant::now();
            while started.elapsed() < timeout && !cancelled.load(Ordering::SeqCst) {
                if let Ok(mut windows) = CREATED_WINDOWS.lock() {
                    if let Some(native_id) = windows.pop_front() {
                        return Some(native_id);
                    }
                }
                unsafe {
                    let mut message = MSG::default();
                    while PeekMessageW(&mut message, HWND::default(), 0, 0, PM_REMOVE).as_bool() {
                        let _ = TranslateMessage(&message);
                        DispatchMessageW(&message);
                    }
                }
                std::thread::sleep(Duration::from_millis(15));
            }
            CREATED_WINDOWS
                .lock()
                .ok()
                .and_then(|mut windows| windows.pop_front())
        }
    }

    impl Drop for WindowEventWatcher {
        fn drop(&mut self) {
            if !self.hook.0.is_null() {
                unsafe {
                    let _ = UnhookWinEvent(self.hook);
                }
            }
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::*;

    pub(super) fn capture(_broker: &DesktopBroker) -> ApiResult<WorkspaceSnapshot> {
        Ok(WorkspaceSnapshot {
            monitors: Vec::new(),
            windows: Vec::new(),
        })
    }

    pub(super) fn launch(
        _target: &LaunchTarget,
        _requested_state: WindowState,
    ) -> ApiResult<LaunchedProcess> {
        Err(ApiError::new(
            "windowsOnly",
            "Launching desktop applications is supported only on Windows",
        ))
    }

    pub(super) fn place(
        _window: &CapturedWindow,
        _rectangle: RectI32,
        _state: WindowState,
    ) -> ApiResult<()> {
        Err(ApiError::new(
            "windowsOnly",
            "Window placement is supported only on Windows",
        ))
    }

    pub(super) fn close_owned(
        _window: &CapturedWindow,
        _force_if_hung: bool,
    ) -> ApiResult<CloseOutcome> {
        Err(ApiError::new(
            "windowsOnly",
            "Window closing is supported only on Windows",
        ))
    }

    pub(super) fn is_alive(_window: &CapturedWindow) -> bool {
        false
    }

    pub(super) struct WindowEventWatcher;

    impl WindowEventWatcher {
        pub(super) fn arm() -> Self {
            Self
        }

        pub(super) fn wait_next(
            &self,
            timeout: Duration,
            _cancelled: &AtomicBool,
        ) -> Option<isize> {
            std::thread::sleep(timeout.min(Duration::from_millis(125)));
            None
        }
    }

    pub(super) fn enable_per_monitor_v2_dpi() {}

    pub(super) fn ensure_process_unelevated() -> ApiResult<()> {
        Ok(())
    }

    pub(super) fn show_startup_error(message: &str) {
        eprintln!("WorkSets: {message}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn monitor(id: &str, x: i32, width: i32, primary: bool) -> MonitorRef {
        MonitorRef {
            id: id.to_string(),
            device_name: id.to_string(),
            bounds: RectI32 {
                x,
                y: 0,
                width,
                height: 1080,
            },
            work_area: RectI32 {
                x,
                y: 0,
                width,
                height: 1040,
            },
            dpi_x: 96,
            dpi_y: 96,
            primary,
        }
    }

    fn captured_window(native_id: &str, process_id: u32) -> CapturedWindow {
        let monitor = monitor("monitor", 0, 1920, true);
        CapturedWindow {
            native_id: native_id.to_string(),
            process_id,
            title: "Editor".to_string(),
            application: "Editor".to_string(),
            class_name: "EditorWindow".to_string(),
            executable_path: Some(r"C:\Tools\editor.exe".to_string()),
            aumid: None,
            placement: WindowPlacementSpec {
                monitor,
                physical: RectI32 {
                    width: 800,
                    height: 600,
                    ..RectI32::default()
                },
                normalized: NormalizedRect {
                    width: 0.5,
                    height: 0.5,
                    ..NormalizedRect::default()
                },
                state: WindowState::Normal,
            },
            desktop_index: Some(0),
            elevated: false,
            manageable: true,
            process_creation_time: Some(u64::from(process_id)),
        }
    }

    fn launch_item(instance_index: u32) -> LaunchItem {
        LaunchItem {
            id: Uuid::new_v4(),
            display_name: "Editor".to_string(),
            desktop_id: Uuid::new_v4(),
            launch_target: LaunchTarget::Executable {
                path: r"C:\Tools\editor.exe".to_string(),
                args: Vec::new(),
                working_directory: None,
            },
            selector: WindowSelector {
                executable_path: Some(r"C:\Tools\editor.exe".to_string()),
                class_name: Some("EditorWindow".to_string()),
                instance_index,
                ..WindowSelector::default()
            },
            placement: captured_window("0x1", 1).placement,
            lifecycle: crate::domain::LifecyclePolicy::Reuse,
            enabled: true,
        }
    }

    #[test]
    fn remaps_negative_monitor_coordinates() {
        let original = monitor("left", -1920, 1920, false);
        let placement = WindowPlacementSpec {
            monitor: original,
            physical: RectI32 {
                x: -1800,
                y: 100,
                width: 960,
                height: 520,
            },
            normalized: NormalizedRect {
                x: 0.0625,
                y: 0.096_153_846,
                width: 0.5,
                height: 0.5,
            },
            state: WindowState::Normal,
        };
        let reordered = monitor("left", 1920, 1920, false);
        let (mapped, adjusted) = map_placement(&placement, &[reordered]);
        assert_eq!(mapped.x, 2040);
        assert_eq!(mapped.width, 960);
        assert!(adjusted);
    }

    #[test]
    fn workspace_geometry_round_trips_with_top_taskbar() {
        let mut monitor = monitor("primary", 0, 1920, true);
        monitor.bounds.height = 1080;
        monitor.work_area = RectI32 {
            x: 0,
            y: 48,
            width: 1920,
            height: 1032,
        };
        let workspace = RectI32 {
            x: 100,
            y: 52,
            width: 800,
            height: 600,
        };
        let screen = workspace_to_screen_rectangle(workspace, &monitor);
        assert_eq!(screen.y, 100);
        assert_eq!(screen_to_workspace_rectangle(screen, &monitor), workspace);
    }

    #[test]
    fn workspace_geometry_round_trips_on_negative_monitor_with_left_taskbar() {
        let mut monitor = monitor("left", -1920, 1920, false);
        monitor.work_area = RectI32 {
            x: -1880,
            y: 0,
            width: 1880,
            height: 1040,
        };
        let workspace = RectI32 {
            x: -1840,
            y: 80,
            width: 900,
            height: 700,
        };
        let screen = workspace_to_screen_rectangle(workspace, &monitor);
        assert_eq!(screen.x, -1800);
        assert_eq!(screen_to_workspace_rectangle(screen, &monitor), workspace);
    }

    #[test]
    fn missing_monitor_falls_back_to_primary_and_stays_visible() {
        let missing = monitor("missing", 3000, 1920, false);
        let placement = WindowPlacementSpec {
            monitor: missing,
            physical: RectI32 {
                x: 4000,
                y: 200,
                width: 1000,
                height: 800,
            },
            normalized: NormalizedRect {
                x: 0.9,
                y: 0.9,
                width: 0.8,
                height: 0.8,
            },
            state: WindowState::Normal,
        };
        let primary = monitor("primary", 0, 1920, true);
        let (mapped, adjusted) = map_placement(&placement, &[primary]);
        assert!(mapped.x + mapped.width <= 1920);
        assert!(mapped.y + mapped.height <= 1040);
        assert!(adjusted);
    }

    #[test]
    fn instance_index_is_applied_before_used_window_filter() {
        let windows = vec![captured_window("0x10", 10), captured_window("0x20", 20)];
        let mut used = HashSet::new();
        used.insert("0x10".to_string());
        let matched = WindowManager::find_match(&launch_item(1), &windows, &used).unwrap();
        assert_eq!(matched.native_id, "0x20");

        used.insert("0x20".to_string());
        assert!(WindowManager::find_match(&launch_item(1), &windows, &used).is_none());
    }

    #[test]
    fn restart_selector_fallback_rejects_all_prelaunch_windows() {
        let old_a = captured_window("0x10", 10);
        let old_b = captured_window("0x20", 20);
        let new_window = captured_window("0x30", 30);
        let used = HashSet::new();
        let baseline = [old_a.native_id.clone(), old_b.native_id.clone()]
            .into_iter()
            .collect();
        assert!(!eligible_selector_fallback(&old_a, &used, &baseline, None));
        assert!(!eligible_selector_fallback(&old_b, &used, &baseline, None));
        assert!(eligible_selector_fallback(
            &new_window,
            &used,
            &baseline,
            None
        ));
        assert!(eligible_selector_fallback(
            &old_b,
            &used,
            &baseline,
            Some("0x20")
        ));
    }

    #[test]
    fn packaged_target_aumid_is_a_selector_fallback() {
        let mut window = captured_window("0x10", 10);
        window.aumid = Some("Example.App_123!Main".to_string());
        let target = LaunchTarget::PackagedApp {
            aumid: "Example.App_123!Main".to_string(),
            args: Vec::new(),
        };
        assert!(selector_matches(
            &WindowSelector::default(),
            &target,
            &window
        ));
    }

    #[cfg(windows)]
    #[test]
    fn packaged_arguments_are_encoded_without_shell_interpretation() {
        let encoded = platform::quote_windows_arguments(&[
            "plain".to_string(),
            "two words".to_string(),
            r#"quote\"inside"#.to_string(),
            r"trailing\\".to_string(),
        ]);
        assert_eq!(encoded, r#"plain "two words" "quote\\\"inside" trailing\\"#);
    }

    #[cfg(windows)]
    #[test]
    fn prearmed_event_watcher_keeps_event_emitted_before_wait() {
        let watcher = WindowManager::arm_window_events();
        platform::record_test_window_event(0x44);
        assert_eq!(
            watcher
                .inner
                .wait_next(Duration::from_millis(1), &AtomicBool::new(false)),
            Some(0x44)
        );
    }
}
