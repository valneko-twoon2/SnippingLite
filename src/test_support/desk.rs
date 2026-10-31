use super::{Sandbox, CAPTIONS, THEME};
use std::{
    io::Write as _,
    sync::MutexGuard,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{BOOL, HWND, LPARAM, LRESULT, WPARAM},
    UI::{
        HiDpi::{
            SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT,
            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        },
        WindowsAndMessaging::*,
    },
};

/// 1 回に流すか捨てるメッセージの上限。取り出しても消えないメッセージ（描かれないままの窓の
/// WM_PAINT など）があっても、ループを終える。
pub const MESSAGE_LIMIT: usize = 10_000;

/// 見えている窓を隠す（`EnumThreadWindows` のコールバック）。
unsafe extern "system" fn hide(hwnd: HWND, _: LPARAM) -> BOOL {
    if IsWindowVisible(hwnd) != 0 {
        ShowWindow(hwnd, SW_HIDE);
    }
    1
}

pub struct Desk {
    pub sandbox: Sandbox,
    state: Box<crate::State>,
    pub main: HWND,
    dpi: DPI_AWARENESS_CONTEXT,
    _guards: [MutexGuard<'static, ()>; 2],
}

impl Desk {
    /// Starts the tray process with the settings saved in a fresh sandbox.
    pub fn start() -> Desk {
        Desk::start_with(|_| {})
    }

    /// Like `start`, after `prepare` changed the saved settings.
    pub fn start_with(prepare: impl FnOnce(&mut crate::config::Settings)) -> Desk {
        // The same order everywhere: theme, captions, then the environment.
        let theme = THEME.lock().unwrap_or_else(|e| e.into_inner());
        let captions = CAPTIONS.lock().unwrap_or_else(|e| e.into_inner());
        let sandbox = Sandbox::new();
        let mut settings = crate::config::parse_settings("");
        // Files go into the sandbox, never into the real Pictures and Videos folders.
        let folder = |name: &str| sandbox.path().join(name).to_string_lossy().into_owned();
        settings.screenshot_folder = folder("shots");
        settings.video_folder = folder("videos");
        prepare(&mut settings);
        crate::config::save_settings(&settings).unwrap();
        // SAFETY: the state and the windows live and die on this thread, one test at a time.
        unsafe {
            let dpi = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            crate::ui::set_theme_choice(settings.theme);
            let mut state = crate::initial_state(&settings);
            crate::STATE = &mut *state;
            crate::register_classes();
            let main = crate::create_main_window();
            assert!(!main.is_null(), "the tray window");
            assert!(crate::create_windows(), "the overlay and the toolbar");
            Desk {
                sandbox,
                state,
                main,
                dpi,
                _guards: [theme, captions],
            }
        }
    }

    /// Handles the messages that are waiting, including timers, for this long.
    pub fn pump(&self, duration: Duration) {
        self.pump_until(duration, || false);
    }

    /// Handles messages until `done` holds; false when the time ran out first.
    pub fn pump_until(&self, timeout: Duration, mut done: impl FnMut() -> bool) -> bool {
        let end = Instant::now() + timeout;
        loop {
            // SAFETY: plain message loop calls on this thread's queue.
            unsafe {
                let mut message: MSG = std::mem::zeroed();
                for _ in 0..MESSAGE_LIMIT {
                    if PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) == 0 {
                        break;
                    }
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
            if done() {
                return true;
            }
            if Instant::now() >= end {
                return false;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    pub fn send(&self, hwnd: HWND, message: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        // SAFETY: the window belongs to this thread.
        unsafe { SendMessageW(hwnd, message, wp, lp) }
    }

    /// A mouse message at client coordinates.
    pub fn mouse(&self, hwnd: HWND, message: u32, x: i32, y: i32) -> LRESULT {
        self.send(
            hwnd,
            message,
            0,
            ((y as u16 as isize) << 16) | (x as u16 as isize),
        )
    }

    pub fn click(&self, hwnd: HWND, x: i32, y: i32) {
        self.mouse(hwnd, WM_MOUSEMOVE, x, y);
        self.mouse(hwnd, WM_LBUTTONDOWN, x, y);
        self.mouse(hwnd, WM_LBUTTONUP, x, y);
    }

    /// Presses the left button at `from`, moves through `via` and releases at `to`.
    pub fn drag(&self, hwnd: HWND, from: (i32, i32), via: &[(i32, i32)], to: (i32, i32)) {
        self.mouse(hwnd, WM_MOUSEMOVE, from.0, from.1);
        self.mouse(hwnd, WM_LBUTTONDOWN, from.0, from.1);
        for &(x, y) in via {
            self.mouse(hwnd, WM_MOUSEMOVE, x, y);
        }
        self.mouse(hwnd, WM_MOUSEMOVE, to.0, to.1);
        self.mouse(hwnd, WM_LBUTTONUP, to.0, to.1);
    }

    pub fn key(&self, hwnd: HWND, key: u16) {
        self.send(hwnd, WM_KEYDOWN, key as WPARAM, 0);
    }
}

impl Drop for Desk {
    fn drop(&mut self) {
        // SAFETY: the windows were created by this desk on this thread.
        unsafe {
            if let Some(recording) = crate::state().recording.take() {
                let _ = recording.stop();
            }
            crate::cancel();
            crate::cancel_countdown();
            crate::toast::close();
            // このスレッドの見えている窓を隠す。途中で失敗したテストは、表示した窓（設定や履歴の
            // 代わりの窓など）を残す。見えている窓は描かれるまで WM_PAINT を出し続け、取り出しても
            // 消えないので、下でメッセージを捨てるループが終わらなくなる。隠した窓には WM_PAINT が
            // 来ない。取り消しは撮影のために隠した窓を戻すので、その後で隠す。
            EnumThreadWindows(
                GetWindowThreadProcessId(self.main, std::ptr::null_mut()),
                Some(hide),
                0,
            );
            for window in [
                std::mem::replace(&mut crate::state().record_window, std::ptr::null_mut()),
                std::mem::replace(&mut crate::state().record_frame, std::ptr::null_mut()),
                crate::state().toolbar,
                crate::state().overlay,
                self.main,
            ] {
                if !window.is_null() {
                    DestroyWindow(window);
                }
            }
            // Messages for the destroyed windows, such as the quit request, are not wanted.
            // 上限まで捨てても尽きなければ、そう書いて先へ進む（後片付けの中では panic しない）。
            let mut message: MSG = std::mem::zeroed();
            let emptied = (0..MESSAGE_LIMIT)
                .any(|_| PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) == 0);
            if !emptied {
                let _ = writeln!(
                    std::io::stderr(),
                    "Desk: {MESSAGE_LIMIT} messages were thrown away and more kept coming"
                );
            }
            crate::STATE = std::ptr::null_mut();
            if !self.dpi.is_null() {
                SetThreadDpiAwarenessContext(self.dpi);
            }
        }
        let _ = &self.state;
    }
}

mod tests;
