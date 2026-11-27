use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock, RwLock};

/// Window title for the YouTube sign-in WebView window.
pub const YT_SIGN_IN_TITLE: &str = "LastWave YouTube Sign In";

/// Window title for the persistent BotGuard WebView window.
pub const BOTGUARD_TITLE: &str = "LastWave BotGuard";

/// Window title for the ephemeral BotGuard WebView window.
pub const BOTGUARD_EPHEMERAL_TITLE: &str = "LastWave BotGuard Ephemeral";

static DISMISSED: AtomicBool = AtomicBool::new(false);
static PLATFORM_BACKEND: OnceLock<RwLock<Option<Arc<dyn WebviewGuardBackend>>>> = OnceLock::new();

fn backend_slot() -> &'static RwLock<Option<Arc<dyn WebviewGuardBackend>>> {
    PLATFORM_BACKEND.get_or_init(|| RwLock::new(None))
}

/// Platform hook abstraction for Linux GTK (`yt_webview_guard.cc`) or custom Tauri window
/// backends where WebView visibility/close interception is wired through the UI shell.
pub trait WebviewGuardBackend: Send + Sync {
    fn is_sign_in_visible(&self) -> bool;
    fn hide_sign_in(&self) -> bool;
    fn show_sign_in(&self) -> bool;
    fn hide_botguard(&self) -> bool;
    fn show_botguard(&self) -> bool;
}

/// Registers an optional platform backend (used on Linux GTK to wire delete-event hiding).
pub fn register_backend(backend: Arc<dyn WebviewGuardBackend>) {
    if let Ok(mut slot) = backend_slot().write() {
        *slot = Some(backend);
    }
}

/// Guard for WebView sign-in and BotGuard windows across Windows and Linux.
pub struct YtWebviewGuard;

impl YtWebviewGuard {
    /// Returns `true` if native window guarding is supported on this platform.
    pub fn is_supported() -> bool {
        cfg!(any(target_os = "linux", target_os = "windows"))
    }

    /// Returns `true` if the sign-in window was dismissed by the user or hidden.
    pub fn is_dismissed() -> bool {
        DISMISSED.load(Ordering::Acquire)
    }

    /// Marks the sign-in window as dismissed (e.g. from a GTK `delete-event` callback).
    pub fn mark_dismissed() {
        DISMISSED.store(true, Ordering::Release);
    }

    /// Whether the sign-in window is currently visible and not dismissed.
    pub fn is_visible() -> bool {
        if !Self::is_supported() {
            return true;
        }
        if Self::is_dismissed() {
            return false;
        }
        if let Some(backend) = current_backend() {
            let vis = backend.is_sign_in_visible();
            if !vis {
                DISMISSED.store(true, Ordering::Release);
            }
            return vis;
        }
        platform::is_sign_in_visible().unwrap_or(true)
    }

    /// Hides the sign-in window. Returns `false` when unsupported or not found.
    pub fn hide() -> bool {
        if !Self::is_supported() {
            return false;
        }
        DISMISSED.store(true, Ordering::Release);
        if let Some(backend) = current_backend() {
            return backend.hide_sign_in();
        }
        platform::hide_sign_in()
    }

    /// Shows (and presents) the sign-in window. Returns `false` when missing.
    pub fn show() -> bool {
        if !Self::is_supported() {
            return false;
        }
        DISMISSED.store(false, Ordering::Release);
        if let Some(backend) = current_backend() {
            return backend.show_sign_in();
        }
        platform::show_sign_in()
    }

    /// Hides all BotGuard windows belonging to this process.
    ///
    /// On Windows, hides the top-level HWND, removes `WS_EX_APPWINDOW`, applies
    /// `WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE`, and moves it off-screen at `(-32000, -32000)`.
    pub fn hide_botguard() -> bool {
        if !Self::is_supported() {
            return false;
        }
        if let Some(backend) = current_backend() {
            return backend.hide_botguard();
        }
        platform::hide_botguard()
    }

    /// Shows the BotGuard window for diagnostics only.
    pub fn show_botguard() -> bool {
        if !Self::is_supported() {
            return false;
        }
        if let Some(backend) = current_backend() {
            return backend.show_botguard();
        }
        platform::show_botguard()
    }

