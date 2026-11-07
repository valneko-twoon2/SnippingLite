use crate::{state, ui};
use windows_sys::Win32::{Foundation::HWND, UI::WindowsAndMessaging::*};

// Both document windows share a caption; properties identify their roles.
pub(crate) const SETTINGS_TITLE: &str = "Snipping Lite";
pub(crate) const HISTORY_TITLE: &str = SETTINGS_TITLE;
pub(crate) const PLAYER_TITLE: &str = SETTINGS_TITLE;
pub(crate) const SETTINGS_WINDOW: &str = "SnippingLite.Settings";
pub(crate) const HISTORY_WINDOW: &str = "SnippingLite.History";
pub(crate) const PLAYER_WINDOW: &str = "SnippingLite.Player";
pub(crate) const WINDOWS: [&str; 3] = [SETTINGS_WINDOW, HISTORY_WINDOW, PLAYER_WINDOW];
/// 開いている（または隠して待たせている）履歴の窓に、最新の画像を開くよう頼むプロパティ。
pub(crate) const LATEST_REQUEST: &str = "SnippingLite.Latest";
/// 開いている設定の窓に、ショートカットを登録できているかを知らせるプロパティと、その値。
pub(crate) const HOTKEY_STATE: &str = "SnippingLite.Hotkey";
pub(crate) const HOTKEY_OK: usize = 1;
pub(crate) const HOTKEY_BUSY: usize = 2;
/// トレイと設定・履歴・プレイヤーの窓が互いを探すプロセス（`None` はすべて）。どれも別のプロセスで
/// 動くので、製品はすべてのプロセスから探す。テストは代わりの窓（`stub_window`）やトレイを自分の
/// プロセスに作るので、自分のプロセスだけから探す。同時に動くほかのテストのプロセスの窓や、利用者が
/// 動かしている Snipping Lite の窓を見つけて、隠したり、前に出したり、知らせを送ったりしない。
pub(crate) fn searched_process() -> Option<u32> {
    cfg!(test).then(std::process::id)
}
pub(crate) unsafe fn find_window(identity: &str) -> HWND {
    ui::find_app_window(identity, searched_process())
}
/// Opens a window process (`--settings` or `--history`), or brings the running one forward.
/// 閉じて隠してある窓（`standby`）も、ここで表示し直すだけで、プロセスは起こし直さない。
/// `latest` asks the history window to open the newest screenshot.
pub(crate) unsafe fn launch_window(identity: &str, argument: &str, latest: bool) {
    let window = find_window(identity);
    if !window.is_null() {
        if latest {
            SetPropW(window, crate::w(LATEST_REQUEST).as_ptr(), 1usize as _);
        }
        if identity == SETTINGS_WINDOW {
            let hotkey = if state().hotkey_ok {
                HOTKEY_OK
            } else {
                HOTKEY_BUSY
            };
            SetPropW(window, crate::w(HOTKEY_STATE).as_ptr(), hotkey as _);
        }
        ShowWindow(window, SW_SHOW);
        // 開いたままの窓も、頼んだことをすぐ描き直して反映するよう、描画を促す。
        windows_sys::Win32::Graphics::Gdi::InvalidateRect(window, std::ptr::null(), 0);
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
/// Opens a recording in a player window of its own.
pub(crate) fn launch_player(path: &std::path::Path) {
    #[cfg(test)]
    crate::test_support::shell_opened(
        &crate::w(&path.to_string_lossy()),
        Some(&crate::w("--player")),
    );
    #[cfg(not(test))]
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe)
            .arg("--player")
            .arg(path)
            .spawn();
    }
}
pub(crate) unsafe fn launch_settings() {
    launch_window(SETTINGS_WINDOW, "--settings", false);
}
pub(crate) unsafe fn launch_history(latest: bool) {
    launch_window(HISTORY_WINDOW, "--history", latest);
}
/// Runs the window process the arguments ask for (`--history`, `--player <file>` or
/// `--settings`); false when this is the tray process.
pub(crate) fn run_window_process(args: &[String]) -> bool {
    let has = |flag: &str| args.iter().any(|a| a == flag);
    if has("--history") {
        crate::history_ui::run(has("--latest"));
    } else if let Some(at) = args.iter().position(|a| a == "--player") {
        if let Some(path) = args.get(at + 1) {
            crate::player::run(path.into());
        }
    } else if has("--settings") {
        crate::settings_ui::run(has("--hotkey-ok"));
    } else {
        return false;
    }
    true
}

#[cfg(test)]
mod tests;
