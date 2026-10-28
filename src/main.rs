#![windows_subsystem = "windows"]

mod annotate;
mod config;
mod history;
mod history_ui;
mod i18n;
mod imaging;
mod overlay;
mod record;
mod screenshot;
mod settings_ui;
mod toast;
mod toolbar;
mod ui;
mod video;
mod window_capture;

use i18n::tr;
use overlay::*;
use screenshot::*;
use std::{
    path::PathBuf,
    ptr::{null, null_mut},
};
use toolbar::*;
use video::*;
use windows_sys::{
    core::PCWSTR,
    Win32::{
        Foundation::*,
        Graphics::{
            Dwm::{DwmFlush, DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS},
            Gdi::*,
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Controls::WM_MOUSELEAVE, HiDpi::*, Input::KeyboardAndMouse::*, Shell::*,
            WindowsAndMessaging::*,
        },
    },
};

const MAIN: &str = "SnippingLiteMain";
const OVERLAY: &str = "SnippingLiteOverlay";
const TOOLBAR: &str = "SnippingLiteToolbar";
const RECORD: &str = "SnippingLiteRecord";
const FRAME: &str = "SnippingLiteFrame";
const COUNT: &str = "SnippingLiteCount";
const NEW: usize = 100;
const DELAY_TIMER: usize = 1;
const RECORD_TIMER: usize = 2;
const COUNT_TIMER: usize = 3;
const HOTKEY: i32 = 1;
const TRAY_MESSAGE: u32 = WM_APP + 1;
const TRAY_RELOAD: u32 = WM_APP + 2;
const TRAY_SETTINGS: usize = 201;
const TRAY_HISTORY: usize = 203;
const TRAY_EXIT: usize = 202;
const TRAY_STOP: usize = 204;
const MODE_RECT: i32 = 0;
const MODE_FREE: i32 = 1;
const MODE_WINDOW: i32 = 2;
const MODE_SCREEN: i32 = 3;

