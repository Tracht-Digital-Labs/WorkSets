use crate::domain::{DesktopPolicy, Notice};
use crate::error::{ApiError, ApiResult};
use parking_lot::Mutex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use uuid::Uuid;

#[cfg(windows)]
type BrokerWriter = std::fs::File;
#[cfg(not(windows))]
type BrokerWriter = std::process::ChildStdin;

pub const BROKER_PROTOCOL_VERSION: u32 = 1;

#[derive(Debug)]
pub struct DesktopBroker {
    os: OsBuildInfo,
    adapter: DesktopAdapter,
    mutations_disabled: AtomicBool,
    last_error: Mutex<Option<String>>,
    safe_probe_complete: AtomicBool,
    client: Mutex<Option<SidecarClient>>,
}

impl DesktopBroker {
    pub fn new() -> Self {
        let os = detect_os_build();
        let adapter = DesktopAdapter::for_build(os.build);
        Self {
            os,
            adapter,
            mutations_disabled: AtomicBool::new(false),
            last_error: Mutex::new(None),
            safe_probe_complete: AtomicBool::new(false),
            client: Mutex::new(None),
        }
    }

    pub fn compatibility(&self) -> CompatibilityInfo {
        let mut warnings = Vec::new();
        if self.adapter.experimental() {
            warnings.push(Notice::warning(
                "experimentalDesktopProvider",
                "Virtual desktop control is experimental on this Windows build",
            ));
        }
        if !self.adapter.supports_count_mutation() {
            warnings.push(Notice::warning(
                "desktopCountMutationUnavailable",
                "This provider can capture and switch desktops but cannot safely change their count",
            ));
        }
        if self.mutations_disabled.load(Ordering::SeqCst) {
            warnings.push(Notice::error(
                "desktopCircuitBreakerOpen",
                "Virtual desktop changes are disabled for this session after a provider error",
            ));
        }
        CompatibilityInfo {
            os: self.os.clone(),
            // Build numbers alone are not sufficient: for example, a future Windows 10
            // build must not be reported as supported unless it has a selected adapter.
            supported: !matches!(self.adapter, DesktopAdapter::Unsupported),
            virtual_desktops: DesktopCapability {
                provider: self.adapter.kind(),
                provider_version: self.adapter.version().to_string(),
                experimental: self.adapter.experimental(),
                can_read: !matches!(self.adapter, DesktopAdapter::Unsupported),
                can_create: self.adapter.supports_count_mutation()
                    && !self.mutations_disabled.load(Ordering::SeqCst),
                can_remove: self.adapter.supports_count_mutation()
                    && !self.mutations_disabled.load(Ordering::SeqCst),
                can_move_windows: !matches!(self.adapter, DesktopAdapter::Unsupported)
                    && !self.mutations_disabled.load(Ordering::SeqCst),
                mutations_disabled: self.mutations_disabled.load(Ordering::SeqCst),
                last_error: self.last_error.lock().clone(),
                isolated_process: true,
            },
            windows: WindowCapability {
                can_capture: true,
                can_launch: true,
                can_place: true,
                can_close_managed: true,
            },
            warnings,
        }
    }

    pub fn snapshot(&self) -> ApiResult<DesktopSnapshot> {
        let result = self.request_read(BrokerAction::Snapshot);
        if result.is_ok() {
            self.safe_probe_complete.store(true, Ordering::SeqCst);
        }
        result.and_then(decode_payload)
    }

    pub fn window_desktop_index(&self, native_id: isize) -> Option<u32> {
        self.request_read(BrokerAction::WindowDesktopIndex { native_id })
            .and_then(decode_payload)
            .ok()
    }

    pub fn ensure_desktop_count(
        &self,
        target: u32,
        policy: DesktopPolicy,
        allow_remove: bool,
    ) -> ApiResult<DesktopMutationResult> {
        let current = self.snapshot()?.count;
        if current == target || (policy == DesktopPolicy::ManagedOnly && current > target) {
            return Ok(DesktopMutationResult {
                before: current,
                after: current,
                created: 0,
                removed: 0,
            });
        }
        if !self.adapter.supports_count_mutation() {
            return Err(ApiError::new(
                "desktopMutationUnsupported",
                "The selected Windows desktop provider cannot change the desktop count",
            ));
        }
        if current > target && !allow_remove {
            return Err(ApiError::new(
                "desktopRemovalNeedsConfirmation",
                "Removing desktops requires explicit confirmation",
            ));
        }
        self.request_mutation(BrokerAction::EnsureCount {
            target,
            policy,
            allow_remove,
        })
        .and_then(decode_payload)
    }

    pub fn move_window(&self, native_id: isize, desktop_index: u32) -> ApiResult<()> {
        self.request_mutation(BrokerAction::MoveWindow {
            native_id,
            desktop_index,
        })?;
        Ok(())
    }

    pub fn switch_to(&self, desktop_index: u32) -> ApiResult<()> {
        self.request_mutation(BrokerAction::SwitchTo { desktop_index })?;
        Ok(())
    }

