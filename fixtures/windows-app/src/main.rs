#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(not(windows))]
fn main() {
    eprintln!("worksets-window-fixture is available only on Windows");
}

#[cfg(windows)]
mod windows_fixture {
    use std::ffi::c_void;
    use std::ptr::{null, null_mut};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;
    use windows_sys::Win32::Foundation::{
        CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM,
    };
    use windows_sys::Win32::Graphics::Gdi::{UpdateWindow, COLOR_WINDOW, HBRUSH};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::System::Threading::CreateMutexW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, FindWindowW, GetMessageW,
        LoadCursorW, MessageBoxW, PostMessageW, PostQuitMessage, RegisterClassW,
        SetForegroundWindow, ShowWindow, TranslateMessage, CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW,
        CW_USEDEFAULT, IDC_ARROW, IDNO, IDYES, MB_ICONWARNING, MB_YESNOCANCEL, MINMAXINFO, MSG,
        SW_RESTORE, SW_SHOW, WM_APP, WM_CLOSE, WM_CREATE, WM_DESTROY, WM_GETMINMAXINFO, WNDCLASSW,
        WS_OVERLAPPEDWINDOW, WS_VISIBLE,
    };

    const CLASS_NAME: &str = "TDS.WorkSets.WindowFixture";
    const MUTEX_NAME: &str = "Local\\TDS.WorkSets.WindowFixture.SingleInstance";
    const ACTIVATE_MESSAGE: u32 = WM_APP + 17;
    static ENFORCE_MINIMUM: AtomicBool = AtomicBool::new(false);
    static SAVE_DIALOG: AtomicBool = AtomicBool::new(false);

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Mode {
        Normal,
        Delayed,
        Multiple,
        MinimumSize,
        SaveDialog,
        Hung,
        SingleInstance,
    }

    impl Mode {
        fn parse(value: Option<String>) -> Self {
            match value.as_deref() {
                Some("delayed") => Self::Delayed,
                Some("multiple") => Self::Multiple,
                Some("minimum-size") => Self::MinimumSize,
                Some("save-dialog") => Self::SaveDialog,
                Some("hung") => Self::Hung,
                Some("single-instance") => Self::SingleInstance,
                _ => Self::Normal,
            }
        }

        fn label(self) -> &'static str {
            match self {
                Self::Normal => "Normal",
                Self::Delayed => "Delayed start",
                Self::Multiple => "Multiple windows",
                Self::MinimumSize => "Minimum size 640 x 420",
                Self::SaveDialog => "Unsaved work",
                Self::Hung => "Not responding",
                Self::SingleInstance => "Single instance",
            }
        }
    }

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    unsafe extern "system" fn window_proc(
        window: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match message {
            WM_CREATE => {
                let create = lparam as *const CREATESTRUCTW;
                if !create.is_null() {
                    let _ = (*create).lpCreateParams;
                }
                0
            }
            WM_GETMINMAXINFO if ENFORCE_MINIMUM.load(Ordering::Relaxed) => {
                let info = lparam as *mut MINMAXINFO;
                if !info.is_null() {
                    (*info).ptMinTrackSize.x = 640;
                    (*info).ptMinTrackSize.y = 420;
                }
                0
            }
            WM_CLOSE if SAVE_DIALOG.load(Ordering::Relaxed) => {
                let title = wide("WorkSets fixture – unsaved work");
                let message = wide("Close this fixture window and discard the simulated changes?");
                let result = MessageBoxW(
                    window,
                    message.as_ptr(),
                    title.as_ptr(),
                    MB_YESNOCANCEL | MB_ICONWARNING,
                );
                if result == IDYES {
                    DestroyWindow(window);
                } else if result == IDNO {
                    SAVE_DIALOG.store(false, Ordering::Relaxed);
                }
                0
            }
            ACTIVATE_MESSAGE => {
                ShowWindow(window, SW_RESTORE);
                SetForegroundWindow(window);
                0
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(window, message, wparam, lparam),
        }
    }

    unsafe fn register_window_class(instance: HINSTANCE, class_name: &[u16]) -> Result<(), String> {
        let cursor = LoadCursorW(null_mut(), IDC_ARROW);
        let window_class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(window_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: null_mut(),
            hCursor: cursor,
            hbrBackground: (COLOR_WINDOW + 1) as HBRUSH,
            lpszMenuName: null(),
            lpszClassName: class_name.as_ptr(),
        };
        if RegisterClassW(&window_class) == 0 {
            return Err(format!("RegisterClassW failed with {}", GetLastError()));
        }
        Ok(())
    }

    unsafe fn create_window(
        instance: HINSTANCE,
        class_name: &[u16],
        title: &str,
        x: i32,
        y: i32,
    ) -> Result<HWND, String> {
        let title = wide(title);
        let window = CreateWindowExW(
            0,
            class_name.as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            x,
            y,
            920,
            620,
            null_mut(),
            null_mut(),
            instance,
            null_mut::<c_void>(),
        );
        if window.is_null() {
            return Err(format!("CreateWindowExW failed with {}", GetLastError()));
        }
        ShowWindow(window, SW_SHOW);
        UpdateWindow(window);
        Ok(window)
    }

    unsafe fn message_loop() -> i32 {
        let mut message: MSG = std::mem::zeroed();
        loop {
            let result = GetMessageW(&mut message, null_mut(), 0, 0);
            if result <= 0 {
                return if result == 0 {
                    message.wParam as i32
                } else {
                    1
                };
            }
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }

    pub fn run() -> Result<i32, String> {
        let mode = Mode::parse(std::env::args().nth(1));
        if mode == Mode::Delayed {
            std::thread::sleep(Duration::from_millis(1_500));
        }
        ENFORCE_MINIMUM.store(mode == Mode::MinimumSize, Ordering::Relaxed);
        SAVE_DIALOG.store(mode == Mode::SaveDialog, Ordering::Relaxed);

        unsafe {
            let class_name = wide(CLASS_NAME);
            let mut mutex = null_mut();
            if mode == Mode::SingleInstance {
                let mutex_name = wide(MUTEX_NAME);
                mutex = CreateMutexW(null(), 1, mutex_name.as_ptr());
                if mutex.is_null() {
                    return Err(format!("CreateMutexW failed with {}", GetLastError()));
                }
                if GetLastError() == ERROR_ALREADY_EXISTS {
                    let existing = FindWindowW(class_name.as_ptr(), null());
                    if !existing.is_null() {
                        PostMessageW(existing, ACTIVATE_MESSAGE, 0, 0);
                    }
                    CloseHandle(mutex);
                    return Ok(0);
                }
            }

            let instance = GetModuleHandleW(null());
            if instance.is_null() {
                return Err(format!("GetModuleHandleW failed with {}", GetLastError()));
            }
            register_window_class(instance, &class_name)?;
            let title = format!("WorkSets fixture – {}", mode.label());
            create_window(instance, &class_name, &title, CW_USEDEFAULT, CW_USEDEFAULT)?;
            if mode == Mode::Multiple {
                create_window(
                    instance,
                    &class_name,
                    "WorkSets fixture – Multiple windows (2)",
                    180,
                    160,
                )?;
            }
            if mode == Mode::Hung {
                loop {
                    std::thread::sleep(Duration::from_secs(60));
                }
            }
            let exit_code = message_loop();
            if !mutex.is_null() {
                CloseHandle(mutex);
            }
            Ok(exit_code)
        }
    }
}

#[cfg(windows)]
fn main() {
    match windows_fixture::run() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