    /// Installs the process-scoped WinEvent hook (`EVENT_OBJECT_CREATE`..`EVENT_OBJECT_SHOW`)
    /// on Windows so newly created/shown BotGuard HWNDs are hidden before flashing on screen.
    pub fn install_botguard_hook() -> Option<BotGuardHookGuard> {
        platform::install_botguard_hook()
    }
}

fn current_backend() -> Option<Arc<dyn WebviewGuardBackend>> {
    backend_slot().read().ok().and_then(|g| g.clone())
}

/// RAII handle for an installed native BotGuard window event hook.
pub struct BotGuardHookGuard {
    #[cfg(target_os = "windows")]
    hook: windows::Win32::UI::Accessibility::HWINEVENTHOOK,
}

#[cfg(target_os = "windows")]
unsafe impl Send for BotGuardHookGuard {}
#[cfg(target_os = "windows")]
unsafe impl Sync for BotGuardHookGuard {}

#[cfg(target_os = "windows")]
impl Drop for BotGuardHookGuard {
    fn drop(&mut self) {
        if !self.hook.is_invalid() {
            unsafe {
                let _ = windows::Win32::UI::Accessibility::UnhookWinEvent(self.hook);
            }
        }
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use super::{
        BotGuardHookGuard, BOTGUARD_EPHEMERAL_TITLE, BOTGUARD_TITLE, YT_SIGN_IN_TITLE,
    };
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
    use windows::Win32::System::Threading::GetCurrentProcessId;
    use windows::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowTextW, GetWindowThreadProcessId, IsWindow, IsWindowVisible,
        SetForegroundWindow, SetWindowPos, ShowWindow, CHILDID_SELF, EVENT_OBJECT_CREATE,
        EVENT_OBJECT_SHOW, GWL_EXSTYLE, OBJID_WINDOW, SWP_FRAMECHANGED, SWP_HIDEWINDOW,
        SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW, SW_HIDE, SW_SHOW,
        WINEVENT_OUTOFCONTEXT, WS_EX_APPWINDOW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    };

    #[cfg(target_pointer_width = "64")]
    unsafe fn get_ex_style(hwnd: HWND) -> isize {
        windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(hwnd, GWL_EXSTYLE)
    }

    #[cfg(not(target_pointer_width = "64"))]
    unsafe fn get_ex_style(hwnd: HWND) -> isize {
        windows::Win32::UI::WindowsAndMessaging::GetWindowLongW(hwnd, GWL_EXSTYLE) as isize
    }