    fn request_read(&self, action: BrokerAction) -> ApiResult<Value> {
        let mut client = self.client.lock();
        let result = (|| {
            if client.is_none() {
                *client = Some(SidecarClient::spawn()?);
            }
            client
                .as_mut()
                .expect("sidecar initialized")
                .request(action)
        })();
        if let Err(error) = &result {
            *client = None;
            // Reads remain available for diagnostics/recovery attempts, but any broker/provider
            // failure disables mutations for the rest of this session.
            self.open_circuit(error);
        }
        result
    }

    fn request_mutation(&self, action: BrokerAction) -> ApiResult<Value> {
        if self.mutations_disabled.load(Ordering::SeqCst) {
            return Err(ApiError::new(
                "desktopCircuitBreakerOpen",
                "Virtual desktop changes are disabled for this session",
            ));
        }
        if self.adapter.experimental() && !self.safe_probe_complete.load(Ordering::SeqCst) {
            if let Err(error) = self.snapshot() {
                self.open_circuit(&error);
                return Err(error);
            }
        }
        let result = self.request_read(action);
        if let Err(error) = &result {
            self.open_circuit(error);
        }
        result
    }

    fn open_circuit(&self, error: &ApiError) {
        self.mutations_disabled.store(true, Ordering::SeqCst);
        *self.last_error.lock() = Some(error.message.clone());
    }
}

impl Default for DesktopBroker {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
enum BrokerAction {
    Handshake,
    Snapshot,
    WindowDesktopIndex {
        native_id: isize,
    },
    EnsureCount {
        target: u32,
        policy: DesktopPolicy,
        allow_remove: bool,
    },
    MoveWindow {
        native_id: isize,
        desktop_index: u32,
    },
    SwitchTo {
        desktop_index: u32,
    },
    Shutdown,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BrokerRequest {
    protocol_version: u32,
    request_id: u64,
    nonce: Uuid,
    request: BrokerAction,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BrokerResponse {
    protocol_version: u32,
    request_id: u64,
    nonce: Uuid,
    ok: bool,
    #[serde(default)]
    payload: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<ApiError>,
}

#[derive(Debug)]
struct SidecarClient {
    child: Child,
    writer: BrokerWriter,
    responses: std::sync::mpsc::Receiver<Result<String, String>>,
    nonce: Uuid,
    next_id: u64,
}

impl SidecarClient {
    fn spawn() -> ApiResult<Self> {
        let (binary, fallback_mode) = locate_sidecar()?;
        #[cfg(windows)]
        let (pipe_name, server_pipe) = create_broker_pipe_server()?;
        let mut command = Command::new(&binary);
        if fallback_mode {
            command.arg("--desktop-broker");
        }
        #[cfg(windows)]
        command
            .arg("--pipe-name")
            .arg(&pipe_name)
            .arg("--parent-pid")
            .arg(std::process::id().to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(not(windows))]
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = command.spawn().map_err(|error| {
            ApiError::new(
                "desktopBrokerStartFailed",
                format!("Could not start isolated desktop broker: {error}"),
            )
        })?;
        #[cfg(windows)]
        let (writer, reader) = match accept_broker_pipe(server_pipe, &mut child) {
            Ok(pipe) => {
                let reader = pipe.try_clone().map_err(|error| {
                    ApiError::io("Could not duplicate desktop broker pipe", error)
                })?;
                (pipe, reader)
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        };
        #[cfg(not(windows))]
        let writer = child.stdin.take().ok_or_else(|| {
            ApiError::new(
                "desktopBrokerPipeFailed",
                "Desktop broker input pipe was not created",
            )
        })?;
        #[cfg(not(windows))]
        let reader = child.stdout.take().ok_or_else(|| {
            ApiError::new(
                "desktopBrokerPipeFailed",
                "Desktop broker output pipe was not created",
            )
        })?;
        let (sender, responses) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("worksets-broker-reader".to_string())
            .spawn(move || {
                let reader = BufReader::new(reader);
                for line in reader.lines() {
                    let value = line.map_err(|error| error.to_string());
                    if sender.send(value).is_err() {
                        break;
                    }
                }
            })
            .map_err(|error| {
                ApiError::new(
                    "desktopBrokerReaderFailed",
                    format!("Could not start broker reader: {error}"),
                )
            })?;
        let mut client = Self {
            child,
            writer,
            responses,
            nonce: Uuid::new_v4(),
            next_id: 1,
        };
        client.request(BrokerAction::Handshake)?;
        Ok(client)
    }

    fn request(&mut self, request: BrokerAction) -> ApiResult<Value> {
        let request_id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        let envelope = BrokerRequest {
            protocol_version: BROKER_PROTOCOL_VERSION,
            request_id,
            nonce: self.nonce,
            request,
        };
        serde_json::to_writer(&mut self.writer, &envelope)
            .and_then(|_| self.writer.write_all(b"\n").map_err(serde_json::Error::io))
            .and_then(|_| self.writer.flush().map_err(serde_json::Error::io))
            .map_err(|error| {
                ApiError::new(
                    "desktopBrokerWriteFailed",
                    format!("Could not send request to desktop broker: {error}"),
                )
            })?;
        let line = match self
            .responses
            .recv_timeout(std::time::Duration::from_secs(8))
        {
            Ok(Ok(line)) => line,
            Ok(Err(error)) => {
                return Err(ApiError::new(
                    "desktopBrokerReadFailed",
                    format!("Could not read desktop broker response: {error}"),
                ));
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                let _ = self.child.kill();
                return Err(ApiError::new(
                    "desktopBrokerTimeout",
                    "The isolated desktop broker did not respond and was terminated",
                ));
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                return Err(ApiError::new(
                    "desktopBrokerExited",
                    "The isolated desktop broker exited unexpectedly",
                ));
            }
        };
        let response: BrokerResponse = serde_json::from_str(&line).map_err(|error| {
            ApiError::new(
                "desktopBrokerProtocolError",
                format!("Desktop broker returned invalid JSON: {error}"),
            )
        })?;
        if response.protocol_version != BROKER_PROTOCOL_VERSION
            || response.request_id != request_id
            || response.nonce != self.nonce
        {
            return Err(ApiError::new(
                "desktopBrokerHandshakeFailed",
                "Desktop broker response did not match protocol, request, or nonce",
            ));
        }
        if response.ok {
            Ok(response.payload)
        } else {
            Err(response.error.unwrap_or_else(|| {
                ApiError::new("desktopProviderError", "Desktop broker request failed")
            }))
        }
    }
}

impl Drop for SidecarClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(windows)]
fn create_broker_pipe_server() -> ApiResult<(String, std::fs::File)> {
    use std::mem::size_of;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::FromRawHandle;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{LocalFree, BOOL, HLOCAL};
    use windows::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
    use windows::Win32::Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX};
    use windows::Win32::System::Pipes::{
        CreateNamedPipeW, PIPE_NOWAIT, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS,
        PIPE_TYPE_BYTE,
    };

    let pipe_name = format!(
        r"\\.\pipe\TrachtDigitalSolutions.WorkSets.{}",
        Uuid::new_v4()
    );
    let pipe_name_wide = std::ffi::OsStr::new(&pipe_name)
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let current_sid = current_user_sid_string()?;
    let descriptor_text = format!("D:P(A;;GA;;;SY)(A;;GA;;;{current_sid})");
    let descriptor_wide = std::ffi::OsStr::new(&descriptor_text)
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            PCWSTR(descriptor_wide.as_ptr()),
            SDDL_REVISION_1,
            &mut descriptor,
            None,
        )
        .map_err(|error| {
            ApiError::new(
                "desktopBrokerSecurityFailed",
                format!("Could not create the broker pipe ACL: {error}"),
            )
        })?;
        let attributes = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: BOOL(0),
        };
        let handle = CreateNamedPipeW(
            PCWSTR(pipe_name_wide.as_ptr()),
            PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_NOWAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            64 * 1024,
            64 * 1024,
            8_000,
            Some(&attributes),
        );
        let _ = LocalFree(HLOCAL(descriptor.0));
        if handle.is_invalid() {
            return Err(ApiError::new(
                "desktopBrokerPipeFailed",
                format!(
                    "Could not create the secured desktop broker pipe: {}",
                    windows::core::Error::from_win32()
                ),
            ));
        }
        Ok((pipe_name, std::fs::File::from_raw_handle(handle.0.cast())))
    }
}

