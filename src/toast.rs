//! Notification shown after a capture, styled like a Windows 11 toast.

use std::{
    path::PathBuf,
    ptr::{null, null_mut},
    time::Instant,
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    UI::{
        Controls::WM_MOUSELEAVE,
        Input::KeyboardAndMouse::{TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT},
        WindowsAndMessaging::*,
    },
};

use crate::i18n::tr;
use crate::{
    point, state,
    ui::{self, Canvas, Font},
    w,
};

mod paint;
use paint::paint;

pub const CLASS: &str = "SnippingLiteToast";
const ANIMATION: usize = 1;
const DISMISS: usize = 2;
const WIDTH: f32 = 364.0;
const PADDING: f32 = 16.0;
const HEADER: f32 = 20.0;
const SLIDE_MS: f32 = 250.0;
const VISIBLE_MS: u32 = 6000;

pub struct Toast {
    hwnd: HWND,
    title: &'static str,
    body: String,
    path: Option<PathBuf>,
    can_open: bool,
    edit: bool,
    action_hover: bool,
    menu_hover: bool,
    scale: f32,
    left: i32,
    right: i32,
    top: i32,
    started: Instant,
    closing: bool,
    hover: bool,
    close_hover: bool,
}

/// Shows a notification for a finished capture, replacing an existing one.
pub unsafe fn show(pixels: &[u8], width: i32, height: i32, copied: bool, path: Option<PathBuf>) {
    let (title, body) = match (copied, &path) {
        (true, Some(_)) => (
            tr("Snip copied to clipboard"),
            tr("Saved automatically to the Screenshots folder").to_string(),
        ),
        (true, None) => (tr("Snip copied to clipboard"), String::new()),
        (false, Some(p)) => (tr("Snip saved"), display_name(p)),
        (false, None) => return,
    };
    present(pixels, width, height, title, body, path);
}

/// Shows a notification for a finished recording; `pixels` is its first frame.
/// `saved` が false なら、録画は一時フォルダーにあり、クリップボードにだけ渡した。
pub unsafe fn show_recording(pixels: &[u8], width: i32, height: i32, path: PathBuf, saved: bool) {
    let body = display_name(&path);
    let title = if saved {
        tr("Recording saved")
    } else {
        tr("Recording copied to clipboard")
    };
    present(pixels, width, height, title, body, Some(path));
}

/// 決めた名前（同じ名前のファイルが開かれているなど）に置けず、別の名前で残した録画を知らせる。
pub unsafe fn show_moved_recording(pixels: &[u8], width: i32, height: i32, path: PathBuf) {
    let body = display_name(&path);
    let title = tr("Recording saved under another name");
    present(pixels, width, height, title, body, Some(path));
}

/// 通知の置き場所（画面の座標）。出ている間の左端 `left`、滑り込む前と滑り出た後の左端 `right`
/// （作業領域の右端の外）、上端、幅と高さ。
struct Frame {
    left: i32,
    right: i32,
    top: i32,
    width: i32,
    height: i32,
}

/// 作業領域 `work` の右下に、端から 12 DIP 空けて置く。本文があれば 20 DIP 高い。
fn frame(work: RECT, scale: f32, body: bool) -> Frame {
    let height = if body { 156.0 } else { 136.0 };
    let (width, height) = (
        (WIDTH * scale).round() as i32,
        (height * scale).round() as i32,
    );
    let margin = (12.0 * scale).round() as i32;
    Frame {
        left: work.right - margin - width,
        right: work.right,
        top: work.bottom - margin - height,
        width,
        height,
    }
}

/// 通知を出す作業領域（主モニターの、タスクバーを除いた所）と、その右下の倍率。
unsafe fn home() -> (RECT, f32) {
    let mut work: RECT = std::mem::zeroed();
    SystemParametersInfoW(SPI_GETWORKAREA, 0, (&mut work as *mut RECT).cast(), 0);
    let scale = ui::scale_at(POINT {
        x: work.right - 1,
        y: work.bottom - 1,
    });
    (work, scale)
}

