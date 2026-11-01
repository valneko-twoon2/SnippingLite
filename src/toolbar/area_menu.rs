//! 範囲のドロップダウン。Snipping Tool と同じく、ツールバーと同じ見た目の窓に、項目ごとにアイコンと
//! 名前を並べ、いま選んでいる項目の左にアクセントの縦線を描く。標準のポップアップメニューは明るい
//! 枠が残るので使わず、自前の窓でメニューと同じ操作（上下キー、Enter、Escape、外のクリックで閉じる）
//! を受ける。

use super::*;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicIsize, Ordering};
use windows_sys::Win32::UI::{
    Input::KeyboardAndMouse::{VK_DOWN, VK_ESCAPE, VK_RETURN, VK_UP},
    WindowsAndMessaging::{MA_NOACTIVATE, WM_CAPTURECHANGED},
};

/// ドロップダウンの窓のクラス名。
pub(crate) const AREA_MENU: &str = "SLAreaMenu";

const ITEM_WIDTH: f32 = 168.0;
const ITEM_HEIGHT: f32 = 40.0;
const PADDING: f32 = 4.0;

/// ドロップダウンに並べる範囲（上から順）。
const MODES: [i32; 4] = [MODE_RECT, MODE_WINDOW, MODE_SCREEN, MODE_FREE];
/// 録画のときに並べる範囲。Snipping Tool と同じく、録画は四角形とウィンドウだけ。
const VIDEO_MODES: [i32; 2] = [MODE_RECT, MODE_WINDOW];

/// 強調している項目（マウスの下、またはキーで選んだもの）。なければ -1。
static HOT: AtomicI32 = AtomicI32::new(-1);
/// 選んだ範囲。選ばずに閉じたときは -1。
static PICK: AtomicI32 = AtomicI32::new(-1);
/// 閉じるか。
static DONE: AtomicBool = AtomicBool::new(false);
/// 最後に見たマウスの位置（WM_MOUSEMOVE の `lp`）。まだ見ていなければ `NO_MOVE`。
static LAST_MOVE: AtomicIsize = AtomicIsize::new(NO_MOVE);
const NO_MOVE: isize = isize::MIN;

/// 撮るもの（`video` なら録画）で選べる範囲。
fn modes(video: bool) -> &'static [i32] {
    if video {
        &VIDEO_MODES
    } else {
        &MODES
    }
}

/// 撮るものに合う範囲。選べない範囲なら四角形にする。
pub(super) fn fitting(mode: i32, video: bool) -> i32 {
    if modes(video).contains(&mode) {
        mode
    } else {
        MODE_RECT
    }
}

/// 撮るものが `video` のドロップダウンで、上から `position` 番目（0 から）の範囲。下キーは
/// 末尾から先頭へ戻るので、項目の数を超えた分は先頭から数え直す。
#[cfg(test)]
pub(crate) fn area_mode(position: usize, video: bool) -> i32 {
    let modes = modes(video);
    modes[position % modes.len()]
}

/// 範囲の名前。
fn label(mode: i32) -> &'static str {
    match mode {
        MODE_FREE => tr("Freeform"),
        MODE_WINDOW => tr("Window"),
        MODE_SCREEN => tr("Full screen"),
        _ => tr("Rectangle"),
    }
}

/// 範囲のアイコン。ツールバーのボタンにも同じものを描く。
pub(super) fn glyph(mode: i32) -> &'static str {
    match mode {
        MODE_FREE => "\u{EA49}",
        MODE_WINDOW => "\u{E7C4}",
        MODE_SCREEN => "\u{E7F4}",
        _ => "\u{EF20}",
    }
}

/// 項目 `position` の矩形（窓の中の座標）。
pub(super) fn item(position: usize, scale: f32) -> [f32; 4] {
    let top = (PADDING + position as f32 * ITEM_HEIGHT) * scale;
    [
        PADDING * scale,
        top,
        (PADDING + ITEM_WIDTH) * scale,
        top + ITEM_HEIGHT * scale,
    ]
}

/// `count` 個の項目のうち、`p`（窓の中の座標）の下の項目。なければ -1。
pub(super) fn item_at(p: POINT, scale: f32, count: usize) -> i32 {
    (0..count)
        .find(|&i| ui::contains(item(i, scale), p))
        .map_or(-1, |i| i as i32)
}

unsafe fn paint(hwnd: HWND, dc: HDC) {
    let mut client: RECT = std::mem::zeroed();
    GetClientRect(hwnd, &mut client);
    let Some(mut canvas) = ui::Canvas::new(client.right, client.bottom) else {
        return;
    };
    let s = state();
    let (theme, scale) = (&s.theme, s.scale);
    canvas.surface(theme.dark, scale);
    let icon = ui::Font::new(ui::ICON_FONT, 16.0 * scale, FW_NORMAL as i32);
    let text = ui::Font::new(ui::TEXT_FONT, 14.0 * scale, FW_NORMAL as i32);
    let hot = HOT.load(Ordering::Relaxed);
    for (position, &mode) in modes(s.video).iter().enumerate() {
        let [left, top, right, bottom] = item(position, scale);
        if position as i32 == hot {
            canvas.fill_round_rect([left, top, right, bottom], 4.0 * scale, theme.hover);
        }
        if mode == s.mode {
            let middle = (top + bottom) / 2.0;
            canvas.fill_round_rect(
                [
                    left,
                    middle - 8.0 * scale,
                    left + 3.0 * scale,
                    middle + 8.0 * scale,
                ],
                1.5 * scale,
                theme.accent,
            );
        }
        canvas.text(
            glyph(mode),
            &icon,
            ui::rect(left + 8.0 * scale, top, left + 36.0 * scale, bottom),
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            theme.text,
        );
        canvas.text(
            label(mode),
            &text,
            ui::rect(left + 44.0 * scale, top, right - 8.0 * scale, bottom),
            DT_LEFT | DT_VCENTER | DT_SINGLELINE,
            theme.text,
        );
    }
    canvas.present(dc);
}