#[cfg(windows)]
fn current_user_sid_string() -> ApiResult<String> {
    use std::mem::size_of;
    use windows::core::PWSTR;
    use windows::Win32::Foundation::{CloseHandle, LocalFree, HANDLE, HLOCAL};
    use windows::Win32::Security::Authorization::ConvertSidToStringSidW;
    use windows::Win32::Security::{GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER};
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).map_err(|error| {
            ApiError::new(
                "desktopBrokerSecurityFailed",
                format!("Could not open the current user token: {error}"),
            )
        })?;
        let result = (|| {
            let mut required = 0;
            let _ = GetTokenInformation(token, TokenUser, None, 0, &mut required);
            if required < size_of::<TOKEN_USER>() as u32 {
                return Err(ApiError::new(
                    "desktopBrokerSecurityFailed",
                    "Windows did not return a current user SID",
                ));
            }
            let word_count = (required as usize).div_ceil(size_of::<usize>());
            let mut buffer = vec![0_usize; word_count];
            GetTokenInformation(
                token,
                TokenUser,
                Some(buffer.as_mut_ptr().cast()),
                required,
                &mut required,
            )
            .map_err(|error| {
                ApiError::new(
                    "desktopBrokerSecurityFailed",
                    format!("Could not read the current user SID: {error}"),
                )
            })?;
            let user = &*(buffer.as_ptr().cast::<TOKEN_USER>());
            let mut sid_text = PWSTR::null();
            ConvertSidToStringSidW(user.User.Sid, &mut sid_text).map_err(|error| {
                ApiError::new(
                    "desktopBrokerSecurityFailed",
                    format!("Could not encode the current user SID: {error}"),
                )
            })?;
            let value = sid_text.to_string().map_err(|error| {
                ApiError::new(
                    "desktopBrokerSecurityFailed",
                    format!("Current user SID was not valid Unicode: {error}"),
                )
            });
            let _ = LocalFree(HLOCAL(sid_text.0.cast()));
            value
        })();
        let _ = CloseHandle(token);
        result
    }
}

