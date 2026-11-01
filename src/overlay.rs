//! The full-screen selection overlay: window picking, highlight and input.

use super::*;

mod display;
pub(crate) use display::remember_layout;
use display::{display_changed, settle, SETTLE};

unsafe fn select_window(p: POINT) -> Option<(HWND, RECT)> {
    let absolute = POINT {
        x: p.x + state().origin.x,
        y: p.y + state().origin.y,
    };
    let mut window = GetWindow(state().overlay, GW_HWNDNEXT);
    let inside = |r: &RECT| {
        absolute.x >= r.left && absolute.x < r.right && absolute.y >= r.top && absolute.y < r.bottom
    };
    while !window.is_null() {
        let mut rect: RECT = std::mem::zeroed();
        let mut cloaked = 0u32;
        // Cheap local checks first: DWM queries are cross-process calls and
        // most windows in the z-order are hidden.
        if window != state().owner
            && window != state().toolbar
            && IsWindowVisible(window) != 0
            && IsIconic(window) == 0
            && GetWindowRect(window, &mut rect) != 0
            && inside(&rect)
            && (DwmGetWindowAttribute(
                window,
                DWMWA_CLOAKED as u32,
                (&mut cloaked as *mut u32).cast(),
                4,
            ) != 0
                || cloaked == 0)
            && DwmGetWindowAttribute(
                window,
                DWMWA_EXTENDED_FRAME_BOUNDS as u32,
                (&mut rect as *mut RECT).cast(),
                std::mem::size_of::<RECT>() as u32,
            ) == 0
            && inside(&rect)
        {
            // Highlight the captured area; recording can leave out the title bar.
            rect = window_capture::Target::new(window, state().video && state().skip_title)
                .bounds()
                .ok()?;
            return Some((
                window,
                RECT {
                    left: (rect.left - state().origin.x).max(0),
                    top: (rect.top - state().origin.y).max(0),
                    right: (rect.right - state().origin.x).min(state().width),
                    bottom: (rect.bottom - state().origin.y).min(state().height),
                },
            ));
        }
        window = GetWindow(window, GW_HWNDNEXT);
    }
    None
}
fn same(a: Option<RECT>, b: Option<RECT>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => {
            (a.left, a.top, a.right, a.bottom) == (b.left, b.top, b.right, b.bottom)
        }
        (None, None) => true,
        _ => false,
    }
}
/// A pixel that receives mouse input while staying invisible on the live screen.
const LIVE_BASE: u32 = 0x04000000;
pub(crate) unsafe fn set_layered(hwnd: HWND, layered: bool) {
    let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
    let wanted = if layered {
        style | WS_EX_LAYERED as isize
    } else {
        style & !(WS_EX_LAYERED as isize)
    };
    if wanted != style {
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, wanted);
    }
}
fn fill_rect(canvas: &mut ui::Canvas, r: RECT, value: u32) {
    let (width, height) = (canvas.width, canvas.height);
    let (left, right) = (r.left.clamp(0, width), r.right.clamp(0, width));
    let (top, bottom) = (r.top.clamp(0, height), r.bottom.clamp(0, height));
    if right <= left {
        return;
    }
    let pixels = canvas.pixels();
    for y in top..bottom {
        pixels[(y * width + left) as usize..(y * width + right) as usize].fill(value);
    }
}
/// Fills the four edges of a frame of thickness `t` around `r`.
fn fill_frame(canvas: &mut ui::Canvas, r: RECT, t: i32, value: u32) {
    for edge in [
        RECT {
            left: r.left - t,
            top: r.top - t,
            right: r.right + t,
            bottom: r.top,
        },
        RECT {
            left: r.left - t,
            top: r.bottom,
            right: r.right + t,
            bottom: r.bottom + t,
        },
        RECT {
            left: r.left - t,
            top: r.top,
            right: r.left,
            bottom: r.bottom,
        },
        RECT {
            left: r.right,
            top: r.top,
            right: r.right + t,
            bottom: r.bottom,
        },
    ] {
        fill_rect(canvas, edge, value);
    }
}
/// Redraws the live overlay inside `dirty`: an invisible input layer with the
/// selection (outline and a light tint) or the freeform trail on top.
pub(crate) unsafe fn live_draw(dirty: RECT) {
    let t = border();
    let bright = highlight().filter(|r| r.right > r.left && r.bottom > r.top);
    let s = state();
    let Some(canvas) = s.live.as_mut() else {
        return;
    };
    let dirty = RECT {
        left: dirty.left.clamp(0, canvas.width),
        top: dirty.top.clamp(0, canvas.height),
        right: dirty.right.clamp(0, canvas.width),
        bottom: dirty.bottom.clamp(0, canvas.height),
    };
    fill_rect(canvas, dirty, LIVE_BASE);
    if let Some(b) = bright {
        fill_rect(canvas, b, 0x0c0c0c0c);
        fill_frame(canvas, inflate(b, 2), t, 0x80000000);
        fill_frame(canvas, b, t, 0xffffffff);
    }
    if s.mode == MODE_FREE && s.dragging && s.points.len() > 1 {
        for pair in s.points.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let steps = (b.x - a.x).abs().max((b.y - a.y).abs()).max(1);
            for i in 0..=steps {
                let x = a.x + (b.x - a.x) * i / steps;
                let y = a.y + (b.y - a.y) * i / steps;
                fill_rect(
                    canvas,
                    RECT {
                        left: x - t / 2,
                        top: y - t / 2,
                        right: x - t / 2 + t,
                        bottom: y - t / 2 + t,
                    },
                    0xffffffff,
                );
            }
        }
    }
    let mut origin = s.origin;
    let mut size = SIZE {
        cx: canvas.width,
        cy: canvas.height,
    };
    let mut source = POINT { x: 0, y: 0 };
    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: 255,
        AlphaFormat: AC_SRC_ALPHA as u8,
    };
    let info = UPDATELAYEREDWINDOWINFO {
        cbSize: std::mem::size_of::<UPDATELAYEREDWINDOWINFO>() as u32,
        hdcDst: null_mut(),
        pptDst: &mut origin,
        psize: &mut size,
        hdcSrc: canvas.dc(),
        pptSrc: &mut source,
        crKey: 0,
        pblend: &blend,
        dwFlags: ULW_ALPHA,
        prcDirty: &dirty,
    };
    UpdateLayeredWindowIndirect(s.overlay, &info);
}
unsafe fn border() -> i32 {
    (state().scale * 2.0).round().max(1.0) as i32
}
unsafe fn invalidate(r: RECT) {
    let r = inflate(r, border() + 3);
    if state().live.is_some() {
        live_draw(r);
    } else {
        InvalidateRect(state().overlay, &r, 0);
    }
}
/// Repaints what a change from `old` to `new` touched; the state must already be updated.
unsafe fn invalidate_change(old: Option<RECT>, new: Option<RECT>) {
    match (old, new) {
        (Some(a), Some(b)) => invalidate(RECT {
            left: a.left.min(b.left),
            top: a.top.min(b.top),
            right: a.right.max(b.right),
            bottom: a.bottom.max(b.bottom),
        }),
        (Some(r), None) | (None, Some(r)) => invalidate(r),
        (None, None) => {}
    }
}
pub(crate) unsafe fn refresh_overlay() {
    if state().live.is_some() {
        live_draw(RECT {
            left: 0,
            top: 0,
            right: state().width,
            bottom: state().height,
        });
    } else {
        InvalidateRect(state().overlay, null(), 0);
    }
}
/// Window selection is explicit for both screenshots and recordings.
pub(crate) unsafe fn hovers_windows() -> bool {
    state().mode == MODE_WINDOW
}
pub(crate) unsafe fn hover_window(p: POINT) {
    let selected = select_window(p);
    state().hover_hwnd = selected.map_or(null_mut(), |(window, _)| window);
    let hover = selected.map(|(_, rect)| rect);
    if !same(hover, state().hover_window) {
        let old = std::mem::replace(&mut state().hover_window, hover);
        invalidate_change(old, hover);
    }
}
pub(crate) unsafe fn set_mode(mode: i32) {
    if let Some(old) = state().hover_window.take() {
        invalidate(old);
    }
    state().hover_hwnd = null_mut();
    state().mode = mode;
    InvalidateRect(state().toolbar, null(), 0);
    if mode == MODE_SCREEN {
        complete_selection(
            RECT {
                left: 0,
                top: 0,
                right: state().width,
                bottom: state().height,
            },
            false,
        );
    } else if hovers_windows() {
        let mut cursor = POINT { x: 0, y: 0 };
        GetCursorPos(&mut cursor);
        hover_window(POINT {
            x: cursor.x - state().origin.x,
            y: cursor.y - state().origin.y,
        });
    }
}
/// The undimmed part of the overlay, if any.
unsafe fn highlight() -> Option<RECT> {
    let s = state();
    match s.mode {
        MODE_RECT if s.dragging => Some(bounds(s.start, s.end, s.width, s.height)),
        MODE_WINDOW => s.hover_window,
        _ => None,
    }
}
unsafe fn paint_overlay(dc: HDC, area: RECT) {
    let s = state();
    let Some(dim) = s.dim_canvas.as_ref() else {
        return;
    };
    let bright = highlight().filter(|r| r.right > r.left && r.bottom > r.top);
    let saved = SaveDC(dc);
    if let Some(b) = bright {
        ExcludeClipRect(dc, b.left, b.top, b.right, b.bottom);
    }
    BitBlt(
        dc,
        area.left,
        area.top,
        area.right - area.left,
        area.bottom - area.top,
        dim.dc(),
        area.left,
        area.top,
        SRCCOPY,
    );
    RestoreDC(dc, saved);
    let t = border();
    if let Some(b) = bright {
        BitBlt(
            dc,
            b.left,
            b.top,
            b.right - b.left,
            b.bottom - b.top,
            s.screen_dc,
            b.left,
            b.top,
            SRCCOPY,
        );
        let white = GetStockObject(WHITE_BRUSH) as HBRUSH;
        for edge in [
            RECT {
                left: b.left - t,
                top: b.top - t,
                right: b.right + t,
                bottom: b.top,
            },
            RECT {
                left: b.left - t,
                top: b.bottom,
                right: b.right + t,
                bottom: b.bottom + t,
            },
            RECT {
                left: b.left - t,
                top: b.top,
                right: b.left,
                bottom: b.bottom,
            },
            RECT {
                left: b.right,
                top: b.top,
                right: b.right + t,
                bottom: b.bottom,
            },
        ] {
            FillRect(dc, &edge, white);
        }
    }
    if s.mode == MODE_FREE && s.dragging && s.points.len() > 1 {
        let pen = CreatePen(PS_SOLID, t, 0x00ffffff);
        let old = SelectObject(dc, pen);
        Polyline(dc, s.points.as_ptr(), s.points.len() as i32);
        SelectObject(dc, old);
        DeleteObject(pen);
    }
}
pub(crate) unsafe extern "system" fn overlay_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
) -> LRESULT {
    // 切り取った範囲に書き込んでいる間は、その操作が入力と描画を受け持つ。
    if let Some(result) = markup::overlay_message(hwnd, msg, wp, lp) {
        return result;
    }
    match msg {
        WM_LBUTTONDOWN => {
            let p = point(lp);
            if state().mode == MODE_SCREEN {
                complete_selection(
                    RECT {
                        left: 0,
                        top: 0,
                        right: state().width,
                        bottom: state().height,
                    },
                    false,
                );
                return 0;
            }
            if state().mode == MODE_WINDOW {
                if let Some(rect) = state().hover_window {
                    complete_selection(rect, false);
                }
                return 0;
            }
            if let Some(old) = state().hover_window.take() {
                invalidate(old);
            }
            state().start = p;
            state().end = p;
            state().points.clear();
            state().points.push(p);
            state().dragging = true;
            SetCapture(hwnd);
            0
        }
        WM_MOUSEMOVE if state().dragging => {
            let p = point(lp);
            if state().mode == MODE_FREE {
                let last = state().points.last().copied();
                state().points.push(p);
                if let Some(last) = last {
                    invalidate(bounds(last, p, state().width, state().height));
                }
            } else {
                let old = highlight();
                state().end = p;
                invalidate_change(old, highlight());
            }
            0
        }
        WM_MOUSEMOVE if hovers_windows() => {
            hover_window(point(lp));
            0
        }
        WM_LBUTTONUP if state().dragging => {
            state().end = point(lp);
            state().dragging = false;
            ReleaseCapture();
            let free = state().mode == MODE_FREE;
            let rect = if free && state().points.len() >= 3 {
                let mut r = RECT {
                    left: state().width,
                    top: state().height,
                    right: 0,
                    bottom: 0,
                };
                for p in &state().points {
                    r.left = r.left.min(p.x);
                    r.top = r.top.min(p.y);
                    r.right = r.right.max(p.x);
                    r.bottom = r.bottom.max(p.y);
                }
                bounds(
                    POINT {
                        x: r.left,
                        y: r.top,
                    },
                    POINT {
                        x: r.right,
                        y: r.bottom,
                    },
                    state().width,
                    state().height,
                )
            } else {
                bounds(state().start, state().end, state().width, state().height)
            };
            if !free || state().points.len() >= 3 {
                complete_selection(rect, free);
            } else {
                refresh_overlay();
            }
            0
        }
        WM_KEYDOWN if wp as u32 == VK_ESCAPE as u32 => {
            cancel();
            0
        }
        WM_ERASEBKGND => 1,
        // ディスプレイの構成が変わった（overlay/display.rs）。
        WM_DISPLAYCHANGE => {
            display_changed(hwnd);
            0
        }
        SETTLE => {
            settle();
            0
        }
        // オーバーレイは仮想画面を物理画素で覆うので、拡大率が変わっても提案された矩形へは
        // 動かさない（凍結した画像も選んでいる範囲も画素の位置のまま正しい）。
        WM_DPICHANGED => 0,
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let dc = BeginPaint(hwnd, &mut ps);
            paint_overlay(dc, ps.rcPaint);
            EndPaint(hwnd, &ps);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

#[cfg(test)]
mod tests;