struct State {
    owner: HWND,
    overlay: HWND,
    screen: HBITMAP,
    screen_dc: HDC,
    previous: HGDIOBJ,
    dim_canvas: Option<ui::Canvas>,
    toolbar: HWND,
    toolbar_dark: Option<bool>,
    active: bool,
    hover_button: i32,
    pressed_button: i32,
    hover_window: Option<RECT>,
    hover_hwnd: HWND,
    scale: f32,
    theme: ui::Theme,
    toast: Option<Box<toast::Toast>>,
    origin: POINT,
    width: i32,
    height: i32,
    mode: i32,
    video: bool,
    start: POINT,
    end: POINT,
    points: Vec<POINT>,
    dragging: bool,
    hidden: Vec<&'static str>,
    copy: bool,
    save: bool,
    ask: bool,
    delay: u32,
    pending: bool,
    history: bool,
    hotkey_ok: bool,
    recording: Option<record::Recording>,
    record_window: HWND,
    record_frame: HWND,
    tray_icon: HICON,
    /// The live overlay content; set instead of the frozen screen when `freeze` is off.
    live: Option<ui::Canvas>,
    freeze: bool,
    notify: bool,
    mic: bool,
    system_audio: bool,
    app_audio: bool,
    ask_video: bool,
    countdown: bool,
    skip_title: bool,
    record_cursor: bool,
    record_fps: u32,
    count_window: HWND,
    count_left: i32,
    count_area: RECT,
    count_target: Option<window_capture::Target>,
    count_points: Vec<POINT>,
}
static mut STATE: *mut State = null_mut();
fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
unsafe fn state() -> &'static mut State {
    &mut *STATE
}
unsafe fn message(hwnd: HWND, text: &str) {
    MessageBoxW(
        hwnd,
        w(text).as_ptr(),
        w("Snipping Lite").as_ptr(),
        MB_OK | MB_ICONINFORMATION,
    );
}
// Both document windows share a caption; properties identify their roles.
const SETTINGS_TITLE: &str = "Snipping Lite";
const HISTORY_TITLE: &str = SETTINGS_TITLE;
const SETTINGS_WINDOW: &str = "SnippingLite.Settings";
const HISTORY_WINDOW: &str = "SnippingLite.History";
const WINDOWS: [&str; 2] = [SETTINGS_WINDOW, HISTORY_WINDOW];
unsafe fn find_window(identity: &str) -> HWND {
    ui::find_app_window(identity)
}
/// Opens a window process (`--settings` or `--history`), or brings the running one forward.
/// `latest` asks a new history window to open the newest screenshot.
unsafe fn launch_window(identity: &str, argument: &str, latest: bool) {
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
unsafe fn launch_settings() {
    launch_window(SETTINGS_WINDOW, "--settings", false);
}
unsafe fn launch_history(latest: bool) {
    launch_window(HISTORY_WINDOW, "--history", latest);
}
fn point(lp: LPARAM) -> POINT {
    POINT {
        x: (lp as u32 as u16 as i16) as i32,
        y: ((lp as u32 >> 16) as u16 as i16) as i32,
    }
}
fn bounds(a: POINT, b: POINT, width: i32, height: i32) -> RECT {
    RECT {
        left: a.x.min(b.x).clamp(0, width),
        top: a.y.min(b.y).clamp(0, height),
        right: a.x.max(b.x).clamp(0, width),
        bottom: a.y.max(b.y).clamp(0, height),
    }
}
fn inflate(r: RECT, by: i32) -> RECT {
    RECT {
        left: r.left - by,
        top: r.top - by,
        right: r.right + by,
        bottom: r.bottom + by,
    }
}
/// Creates the hidden overlay and toolbar once; they are reused for every capture.
unsafe fn create_windows() -> bool {
    let s = state();
    if s.overlay.is_null() {
        s.overlay = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            w(OVERLAY).as_ptr(),
            w(OVERLAY).as_ptr(),
            WS_POPUP,
            0,
            0,
            1,
            1,
            null_mut(),
            null_mut(),
            GetModuleHandleW(null()),
            null(),
        );
    }
    if !s.overlay.is_null() && s.toolbar.is_null() {
        s.toolbar = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            w(TOOLBAR).as_ptr(),
            w(TOOLBAR).as_ptr(),
            WS_POPUP,
            0,
            0,
            1,
            1,
            s.overlay,
            null_mut(),
            GetModuleHandleW(null()),
            null(),
        );
    }
    !s.overlay.is_null() && !s.toolbar.is_null()
}
unsafe extern "system" fn main_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
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
            let settings = config::load_settings();
            state().copy = settings.copy;
            state().save = settings.save;
            state().ask = settings.ask;
            state().delay = settings.delay;
            state().history = settings.history;
            state().countdown = settings.countdown;
            state().skip_title = settings.skip_title;
            state().record_cursor = settings.record_cursor;
            state().record_fps = settings.record_fps;
            state().freeze = settings.freeze;
            state().notify = settings.notify;
            state().mic = settings.mic;
            state().system_audio = settings.system_audio;
            state().app_audio = settings.app_audio;
            state().ask_video = settings.ask_video;
            ui::set_theme_choice(settings.theme);
            0
        }
        WM_COMMAND => {
            match wp & 0xffff {
                NEW => begin_capture(),
                TRAY_SETTINGS => launch_settings(),
                TRAY_HISTORY => launch_history(false),
                TRAY_STOP => stop_recording(),
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
                AppendMenuW(menu, MF_STRING, NEW, w(tr("New snip")).as_ptr());
                AppendMenuW(menu, MF_STRING, TRAY_HISTORY, w(tr("History")).as_ptr());
                AppendMenuW(menu, MF_STRING, TRAY_SETTINGS, w(tr("Settings")).as_ptr());
                AppendMenuW(menu, MF_SEPARATOR, 0, null());
                AppendMenuW(menu, MF_STRING, TRAY_EXIT, w(tr("Exit")).as_ptr());
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
unsafe fn register(name: &str, proc: WNDPROC, brush: HBRUSH, cursor: PCWSTR) {
    let class_name = w(name);
    let class = WNDCLASSW {
        lpfnWndProc: proc,
        hInstance: GetModuleHandleW(null()),
        hIcon: LoadIconW(GetModuleHandleW(null()), 1usize as PCWSTR),
        lpszClassName: class_name.as_ptr(),
        hCursor: LoadCursorW(null_mut(), cursor),
        hbrBackground: brush,
        ..std::mem::zeroed()
    };
    RegisterClassW(&class);
}
fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--history") {
        history_ui::run(args.iter().any(|a| a == "--latest"));
        return;
    }
    if args.iter().any(|a| a == "--settings") {
        settings_ui::run(args.iter().any(|a| a == "--hotkey-ok"));
        return;
    }
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let settings = config::load_settings();
        ui::set_theme_choice(settings.theme);
        let mut app = Box::new(State {
            owner: null_mut(),
            overlay: null_mut(),
            screen: null_mut(),
            screen_dc: null_mut(),
            previous: null_mut(),
            dim_canvas: None,
            toolbar: null_mut(),
            toolbar_dark: None,
            active: false,
            hover_button: -1,
            pressed_button: -1,
            hover_window: None,
            hover_hwnd: null_mut(),
            scale: 1.0,
            theme: ui::theme(),
            toast: None,
            origin: POINT { x: 0, y: 0 },
            width: 0,
            height: 0,
            mode: MODE_RECT,
            video: false,
            start: POINT { x: 0, y: 0 },
            end: POINT { x: 0, y: 0 },
            points: Vec::new(),
            dragging: false,
            hidden: Vec::new(),
            copy: settings.copy,
            save: settings.save,
            ask: settings.ask,
            delay: settings.delay,
            pending: false,
            history: settings.history,
            hotkey_ok: false,
            recording: None,
            record_window: null_mut(),
            record_frame: null_mut(),
            tray_icon: null_mut(),
            live: None,
            freeze: settings.freeze,
            notify: settings.notify,
            mic: settings.mic,
            system_audio: settings.system_audio,
            app_audio: settings.app_audio,
            ask_video: settings.ask_video,
            countdown: settings.countdown,
            skip_title: settings.skip_title,
            record_cursor: settings.record_cursor,
            record_fps: settings.record_fps,
            count_window: null_mut(),
            count_left: 0,
            count_points: Vec::new(),
            count_target: None,
            count_area: RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
        });
        STATE = &mut *app;
        register(MAIN, Some(main_proc), null_mut(), IDC_ARROW);
        register(OVERLAY, Some(overlay_proc), null_mut(), IDC_CROSS);
        register(TOOLBAR, Some(toolbar_proc), null_mut(), IDC_ARROW);
        register(RECORD, Some(record_proc), null_mut(), IDC_ARROW);
        register(COUNT, Some(count_proc), null_mut(), IDC_ARROW);
        // The frame around a recording is a solid red window shaped like a ring.
        register(
            FRAME,
            Some(DefWindowProcW),
            CreateSolidBrush(0x002311e8),
            IDC_ARROW,
        );
        register(toast::CLASS, Some(toast::proc), null_mut(), IDC_HAND);
        // The tray window is never shown; it receives the tray, hotkey and timer messages.
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            w(MAIN).as_ptr(),
            w(MAIN).as_ptr(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            GetModuleHandleW(null()),
            null(),
        );
        if hwnd.is_null() {
            return;
        }
        create_windows();
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        STATE = null_mut();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection_bounds_are_sorted_and_clipped() {
        let r = bounds(POINT { x: 140, y: 40 }, POINT { x: -10, y: 10 }, 100, 100);
        assert_eq!((r.left, r.top, r.right, r.bottom), (0, 10, 100, 40));
    }
}