#[cfg(windows)]
fn accept_broker_pipe(pipe: std::fs::File, child: &mut Child) -> ApiResult<std::fs::File> {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::{
        ERROR_NO_DATA, ERROR_PIPE_CONNECTED, ERROR_PIPE_LISTENING, HANDLE,
    };
    use windows::Win32::System::Pipes::{
        ConnectNamedPipe, GetNamedPipeClientProcessId, SetNamedPipeHandleState, PIPE_READMODE_BYTE,
        PIPE_WAIT,
    };

    let handle = HANDLE(pipe.as_raw_handle());
    let started = std::time::Instant::now();
    loop {
        match unsafe { ConnectNamedPipe(handle, None) } {
            Ok(()) => break,
            Err(error) => {
                let code = win32_error_code(&error);
                if code == ERROR_PIPE_CONNECTED.0 {
                    break;
                }
                if code != ERROR_PIPE_LISTENING.0 && code != ERROR_NO_DATA.0 {
                    return Err(ApiError::new(
                        "desktopBrokerPipeFailed",
                        format!("Could not accept the desktop broker pipe: {error}"),
                    ));
                }
            }
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|error| ApiError::io("Could not inspect desktop broker process", error))?
        {
            return Err(ApiError::new(
                "desktopBrokerExited",
                format!("Desktop broker exited before connecting ({status})"),
            ));
        }
        if started.elapsed() >= std::time::Duration::from_secs(8) {
            return Err(ApiError::new(
                "desktopBrokerTimeout",
                "The desktop broker did not connect to its secured pipe",
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let wait_mode = PIPE_READMODE_BYTE | PIPE_WAIT;
    unsafe {
        SetNamedPipeHandleState(handle, Some(&wait_mode), None, None).map_err(|error| {
            ApiError::new(
                "desktopBrokerPipeFailed",
                format!("Could not finalize desktop broker pipe mode: {error}"),
            )
        })?;
        let mut client_process_id = 0;
        GetNamedPipeClientProcessId(handle, &mut client_process_id).map_err(|error| {
            ApiError::new(
                "desktopBrokerAuthenticationFailed",
                format!("Could not identify desktop broker client: {error}"),
            )
        })?;
        if client_process_id != child.id() {
            return Err(ApiError::new(
                "desktopBrokerAuthenticationFailed",
                "The process connected to the desktop broker pipe was not the spawned broker",
            ));
        }
    }
    Ok(pipe)
}

#[cfg(windows)]
fn win32_error_code(error: &windows::core::Error) -> u32 {
    error.code().0 as u32 & 0xffff
}

#[cfg(windows)]
fn connect_broker_pipe_client() -> ApiResult<std::fs::File> {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::FromRawHandle;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{ERROR_PIPE_BUSY, GENERIC_READ, GENERIC_WRITE, HANDLE};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_MODE, OPEN_EXISTING,
    };
    use windows::Win32::System::Pipes::{
        GetNamedPipeServerProcessId, SetNamedPipeHandleState, WaitNamedPipeW, PIPE_READMODE_BYTE,
        PIPE_WAIT,
    };

    let pipe_name = sidecar_argument("--pipe-name").ok_or_else(|| {
        ApiError::new(
            "desktopBrokerProtocolError",
            "Desktop broker pipe name was not supplied",
        )
    })?;
    let expected_parent = sidecar_argument("--parent-pid")
        .and_then(|value| value.parse::<u32>().ok())
        .ok_or_else(|| {
            ApiError::new(
                "desktopBrokerProtocolError",
                "Desktop broker parent process was not supplied",
            )
        })?;
    if !pipe_name.starts_with(r"\\.\pipe\TrachtDigitalSolutions.WorkSets.") || pipe_name.len() > 160
    {
        return Err(ApiError::new(
            "desktopBrokerProtocolError",
            "Desktop broker pipe name is outside the WorkSets namespace",
        ));
    }
    let pipe_name_wide = std::ffi::OsStr::new(&pipe_name)
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let started = std::time::Instant::now();
    loop {
        let result = unsafe {
            CreateFileW(
                PCWSTR(pipe_name_wide.as_ptr()),
                GENERIC_READ.0 | GENERIC_WRITE.0,
                FILE_SHARE_MODE(0),
                None,
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                HANDLE::default(),
            )
        };
        match result {
            Ok(handle) => unsafe {
                let wait_mode = PIPE_READMODE_BYTE | PIPE_WAIT;
                SetNamedPipeHandleState(handle, Some(&wait_mode), None, None).map_err(|error| {
                    ApiError::new(
                        "desktopBrokerPipeFailed",
                        format!("Could not set desktop broker client pipe mode: {error}"),
                    )
                })?;
                let mut server_process_id = 0;
                GetNamedPipeServerProcessId(handle, &mut server_process_id).map_err(|error| {
                    ApiError::new(
                        "desktopBrokerAuthenticationFailed",
                        format!("Could not identify desktop broker server: {error}"),
                    )
                })?;
                if server_process_id != expected_parent {
                    let _ = windows::Win32::Foundation::CloseHandle(handle);
                    return Err(ApiError::new(
                        "desktopBrokerAuthenticationFailed",
                        "Desktop broker pipe was not hosted by the expected WorkSets process",
                    ));
                }
                return Ok(std::fs::File::from_raw_handle(handle.0.cast()));
            },
            Err(error) if win32_error_code(&error) == ERROR_PIPE_BUSY.0 => unsafe {
                let _ = WaitNamedPipeW(PCWSTR(pipe_name_wide.as_ptr()), 100);
            },
            Err(error) => {
                return Err(ApiError::new(
                    "desktopBrokerPipeFailed",
                    format!("Could not connect to the desktop broker pipe: {error}"),
                ));
            }
        }
        if started.elapsed() >= std::time::Duration::from_secs(8) {
            return Err(ApiError::new(
                "desktopBrokerTimeout",
                "Desktop broker could not connect to the WorkSets pipe",
            ));
        }
    }
}

#[cfg(windows)]
fn sidecar_argument(name: &str) -> Option<String> {
    let mut arguments = std::env::args();
    while let Some(argument) = arguments.next() {
        if argument == name {
            return arguments.next();
        }
    }
    None
}

fn locate_sidecar() -> ApiResult<(PathBuf, bool)> {
    #[cfg(debug_assertions)]
    if let Some(configured) = std::env::var_os("WORKSETS_DESKTOP_BROKER") {
        let path = PathBuf::from(configured);
        if path.is_file() {
            return Ok((path, false));
        }
    }
    let current = std::env::current_exe().map_err(|error| {
        ApiError::new(
            "desktopBrokerLocationFailed",
            format!("Could not determine application path: {error}"),
        )
    })?;
    #[cfg(debug_assertions)]
    {
        let extension = current.extension().map(|value| value.to_os_string());
        let file_name = if let Some(extension) = extension {
            let mut name = std::ffi::OsString::from("worksets-desktop-broker.");
            name.push(extension);
            name
        } else {
            std::ffi::OsString::from("worksets-desktop-broker")
        };
        if let Some(parent) = current.parent() {
            let direct = parent.join(&file_name);
            if direct.is_file() {
                return Ok((direct, false));
            }
            if parent.file_name().and_then(|value| value.to_str()) == Some("deps") {
                if let Some(debug_directory) = parent.parent() {
                    let debug_binary = debug_directory.join(&file_name);
                    if debug_binary.is_file() {
                        return Ok((debug_binary, false));
                    }
                }
            }
        }
    }
    // Release builds only re-execute the already-selected WorkSets image. This keeps the broker
    // path bound to the installed application instead of trusting an environment override or an
    // adjacent executable. The child still has a distinct process and STA COM apartment.
    Ok((current, true))
}

fn decode_payload<T: for<'de> Deserialize<'de>>(payload: Value) -> ApiResult<T> {
    serde_json::from_value(payload).map_err(|error| {
        ApiError::new(
            "desktopBrokerProtocolError",
            format!("Desktop broker payload was invalid: {error}"),
        )
    })
}

/// Runs the isolated desktop provider protocol over a random, local-only named pipe on Windows.
/// The pipe ACL permits only the current user and SYSTEM, both endpoint PIDs are checked, and the
/// first message establishes a versioned UUID nonce repeated by every subsequent request.
pub fn run_sidecar() -> ApiResult<()> {
    #[cfg(windows)]
    {
        crate::window::ensure_process_unelevated()?;
        let _com_apartment = initialize_sta()?;
        let pipe = connect_broker_pipe_client()?;
        let reader = pipe
            .try_clone()
            .map_err(|error| ApiError::io("Could not duplicate desktop broker pipe", error))?;
        run_protocol(BufReader::new(reader), std::io::BufWriter::new(pipe))
    }
    #[cfg(not(windows))]
    {
        let stdin = std::io::stdin();
        run_protocol(
            BufReader::new(stdin.lock()),
            std::io::BufWriter::new(std::io::stdout().lock()),
        )
    }
}

fn run_protocol<R: BufRead, W: Write>(reader: R, mut writer: W) -> ApiResult<()> {
    let mut nonce: Option<Uuid> = None;
    let host = ProviderHost::new();
    for line in reader.lines() {
        let line = line.map_err(|error| ApiError::io("Could not read broker request", error))?;
        let request: BrokerRequest = match serde_json::from_str(&line) {
            Ok(request) => request,
            Err(error) => {
                return Err(ApiError::new(
                    "desktopBrokerProtocolError",
                    format!("Invalid broker request: {error}"),
                ));
            }
        };
        let authenticated = request.protocol_version == BROKER_PROTOCOL_VERSION
            && match nonce {
                None => matches!(request.request, BrokerAction::Handshake),
                Some(expected) => expected == request.nonce,
            };
        if nonce.is_none() && authenticated {
            nonce = Some(request.nonce);
        }
        let shutdown = matches!(request.request, BrokerAction::Shutdown);
        let result = if authenticated {
            catch_unwind(AssertUnwindSafe(|| host.handle(request.request))).unwrap_or_else(|_| {
                Err(ApiError::new(
                    "desktopProviderPanicked",
                    "Virtual desktop provider panicked",
                ))
            })
        } else {
            Err(ApiError::new(
                "desktopBrokerAuthenticationFailed",
                "Broker protocol version or nonce did not match",
            ))
        };
        let response = match result {
            Ok(payload) => BrokerResponse {
                protocol_version: BROKER_PROTOCOL_VERSION,
                request_id: request.request_id,
                nonce: request.nonce,
                ok: true,
                payload,
                error: None,
            },
            Err(error) => BrokerResponse {
                protocol_version: BROKER_PROTOCOL_VERSION,
                request_id: request.request_id,
                nonce: request.nonce,
                ok: false,
                payload: Value::Null,
                error: Some(error),
            },
        };
        serde_json::to_writer(&mut writer, &response)
            .and_then(|_| writer.write_all(b"\n").map_err(serde_json::Error::io))
            .and_then(|_| writer.flush().map_err(serde_json::Error::io))
            .map_err(|error| ApiError::io("Could not write broker response", error))?;
        if shutdown {
            break;
        }
    }
    Ok(())
}

#[cfg(windows)]
struct ComApartment;

#[cfg(windows)]
impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe {
            windows::Win32::System::Com::CoUninitialize();
        }
    }
}

#[cfg(windows)]
fn initialize_sta() -> ApiResult<ComApartment> {
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED)
            .ok()
            .map_err(|error| {
                ApiError::new(
                    "desktopBrokerComInitializationFailed",
                    format!("Could not initialize the broker STA apartment: {error}"),
                )
            })?;
    }
    Ok(ComApartment)
}