/// 通知を倍率 `scale` で作り直し、今の作業領域の右下へ置き直す。滑っている途中なら横の位置は
/// そのままにして、次の刻みから新しい位置へ滑らせる。
unsafe fn relayout(toast: &mut Toast, scale: f32) {
    let f = frame(home().0, scale, !toast.body.is_empty());
    (toast.scale, toast.left, toast.right, toast.top) = (scale, f.left, f.right, f.top);
    let sliding = toast.closing || toast.started.elapsed().as_secs_f32() * 1000.0 < SLIDE_MS;
    let mut bounds: RECT = std::mem::zeroed();
    GetWindowRect(toast.hwnd, &mut bounds);
    let x = if sliding { bounds.left } else { f.left };
    SetWindowPos(
        toast.hwnd,
        null_mut(),
        x,
        f.top,
        f.width,
        f.height,
        SWP_NOZORDER | SWP_NOACTIVATE,
    );
    InvalidateRect(toast.hwnd, null(), 0);
}

/// ディスプレイの構成が変わった（解像度、モニターの追加と取り外し、主モニターの交代）: 出ている
/// 通知を、新しい作業領域の右下へ、そこの倍率で置き直す。消えるまでの時間は変えない。
pub unsafe fn display_changed() {
    let scale = home().1;
    if let Some(toast) = state().toast.as_deref_mut() {
        relayout(toast, scale);
    }
}

unsafe fn present(
    _pixels: &[u8],
    _width: i32,
    _height: i32,
    title: &'static str,
    body: String,
    path: Option<PathBuf>,
) {
    close();
    let (work, scale) = home();
    let place = frame(work, scale, !body.is_empty());
    let hwnd = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
        w(CLASS).as_ptr(),
        w("Snipping Lite Notification").as_ptr(),
        WS_POPUP,
        place.right,
        place.top,
        place.width,
        place.height,
        null_mut(),
        null_mut(),
        null_mut(),
        null(),
    );
    if hwnd.is_null() {
        return;
    }
    state().toast = Some(Box::new(Toast {
        hwnd,
        title,
        body,
        can_open: path.is_some() || state().history,
        // 録画はエディターではなくプレイヤーで開く。
        edit: !path
            .as_ref()
            .is_some_and(|p| p.extension().is_some_and(|e| e == "mp4"))
            && state().history,
        action_hover: false,
        menu_hover: false,
        path,
        scale,
        left: place.left,
        right: place.right,
        top: place.top,
        started: Instant::now(),
        closing: false,
        hover: false,
        close_hover: false,
    }));
    ui::acrylic(hwnd, ui::theme().dark);
    ui::disable_transitions(hwnd);
    ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    SetTimer(hwnd, ANIMATION, 10, None);
    SetTimer(hwnd, DISMISS, VISIBLE_MS, None);
}

fn display_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Removes the notification immediately.
pub unsafe fn close() {
    if let Some(toast) = state().toast.take() {
        DestroyWindow(toast.hwnd);
    }
}

pub unsafe fn visible() -> bool {
    state().toast.is_some()
}

unsafe fn begin_closing(toast: &mut Toast) {
    if !toast.closing {
        toast.closing = true;
        toast.started = Instant::now();
        KillTimer(toast.hwnd, DISMISS);
        SetTimer(toast.hwnd, ANIMATION, 10, None);
    }
}

fn close_button(scale: f32, width: i32) -> RECT {
    let size = 32.0 * scale;
    let right = width as f32 - 4.0 * scale;
    ui::rect(right - size, 8.0 * scale, right, 8.0 * scale + size)
}