/// 強調する項目を変える。
unsafe fn highlight(hwnd: HWND, position: i32) {
    if HOT.swap(position, Ordering::Relaxed) != position {
        InvalidateRect(hwnd, null(), 0);
    }
}

/// 閉じる。最初に決めたものを残す（閉じるときのキャプチャの解除でも呼ばれるため）。
fn finish(pick: i32) {
    if !DONE.swap(true, Ordering::Relaxed) {
        PICK.store(pick, Ordering::Relaxed);
    }
}

pub(crate) unsafe extern "system" fn area_menu_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let dc = BeginPaint(hwnd, &mut ps);
            paint(hwnd, dc);
            EndPaint(hwnd, &ps);
            0
        }
        WM_MOUSEMOVE => {
            // 窓を出した直後や、同じ位置のまま届いた分では、キーで選んだ項目を動かさない。
            let before = LAST_MOVE.swap(lp, Ordering::Relaxed);
            if before != NO_MOVE && before != lp {
                let count = modes(state().video).len();
                highlight(hwnd, item_at(point(lp), state().scale, count));
            }
            0
        }
        WM_LBUTTONDOWN => {
            // 窓の外を押したら、選ばずに閉じる。
            let mut client: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut client);
            let p = point(lp);
            if p.x < 0 || p.y < 0 || p.x >= client.right || p.y >= client.bottom {
                finish(-1);
            }
            0
        }
        WM_LBUTTONUP => {
            let modes = modes(state().video);
            let position = item_at(point(lp), state().scale, modes.len());
            if let Some(&mode) = modes.get(position as usize) {
                finish(mode);
            }
            0
        }
        // ほかの窓に奪われたら、選ばずに閉じる。
        WM_CAPTURECHANGED => {
            finish(-1);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

/// キーで強調する項目を動かす。Enter で選び、Escape で選ばずに閉じる。メニューと同じく、
/// 窓ではなくスレッドに届いたキーも受ける。
unsafe fn key(popup: HWND, key: u16) {
    let hot = HOT.load(Ordering::Relaxed);
    let modes = modes(state().video);
    let count = modes.len() as i32;
    match key {
        VK_DOWN => highlight(popup, (hot + 1).rem_euclid(count)),
        VK_UP => highlight(popup, if hot <= 0 { count - 1 } else { hot - 1 }),
        VK_RETURN if hot >= 0 => finish(modes.get(hot as usize).copied().unwrap_or(-1)),
        VK_ESCAPE => finish(-1),
        _ => {}
    }
}

/// 範囲のボタンの下にドロップダウンを出し、選んだ範囲（なければ `None`）を返す。
unsafe fn pick(hwnd: HWND) -> Option<i32> {
    let scale = state().scale;
    let width = ((ITEM_WIDTH + 2.0 * PADDING) * scale).round() as i32;
    let count = modes(state().video).len();
    let height = ((count as f32 * ITEM_HEIGHT + 2.0 * PADDING) * scale).round() as i32;
    let button = toolbar_button(AREA_BUTTON, scale);
    let mut anchor = POINT {
        x: button[0] as i32,
        y: button[3] as i32,
    };
    ClientToScreen(hwnd, &mut anchor);
    let popup = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
        w(AREA_MENU).as_ptr(),
        w(AREA_MENU).as_ptr(),
        WS_POPUP,
        anchor.x,
        anchor.y,
        width,
        height,
        hwnd,
        null_mut(),
        GetModuleHandleW(null()),
        null(),
    );
    if popup.is_null() {
        return None;
    }
    HOT.store(-1, Ordering::Relaxed);
    PICK.store(-1, Ordering::Relaxed);
    DONE.store(false, Ordering::Relaxed);
    LAST_MOVE.store(NO_MOVE, Ordering::Relaxed);
    ui::acrylic(popup, state().theme.dark);
    ui::disable_transitions(popup);
    ShowWindow(popup, SW_SHOWNOACTIVATE);
    UpdateWindow(popup);
    SetCapture(popup);
    let mut quit = None;
    while !DONE.load(Ordering::Relaxed) {
        let mut msg: MSG = std::mem::zeroed();
        let got = GetMessageW(&mut msg, null_mut(), 0, 0);
        if got == 0 {
            quit = Some(msg.wParam);
            break;
        }
        if got < 0 {
            break;
        }
        if msg.message == WM_KEYDOWN {
            key(popup, msg.wParam as u16);
            continue;
        }
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
    ReleaseCapture();
    DestroyWindow(popup);
    if let Some(code) = quit {
        PostQuitMessage(code as i32);
    }
    let picked = PICK.load(Ordering::Relaxed);
    (picked >= 0).then_some(picked)
}

/// 範囲のボタンの下にドロップダウンを出し、選んだ範囲に切り替える。
pub(super) unsafe fn choose(hwnd: HWND) {
    SetForegroundWindow(hwnd);
    if let Some(mode) = pick(hwnd) {
        if state().active {
            set_mode(mode);
        } else {
            // A kept toolbar between snips starts a new snip in the chosen mode.
            state().mode = mode;
            begin_capture();
            if state().active {
                set_mode(mode);
            }
        }
    }
    if state().active {
        SetForegroundWindow(state().overlay);
    }
}
