#![windows_subsystem = "windows"]

mod about;
mod annotate;
mod config;
mod extract;
mod history;
mod history_ui;
mod i18n;
mod imaging;
mod launch;
#[cfg(test)]
mod leaks;
mod markup;
#[cfg(test)]
mod monkey;
mod ocr;
mod overlay;
mod player;
mod record;
mod screenshot;
mod settings_ui;
mod staging;
#[cfg(test)]
mod test_support;
mod toast;
mod toolbar;
mod tooltip;
mod tray;
mod ui;
mod video;
mod window_capture;

use i18n::tr;
pub(crate) use launch::*;
use overlay::*;
use screenshot::*;
use std::{
    path::PathBuf,
    ptr::{null, null_mut},
};
use toolbar::*;
use tray::*;
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
const RAISE_TIMER: usize = 4;
const HOTKEY: i32 = 1;
const TRAY_MESSAGE: u32 = WM_APP + 1;
const TRAY_RELOAD: u32 = WM_APP + 2;
const TRAY_SETTINGS: usize = 201;
const TRAY_HISTORY: usize = 203;
const TRAY_EXIT: usize = 202;
const TRAY_STOP: usize = 204;
const TRAY_ABOUT: usize = 205;
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
    /// The toolbar stays on the screen between snips until it is closed.
    keep_toolbar: bool,
    /// Screenshots get a border; the color and width are indexes of the settings.
    outline: bool,
    outline_color: usize,
    outline_width: usize,
    /// The kept toolbar is open; it returns after each snip or recording.
    toolbar_open: bool,
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
    /// 録画が終わったら、そのファイルをクリップボードに置く。
    copy_video: bool,
    /// 録画を一時フォルダーではなく録画の保存先に残す。
    save_video: bool,
    countdown: bool,
    skip_title: bool,
    record_cursor: bool,
    record_fps: u32,
    count_window: HWND,
    count_left: i32,
    count_area: RECT,
    count_target: Option<window_capture::Target>,
    count_points: Vec<POINT>,
    /// ツールバーのペンのボタン: 切り取った範囲にその場で書き込んでから確定する。
    markup: bool,
    /// ツールバーの文字のボタン: 切り取った範囲の文字を読み取ってクリップボードへ置く。
    extract: bool,
    /// 書き込み中の切り取り。
    markup_session: Option<Box<markup::Session>>,
    markup_bar: HWND,
    /// ツールバーと書き込みのバーのボタンのヒント。
    tip: tooltip::Tip,
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
        // 残したツールバーを、撮影のたびに隠して出し直さなくても写らないようにする。
        if !s.toolbar.is_null() {
            SetWindowDisplayAffinity(s.toolbar, WDA_EXCLUDEFROMCAPTURE);
        }
    }
    !s.overlay.is_null() && !s.toolbar.is_null()
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
    // 登録できなかった（すでに登録してあるなどの）クラスは、背景のブラシを持たない。持ち主の
    // いないブラシは消す。登録できたクラスのブラシは、クラスとともにシステムが消す。
    if RegisterClassW(&class) == 0 && !brush.is_null() {
        DeleteObject(brush);
    }
}
/// The state of a fresh tray process, taken from the saved settings.
fn initial_state(settings: &config::Settings) -> Box<State> {
    Box::new(State {
        owner: null_mut(),
        overlay: null_mut(),
        screen: null_mut(),
        screen_dc: null_mut(),
        previous: null_mut(),
        dim_canvas: None,
        toolbar: null_mut(),
        toolbar_dark: None,
        keep_toolbar: settings.keep_toolbar,
        outline: settings.outline,
        outline_color: settings.outline_color,
        outline_width: settings.outline_width,
        toolbar_open: false,
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
        delay: 0,
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
        copy_video: settings.copy_video,
        save_video: settings.save_video,
        countdown: settings.countdown,
        skip_title: settings.skip_title,
        record_cursor: settings.record_cursor,
        record_fps: 60,
        count_window: null_mut(),
        count_left: 0,
        count_points: Vec::new(),
        markup: config::load_markup(),
        extract: false,
        markup_session: None,
        markup_bar: null_mut(),
        tip: tooltip::Tip::new(),
        count_target: None,
        count_area: RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        },
    })
}
/// Registers the window classes of the tray, the overlay, the toolbar and the recording windows.
unsafe fn register_classes() {
    register(MAIN, Some(main_proc), null_mut(), IDC_ARROW);
    register(OVERLAY, Some(overlay_proc), null_mut(), IDC_CROSS);
    register(TOOLBAR, Some(toolbar_proc), null_mut(), IDC_ARROW);
    register(AREA_MENU, Some(area_menu_proc), null_mut(), IDC_ARROW);
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
    register(markup::BAR, Some(markup::bar_proc), null_mut(), IDC_ARROW);
    register(tooltip::CLASS, Some(tooltip::proc), null_mut(), IDC_ARROW);
}
/// Creates the tray window, which is never shown; it receives the tray, hotkey and timer messages.
unsafe fn create_main_window() -> HWND {
    CreateWindowExW(
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
    )
}
fn main() {
    let args: Vec<String> = std::env::args().collect();
    // The installer asks for the version to list the app in Settings > Apps.
    if args.iter().any(|a| a == "--version") {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if run_window_process(&args) {
        return;
    }
    unsafe {
        if hand_over_to_running_tray() {
            return;
        }
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let settings = config::load_settings();
        ui::set_theme_choice(settings.theme);
        let mut app = initial_state(&settings);
        STATE = &mut *app;
        register_classes();
        if create_main_window().is_null() {
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
mod tests;