    #[cfg(target_pointer_width = "64")]
    unsafe fn set_ex_style(hwnd: HWND, style: isize) -> isize {
        windows::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style)
    }

    #[cfg(not(target_pointer_width = "64"))]
    unsafe fn set_ex_style(hwnd: HWND, style: isize) -> isize {
        windows::Win32::UI::WindowsAndMessaging::SetWindowLongW(hwnd, GWL_EXSTYLE, style as i32)
            as isize
    }

    fn window_title_if_current_process(hwnd: HWND) -> Option<String> {
        unsafe {
            if hwnd.0.is_null() || !IsWindow(hwnd).as_bool() {
                return None;
            }
            let mut pid: u32 = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid != GetCurrentProcessId() {
                return None;
            }
            let mut buf = [0u16; 256];
            let len = GetWindowTextW(hwnd, &mut buf);
            if len <= 0 {
                return None;
            }
            Some(String::from_utf16_lossy(&buf[..len as usize]))
        }
    }

    fn is_botguard_window(hwnd: HWND) -> bool {
        window_title_if_current_process(hwnd).is_some_and(|title| {
            title == BOTGUARD_TITLE || title == BOTGUARD_EPHEMERAL_TITLE
        })
    }

    fn is_sign_in_window(hwnd: HWND) -> bool {
        window_title_if_current_process(hwnd).is_some_and(|title| title == YT_SIGN_IN_TITLE)
    }

    fn hide_botguard_hwnd(hwnd: HWND) {
        unsafe {
            let _ = ShowWindow(hwnd, SW_HIDE);
            let ex = get_ex_style(hwnd);
            let next_ex = (ex & !(WS_EX_APPWINDOW.0 as isize))
                | (WS_EX_TOOLWINDOW.0 as isize)
                | (WS_EX_NOACTIVATE.0 as isize);
            set_ex_style(hwnd, next_ex);
            let _ = SetWindowPos(
                hwnd,
                None,
                -32000,
                -32000,
                0,
                0,
                SWP_NOSIZE
                    | SWP_NOZORDER
                    | SWP_NOACTIVATE
                    | SWP_FRAMECHANGED
                    | SWP_HIDEWINDOW,
            );
        }
    }

    fn show_botguard_hwnd(hwnd: HWND) {
        unsafe {
            let ex = get_ex_style(hwnd);
            let next_ex = (ex & !(WS_EX_TOOLWINDOW.0 as isize)) & !(WS_EX_NOACTIVATE.0 as isize)
                | (WS_EX_APPWINDOW.0 as isize);
            set_ex_style(hwnd, next_ex);
            let _ = SetWindowPos(
                hwnd,
                None,
                80,
                80,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED | SWP_SHOWWINDOW,
            );
            let _ = ShowWindow(hwnd, SW_SHOW);
        }
    }

    fn find_windows(predicate: fn(HWND) -> bool) -> Vec<HWND> {
        struct CollectState {
            predicate: fn(HWND) -> bool,
            found: Vec<HWND>,
        }

        unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
            let state = &mut *(lparam.0 as *mut CollectState);
            if (state.predicate)(hwnd) {
                state.found.push(hwnd);
            }
            BOOL(1)
        }

        let mut state = CollectState {
            predicate,
            found: Vec::new(),
        };
        unsafe {
            let _ = EnumWindows(Some(enum_proc), LPARAM(&mut state as *mut _ as isize));
        }
        state.found
    }

    pub fn is_sign_in_visible() -> Option<bool> {
        let windows = find_windows(is_sign_in_window);
        let first = windows.first()?;
        unsafe { Some(IsWindowVisible(*first).as_bool()) }
    }

    pub fn hide_sign_in() -> bool {
        let windows = find_windows(is_sign_in_window);
        if windows.is_empty() {
            return false;
        }
        unsafe {
            for hwnd in windows {
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
        }
        true
    }

    pub fn show_sign_in() -> bool {
        let windows = find_windows(is_sign_in_window);
        if windows.is_empty() {
            return false;
        }
        unsafe {
            for hwnd in windows {
                let _ = ShowWindow(hwnd, SW_SHOW);
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_SHOWWINDOW,
                );
                let _ = SetForegroundWindow(hwnd);
            }
        }
        true
    }

    pub fn hide_botguard() -> bool {
        let windows = find_windows(is_botguard_window);
        if windows.is_empty() {
            return false;
        }
        for hwnd in windows {
            hide_botguard_hwnd(hwnd);
        }
        true
    }

    pub fn show_botguard() -> bool {
        let windows = find_windows(is_botguard_window);
        if windows.is_empty() {
            return false;
        }
        for hwnd in windows {
            show_botguard_hwnd(hwnd);
        }
        true
    }

    unsafe extern "system" fn botguard_win_event_proc(
        _hook: HWINEVENTHOOK,
        event: u32,
        hwnd: HWND,
        id_object: i32,
        id_child: i32,
        _thread: u32,
        _time: u32,
    ) {
        if id_object != OBJID_WINDOW.0 || id_child != CHILDID_SELF as i32 {
            return;
        }
        if event != EVENT_OBJECT_CREATE && event != EVENT_OBJECT_SHOW {
            return;
        }
        if is_botguard_window(hwnd) {
            hide_botguard_hwnd(hwnd);
        }
    }

    pub fn install_botguard_hook() -> Option<BotGuardHookGuard> {
        unsafe {
            let hook = SetWinEventHook(
                EVENT_OBJECT_CREATE,
                EVENT_OBJECT_SHOW,
                None,
                Some(botguard_win_event_proc),
                GetCurrentProcessId(),
                0,
                WINEVENT_OUTOFCONTEXT,
            );
            if hook.is_invalid() {
                None
            } else {
                Some(BotGuardHookGuard { hook })
            }
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod platform {
    use super::BotGuardHookGuard;

    pub fn is_sign_in_visible() -> Option<bool> {
        None
    }

    pub fn hide_sign_in() -> bool {
        false
    }

    pub fn show_sign_in() -> bool {
        false
    }

    pub fn hide_botguard() -> bool {
        false
    }

    pub fn show_botguard() -> bool {
        false
    }

    pub fn install_botguard_hook() -> Option<BotGuardHookGuard> {
        None
    }
}
