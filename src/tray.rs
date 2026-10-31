//! トレイのウィンドウ: アイコン、ショートカット、メニュー、設定の反映を受け持つ。

use super::*;

/// 保存された設定を動作中のトレイに反映する。設定ウィンドウからの通知に加え、撮影を始める
/// たびにも呼ぶので、通知を受け取りそこねても、ファイルに書かれた設定が必ず効く。
/// 撮影の遅延とフレームレートは設定から外したため、初期値（遅延なし、60 fps）のまま変えない。
pub(crate) unsafe fn apply_settings(settings: &config::Settings) {
    let s = state();
    s.copy = settings.copy;
    s.save = settings.save;
    s.ask = settings.ask;
    s.history = settings.history;
    s.countdown = settings.countdown;
    s.skip_title = settings.skip_title;
    s.record_cursor = settings.record_cursor;
    s.freeze = settings.freeze;
    s.notify = settings.notify;
    s.keep_toolbar = settings.keep_toolbar;
    s.outline = settings.outline;
    s.outline_color = settings.outline_color;
    s.outline_width = settings.outline_width;
    if !settings.keep_toolbar && s.toolbar_open {
        s.toolbar_open = false;
        if !s.active {
            ShowWindow(s.toolbar, SW_HIDE);
        }
    }
    s.mic = settings.mic;
    s.system_audio = settings.system_audio;
    s.app_audio = settings.app_audio;
    s.copy_video = settings.copy_video;
    s.save_video = settings.save_video;
    ui::set_theme_choice(settings.theme);
}

/// トレイがすでに動いていれば、その設定ウィンドウを開かせて true を返す。
/// 2 つのトレイが動くと、設定の変更が片方にしか届かず、もう片方が古い設定のまま撮影する。
pub(crate) unsafe fn hand_over_to_running_tray() -> bool {
    hand_over_to(FindWindowW(w(MAIN).as_ptr(), null()))
}

/// `running`（見つかったトレイのウィンドウ、なければ null）に設定ウィンドウを開かせる。
/// テストは、利用者が動かしている本物のトレイではなく、自分のトレイを渡して確かめる。
pub(crate) unsafe fn hand_over_to(running: HWND) -> bool {
    if running.is_null() {
        return false;
    }
    // 起動した側は前面に出る権利を持つので、開く設定ウィンドウに譲る。
    AllowSetForegroundWindow(ASFW_ANY);
    PostMessageW(running, WM_COMMAND, TRAY_SETTINGS, 0);
    true
}

pub(crate) unsafe extern "system" fn main_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            state().owner = hwnd;
            ui::disable_transitions(hwnd);
            tray(hwnd, NIM_ADD);
            state().hotkey_ok = RegisterHotKey(
                hwnd,
                HOTKEY,
                MOD_ALT | MOD_SHIFT | MOD_NOREPEAT,
                b'S' as u32,
            ) != 0;
            0
        }
        TRAY_RELOAD => {
            apply_settings(&config::load_settings());
            0
        }
        WM_COMMAND => {
            match wp & 0xffff {
                NEW => begin_capture(),
                TRAY_SETTINGS => launch_settings(),
                TRAY_HISTORY => launch_history(false),
                TRAY_STOP => stop_recording(),
                TRAY_ABOUT => {
                    if about::show().is_err() {
                        message(hwnd, tr("Could not open the About page."));
                    }
                }
                TRAY_EXIT => {
                    DestroyWindow(hwnd);
                }
                _ => {}
            }
            0
        }
        TRAY_MESSAGE => {
            if lp as u32 == WM_LBUTTONUP {
                begin_capture();
            }
            if lp as u32 == WM_RBUTTONUP {
                let menu = CreatePopupMenu();
                if state().recording.is_some() {
                    AppendMenuW(menu, MF_STRING, TRAY_STOP, w(tr("Stop recording")).as_ptr());
                }
                // The first entry carries the icon's name and opens the app's window.
                AppendMenuW(menu, MF_STRING, TRAY_SETTINGS, w("Snipping Lite").as_ptr());
                AppendMenuW(menu, MF_STRING, TRAY_ABOUT, w(tr("About")).as_ptr());
                AppendMenuW(menu, MF_STRING, TRAY_EXIT, w(tr("Exit")).as_ptr());
                SetMenuDefaultItem(menu, TRAY_SETTINGS as u32, 0);
                let mut cursor: POINT = std::mem::zeroed();
                GetCursorPos(&mut cursor);
                SetForegroundWindow(hwnd);
                TrackPopupMenu(menu, TPM_RIGHTBUTTON, cursor.x, cursor.y, 0, hwnd, null());
                DestroyMenu(menu);
            }
            0
        }
        WM_CLOSE => {
            ShowWindow(hwnd, SW_HIDE);
            0
        }
        WM_TIMER if wp == DELAY_TIMER => {
            KillTimer(hwnd, DELAY_TIMER);
            state().pending = false;
            capture(false);
            0
        }
        WM_HOTKEY if wp == HOTKEY as usize => {
            begin_capture();
            0
        }
        WM_DESTROY => {
            // Finish the video file so a recording is not lost when the app exits.
            if let Some(recording) = state().recording.take() {
                let _ = recording.stop();
            }
            toast::close();
            tray(hwnd, NIM_DELETE);
            UnregisterHotKey(hwnd, HOTKEY);
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
unsafe fn tray(hwnd: HWND, operation: u32) {
    let mut data: NOTIFYICONDATAW = std::mem::zeroed();
    data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    data.hWnd = hwnd;
    data.uID = 1;
    data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    data.uCallbackMessage = TRAY_MESSAGE;
    if state().tray_icon.is_null() {
        state().tray_icon = ui::app_icon(32);
    }
    data.hIcon = state().tray_icon;
    let title = w("Snipping Lite");
    data.szTip[..title.len()].copy_from_slice(&title);
    Shell_NotifyIconW(operation, &data);
    if operation == NIM_DELETE {
        DestroyIcon(state().tray_icon);
        state().tray_icon = null_mut();
    }
}