struct ProviderHost {
    adapter: DesktopAdapter,
}

impl ProviderHost {
    fn new() -> Self {
        let build = detect_os_build().build;
        Self {
            adapter: DesktopAdapter::for_build(build),
        }
    }

    fn handle(&self, action: BrokerAction) -> ApiResult<Value> {
        match action {
            BrokerAction::Handshake => Ok(serde_json::json!({
                "provider": self.adapter.kind(),
                "providerVersion": self.adapter.version()
            })),
            BrokerAction::Snapshot => serde_json::to_value(DesktopSnapshot {
                count: self.adapter.count()?,
                current_index: self.adapter.current_index()?,
            })
            .map_err(|error| ApiError::new("desktopBrokerEncodingFailed", error.to_string())),
            BrokerAction::WindowDesktopIndex { native_id } => serde_json::to_value(
                self.adapter.window_desktop_index(native_id)?,
            )
            .map_err(|error| ApiError::new("desktopBrokerEncodingFailed", error.to_string())),
            BrokerAction::EnsureCount {
                target,
                policy,
                allow_remove,
            } => {
                if !(1..=32).contains(&target) {
                    return Err(ApiError::new(
                        "invalidDesktopCount",
                        "The desktop broker accepts between 1 and 32 desktops",
                    ));
                }
                let before = self.adapter.count()?;
                let mut count = before;
                let mut created = 0;
                let mut removed = 0;
                while count < target {
                    self.adapter.create()?;
                    count += 1;
                    created += 1;
                }
                if policy == DesktopPolicy::ExactProtected && allow_remove {
                    while count > target {
                        let remove_index = count - 1;
                        self.adapter
                            .remove(remove_index, remove_index.saturating_sub(1))?;
                        count -= 1;
                        removed += 1;
                    }
                }
                serde_json::to_value(DesktopMutationResult {
                    before,
                    after: count,
                    created,
                    removed,
                })
                .map_err(|error| ApiError::new("desktopBrokerEncodingFailed", error.to_string()))
            }
            BrokerAction::MoveWindow {
                native_id,
                desktop_index,
            } => {
                if desktop_index >= self.adapter.count()? {
                    return Err(ApiError::new(
                        "invalidDesktopIndex",
                        "The destination desktop does not exist",
                    ));
                }
                self.adapter.move_window(native_id, desktop_index)?;
                Ok(Value::Null)
            }
            BrokerAction::SwitchTo { desktop_index } => {
                if desktop_index >= self.adapter.count()? {
                    return Err(ApiError::new(
                        "invalidDesktopIndex",
                        "The destination desktop does not exist",
                    ));
                }
                self.adapter.switch_to(desktop_index)?;
                Ok(Value::Null)
            }
            BrokerAction::Shutdown => Ok(Value::Null),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum DesktopAdapter {
    Windows10,
    Windows11_22H2,
    Windows11_24H2,
    Windows11_25H2,
    ExperimentalLatest,
    Unsupported,
}

impl DesktopAdapter {
    fn for_build(build: u32) -> Self {
        match build {
            19045 => Self::Windows10,
            22621..=22631 => Self::Windows11_22H2,
            26100..=26199 => Self::Windows11_24H2,
            26200..=26299 => Self::Windows11_25H2,
            value if value >= 22000 => Self::ExperimentalLatest,
            _ => Self::Unsupported,
        }
    }

    fn kind(self) -> DesktopProviderKind {
        match self {
            Self::Windows10 => DesktopProviderKind::Windows10Legacy,
            Self::Windows11_22H2 => DesktopProviderKind::Windows11_22H2,
            Self::Windows11_24H2 => DesktopProviderKind::Windows11_24H2,
            Self::Windows11_25H2 => DesktopProviderKind::Windows11_25H2,
            Self::ExperimentalLatest => DesktopProviderKind::ExperimentalLatest,
            Self::Unsupported => DesktopProviderKind::Unavailable,
        }
    }

    fn version(self) -> &'static str {
        match self {
            Self::Windows10 => "winvd-0.0.17",
            Self::Windows11_22H2 => "winvd-0.0.46",
            Self::Windows11_24H2 => "winvd-0.0.49+build-26100",
            Self::Windows11_25H2 => "winvd-0.0.49+build-26200",
            Self::ExperimentalLatest => "winvd-0.0.49+latest-probe",
            Self::Unsupported => "none",
        }
    }

    fn experimental(self) -> bool {
        matches!(self, Self::Windows10 | Self::ExperimentalLatest)
    }

    fn supports_count_mutation(self) -> bool {
        matches!(
            self,
            Self::Windows11_22H2
                | Self::Windows11_24H2
                | Self::Windows11_25H2
                | Self::ExperimentalLatest
        )
    }

    #[cfg(windows)]
    fn count(self) -> ApiResult<u32> {
        match self {
            Self::Windows10 => winvd_win10::get_desktop_count().map_err(provider_error),
            Self::Windows11_22H2 => winvd_win11_22h2::get_desktop_count().map_err(provider_error),
            Self::Windows11_24H2 | Self::Windows11_25H2 | Self::ExperimentalLatest => {
                winvd_win11_24h2::get_desktop_count().map_err(provider_error)
            }
            Self::Unsupported => Err(unsupported_error()),
        }
    }

    #[cfg(not(windows))]
    fn count(self) -> ApiResult<u32> {
        let _ = self;
        Ok(1)
    }

    #[cfg(windows)]
    fn current_index(self) -> ApiResult<u32> {
        match self {
            Self::Windows10 => winvd_win10::get_current_desktop()
                .map(|index| index as u32)
                .map_err(provider_error),
            Self::Windows11_22H2 => winvd_win11_22h2::get_current_desktop()
                .and_then(|desktop| desktop.get_index())
                .map_err(provider_error),
            Self::Windows11_24H2 | Self::Windows11_25H2 | Self::ExperimentalLatest => {
                winvd_win11_24h2::get_current_desktop()
                    .and_then(|desktop| desktop.get_index())
                    .map_err(provider_error)
            }
            Self::Unsupported => Err(unsupported_error()),
        }
    }

    #[cfg(not(windows))]
    fn current_index(self) -> ApiResult<u32> {
        let _ = self;
        Ok(0)
    }

    #[cfg(windows)]
    fn window_desktop_index(self, native_id: isize) -> ApiResult<u32> {
        match self {
            Self::Windows10 => winvd_win10::get_desktop_by_window(native_id as u32)
                .map(|index| index as u32)
                .map_err(provider_error),
            Self::Windows11_22H2 => winvd_win11_22h2::get_desktop_by_window(
                windows_052::Win32::Foundation::HWND(native_id),
            )
            .and_then(|desktop| desktop.get_index())
            .map_err(provider_error),
            Self::Windows11_24H2 | Self::Windows11_25H2 | Self::ExperimentalLatest => {
                winvd_win11_24h2::get_desktop_by_window(windows::Win32::Foundation::HWND(
                    native_id as *mut _,
                ))
                .and_then(|desktop| desktop.get_index())
                .map_err(provider_error)
            }
            Self::Unsupported => Err(unsupported_error()),
        }
    }

    #[cfg(not(windows))]
    fn window_desktop_index(self, _native_id: isize) -> ApiResult<u32> {
        let _ = self;
        Ok(0)
    }

    #[cfg(windows)]
    fn create(self) -> ApiResult<()> {
        match self {
            Self::Windows11_22H2 => winvd_win11_22h2::create_desktop()
                .map(|_| ())
                .map_err(provider_error),
            Self::Windows11_24H2 | Self::Windows11_25H2 | Self::ExperimentalLatest => {
                winvd_win11_24h2::create_desktop()
                    .map(|_| ())
                    .map_err(provider_error)
            }
            _ => Err(unsupported_error()),
        }
    }

    #[cfg(not(windows))]
    fn create(self) -> ApiResult<()> {
        let _ = self;
        Err(unsupported_error())
    }

    #[cfg(windows)]
    fn remove(self, index: u32, fallback: u32) -> ApiResult<()> {
        match self {
            Self::Windows11_22H2 => {
                winvd_win11_22h2::remove_desktop(index, fallback).map_err(provider_error)
            }
            Self::Windows11_24H2 | Self::Windows11_25H2 | Self::ExperimentalLatest => {
                winvd_win11_24h2::remove_desktop(index, fallback).map_err(provider_error)
            }
            _ => Err(unsupported_error()),
        }
    }

    #[cfg(not(windows))]
    fn remove(self, _index: u32, _fallback: u32) -> ApiResult<()> {
        let _ = self;
        Err(unsupported_error())
    }

    #[cfg(windows)]
    fn move_window(self, native_id: isize, index: u32) -> ApiResult<()> {
        match self {
            Self::Windows10 => {
                winvd_win10::move_window_to_desktop(native_id as u32, index as usize)
                    .map_err(provider_error)
            }
            Self::Windows11_22H2 => winvd_win11_22h2::move_window_to_desktop(
                index,
                &windows_052::Win32::Foundation::HWND(native_id),
            )
            .map_err(provider_error),
            Self::Windows11_24H2 | Self::Windows11_25H2 | Self::ExperimentalLatest => {
                winvd_win11_24h2::move_window_to_desktop(
                    index,
                    &windows::Win32::Foundation::HWND(native_id as *mut _),
                )
                .map_err(provider_error)
            }
            Self::Unsupported => Err(unsupported_error()),
        }
    }

    #[cfg(not(windows))]
    fn move_window(self, _native_id: isize, _index: u32) -> ApiResult<()> {
        let _ = self;
        Err(unsupported_error())
    }

    #[cfg(windows)]
    fn switch_to(self, index: u32) -> ApiResult<()> {
        match self {
            Self::Windows10 => winvd_win10::go_to_desktop(index as usize).map_err(provider_error),
            Self::Windows11_22H2 => winvd_win11_22h2::switch_desktop(index).map_err(provider_error),
            Self::Windows11_24H2 | Self::Windows11_25H2 | Self::ExperimentalLatest => {
                winvd_win11_24h2::switch_desktop(index).map_err(provider_error)
            }
            Self::Unsupported => Err(unsupported_error()),
        }
    }

    #[cfg(not(windows))]
    fn switch_to(self, _index: u32) -> ApiResult<()> {
        let _ = self;
        Ok(())
    }
}

fn provider_error(error: impl std::fmt::Debug) -> ApiError {
    ApiError::new(
        "desktopProviderError",
        format!("Virtual desktop provider failed: {error:?}"),
    )
}

fn unsupported_error() -> ApiError {
    ApiError::new(
        "desktopProviderUnavailable",
        "No safe virtual desktop provider is available for this Windows build",
    )
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DesktopSnapshot {
    pub count: u32,
    pub current_index: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DesktopMutationResult {
    pub before: u32,
    pub after: u32,
    pub created: u32,
    pub removed: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CompatibilityInfo {
    pub os: OsBuildInfo,
    pub supported: bool,
    pub virtual_desktops: DesktopCapability,
    pub windows: WindowCapability,
    pub warnings: Vec<Notice>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OsBuildInfo {
    pub major: u32,
    pub minor: u32,
    pub build: u32,
    pub revision: u32,
    pub architecture: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DesktopCapability {
    pub provider: DesktopProviderKind,
    pub provider_version: String,
    pub experimental: bool,
    pub can_read: bool,
    pub can_create: bool,
    pub can_remove: bool,
    pub can_move_windows: bool,
    pub mutations_disabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    pub isolated_process: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DesktopProviderKind {
    Windows10Legacy,
    #[serde(rename = "windows11_22h2")]
    Windows11_22H2,
    #[serde(rename = "windows11_24h2")]
    Windows11_24H2,
    #[serde(rename = "windows11_25h2")]
    Windows11_25H2,
    ExperimentalLatest,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WindowCapability {
    pub can_capture: bool,
    pub can_launch: bool,
    pub can_place: bool,
    pub can_close_managed: bool,
}

#[cfg(windows)]
fn detect_os_build() -> OsBuildInfo {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    use winreg::RegKey;

    let key = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion")
        .ok();
    let build = key
        .as_ref()
        .and_then(|key| key.get_value::<String, _>("CurrentBuildNumber").ok())
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or_default();
    let revision = key
        .as_ref()
        .and_then(|key| key.get_value::<u32, _>("UBR").ok())
        .unwrap_or_default();
    let major = key
        .as_ref()
        .and_then(|key| key.get_value::<u32, _>("CurrentMajorVersionNumber").ok())
        .unwrap_or(10);
    let minor = key
        .as_ref()
        .and_then(|key| key.get_value::<u32, _>("CurrentMinorVersionNumber").ok())
        .unwrap_or_default();
    OsBuildInfo {
        major,
        minor,
        build,
        revision,
        architecture: std::env::consts::ARCH.to_string(),
    }
}

#[cfg(not(windows))]
fn detect_os_build() -> OsBuildInfo {
    OsBuildInfo {
        major: 0,
        minor: 0,
        build: 0,
        revision: 0,
        architecture: std::env::consts::ARCH.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_build_specific_adapters() {
        assert!(matches!(
            DesktopAdapter::for_build(19045),
            DesktopAdapter::Windows10
        ));
        assert_eq!(
            serde_json::to_value(DesktopProviderKind::Windows11_25H2).unwrap(),
            serde_json::json!("windows11_25h2")
        );
        assert_eq!(
            serde_json::to_value(DesktopProviderKind::Windows11_22H2).unwrap(),
            serde_json::json!("windows11_22h2")
        );
        assert_eq!(
            serde_json::to_value(DesktopProviderKind::Windows11_24H2).unwrap(),
            serde_json::json!("windows11_24h2")
        );
        assert!(matches!(
            DesktopAdapter::for_build(22631),
            DesktopAdapter::Windows11_22H2
        ));
        assert!(matches!(
            DesktopAdapter::for_build(26200),
            DesktopAdapter::Windows11_25H2
        ));
        assert!(matches!(
            DesktopAdapter::for_build(27000),
            DesktopAdapter::ExperimentalLatest
        ));
    }

    #[test]
    fn provider_failure_opens_mutation_circuit_without_disabling_reads() {
        let broker = DesktopBroker::new();
        broker.open_circuit(&ApiError::new("fixtureFailure", "provider failed"));
        let compatibility = broker.compatibility();
        assert!(compatibility.virtual_desktops.mutations_disabled);
        assert!(!compatibility.virtual_desktops.can_create);
        assert!(!compatibility.virtual_desktops.can_remove);
        assert!(compatibility.virtual_desktops.can_read || !compatibility.supported);
        assert_eq!(
            compatibility.virtual_desktops.last_error.as_deref(),
            Some("provider failed")
        );
    }
}
