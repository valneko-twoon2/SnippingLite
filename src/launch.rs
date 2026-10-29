use crate::{state, ui};
use windows_sys::Win32::{Foundation::HWND, UI::WindowsAndMessaging::*};

// Both document windows share a caption; properties identify their roles.
pub(crate) const SETTINGS_TITLE: &str = "Snipping Lite";
pub(crate) const HISTORY_TITLE: &str = SETTINGS_TITLE;
pub(crate) const SETTINGS_WINDOW: &str = "SnippingLite.Settings";
pub(crate) const HISTORY_WINDOW: &str = "SnippingLite.History";
pub(crate) const WINDOWS: [&str; 2] = [SETTINGS_WINDOW, HISTORY_WINDOW];
pub(crate) unsafe fn find_window(identity: &str) -> HWND {
    ui::find_app_window(identity)
}
/// Opens a window process (`--settings` or `--history`), or brings the running one forward.
/// `latest` asks a new history window to open the newest screenshot.
pub(crate) unsafe fn launch_window(identity: &str, argument: &str, latest: bool) {
    let window = find_window(identity);
    if !window.is_null() {
        ShowWindow(window, SW_SHOW);
        SetForegroundWindow(window);
        return;
    }
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe)
            .arg(argument)
            .args(latest.then_some("--latest"))
            .arg(if state().hotkey_ok {
                "--hotkey-ok"
            } else {
                "--hotkey-busy"
            })
            .spawn();
    }
}
pub(crate) unsafe fn launch_settings() {
    launch_window(SETTINGS_WINDOW, "--settings", false);
}
pub(crate) unsafe fn launch_history(latest: bool) {
    launch_window(HISTORY_WINDOW, "--history", latest);
}