pub unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let Some(toast) = state().toast.as_deref_mut().filter(|t| t.hwnd == hwnd) else {
        return DefWindowProcW(hwnd, msg, wp, lp);
    };
    match msg {
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        WM_ERASEBKGND => 1,
        // 通知のあるモニターの拡大率が変わった。新しい倍率で作り直し、作業領域の右下へ置き直す。
        // 提案された矩形は使わない（右下にそろえる規則が先に立つ）。倍率が同じなら何もしない。
        WM_DPICHANGED => {
            let scale = ui::monitors::message_scale(wp);
            if scale != toast.scale {
                relayout(toast, scale);
            }
            0
        }
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let dc = BeginPaint(hwnd, &mut ps);
            paint(toast, dc);
            EndPaint(hwnd, &ps);
            0
        }
        WM_TIMER if wp == DISMISS => {
            begin_closing(toast);
            0
        }
        WM_TIMER if wp == ANIMATION => {
            let t = (toast.started.elapsed().as_secs_f32() * 1000.0 / SLIDE_MS).min(1.0);
            let (from, to, eased) = if toast.closing {
                (toast.left, toast.right, t * t * t)
            } else {
                (toast.right, toast.left, 1.0 - (1.0 - t).powi(3))
            };
            let x = from + ((to - from) as f32 * eased).round() as i32;
            SetWindowPos(
                hwnd,
                null_mut(),
                x,
                toast.top,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
            if t >= 1.0 {
                KillTimer(hwnd, ANIMATION);
                if toast.closing {
                    close();
                }
            }
            0
        }
        WM_MOUSEMOVE => {
            let p = point(lp);
            let mut client: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut client);
            let b = close_button(toast.scale, client.right);
            let close_hover = p.x >= b.left && p.x < b.right && p.y >= b.top && p.y < b.bottom;
            let action_top = if toast.body.is_empty() { 88.0 } else { 108.0 };
            let action_hover = p.x >= (PADDING * toast.scale) as i32
                && p.x < ((WIDTH - PADDING) * toast.scale) as i32
                && p.y >= (action_top * toast.scale) as i32
                && p.y < ((action_top + 32.0) * toast.scale) as i32;
            let menu_hover = p.x >= ((WIDTH - 84.0) * toast.scale) as i32
                && p.x < ((WIDTH - 44.0) * toast.scale) as i32
                && p.y >= (8.0 * toast.scale) as i32
                && p.y < (40.0 * toast.scale) as i32;
            if !toast.hover
                || close_hover != toast.close_hover
                || action_hover != toast.action_hover
                || menu_hover != toast.menu_hover
            {
                if !toast.hover {
                    let mut track = TRACKMOUSEEVENT {
                        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    TrackMouseEvent(&mut track);
                    KillTimer(hwnd, DISMISS);
                }
                toast.hover = true;
                toast.close_hover = close_hover;
                toast.action_hover = action_hover;
                toast.menu_hover = menu_hover;
                InvalidateRect(hwnd, null(), 0);
            }
            0
        }
        WM_MOUSELEAVE => {
            toast.hover = false;
            toast.close_hover = false;
            toast.action_hover = false;
            toast.menu_hover = false;
            if !toast.closing {
                SetTimer(hwnd, DISMISS, VISIBLE_MS / 2, None);
            }
            InvalidateRect(hwnd, null(), 0);
            0
        }
        WM_LBUTTONUP => {
            if toast.menu_hover {
                let menu = CreatePopupMenu();
                if !menu.is_null() {
                    AppendMenuW(menu, MF_STRING, 1, w(tr("Settings")).as_ptr());
                    AppendMenuW(menu, MF_STRING, 2, w(tr("Dismiss notification")).as_ptr());
                    let mut p = point(lp);
                    ClientToScreen(hwnd, &mut p);
                    SetForegroundWindow(hwnd);
                    let command = TrackPopupMenu(
                        menu,
                        TPM_RETURNCMD | TPM_NONOTIFY,
                        p.x,
                        p.y,
                        0,
                        hwnd,
                        null(),
                    );
                    DestroyMenu(menu);
                    if command == 1 {
                        crate::launch_settings();
                    }
                    // A popup runs a nested message loop: the timer may already have
                    // destroyed the toast, so reacquire it before closing.
                    if command != 0 {
                        if let Some(current) =
                            state().toast.as_deref_mut().filter(|t| t.hwnd == hwnd)
                        {
                            begin_closing(current);
                        }
                    }
                }
                return 0;
            } else if !toast.close_hover {
                if toast.edit || toast.path.is_none() {
                    // Screenshot actions open the built-in editor; recordings open the player.
                    if state().history {
                        crate::launch_history(true);
                    }
                } else if let Some(path) = toast
                    .path
                    .as_ref()
                    .filter(|p| p.extension().is_some_and(|e| e == "mp4"))
                {
                    crate::launch_player(path);
                } else if let Some(path) = &toast.path {
                    let file: Vec<u16> =
                        std::os::windows::ffi::OsStrExt::encode_wide(path.as_os_str())
                            .chain(Some(0))
                            .collect();
                    crate::imaging::shell_open(&file, None);
                }
            }
            begin_closing(toast);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) mod probe;
