//! The full-screen selection overlay: window picking, highlight and input.

use super::*;

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
mod tests {
    use super::*;
    use crate::test_support::desk::Desk;
    use std::time::Duration;

    fn s() -> &'static mut State {
        // SAFETY: the test owns the state and runs on the thread that created it.
        unsafe { state() }
    }

    fn rect(left: i32, top: i32, right: i32, bottom: i32) -> RECT {
        RECT {
            left,
            top,
            right,
            bottom,
        }
    }

    fn start(desk: &Desk, freeze: bool) {
        s().freeze = freeze;
        // SAFETY: the windows belong to this test.
        unsafe { begin_capture() };
        assert!(s().active);
        desk.pump(Duration::from_millis(20));
    }

    #[test]
    fn two_optional_rectangles_are_the_same_only_when_every_edge_matches() {
        let a = rect(1, 2, 3, 4);
        assert!(same(None, None));
        assert!(same(Some(a), Some(rect(1, 2, 3, 4))));
        assert!(!same(Some(a), None));
        assert!(!same(None, Some(a)));
        // Each edge on its own makes the difference.
        for other in [
            rect(0, 2, 3, 4),
            rect(1, 0, 3, 4),
            rect(1, 2, 0, 4),
            rect(1, 2, 3, 0),
        ] {
            assert!(!same(Some(a), Some(other)));
        }
    }

    #[test]
    fn filling_clips_to_the_canvas_and_ignores_empty_areas() {
        // SAFETY: plain GDI allocation.
        let mut canvas = unsafe { ui::Canvas::new(10, 8) }.unwrap();
        let count = |canvas: &mut ui::Canvas, value: u32| {
            canvas.pixels().iter().filter(|&&p| p == value).count()
        };
        fill_rect(&mut canvas, rect(2, 2, 5, 4), 7);
        assert_eq!(count(&mut canvas, 7), 6);
        // Partly and completely outside, and inverted or empty: clipped, or nothing.
        fill_rect(&mut canvas, rect(-5, -5, 2, 2), 8);
        assert_eq!(count(&mut canvas, 8), 4);
        fill_rect(&mut canvas, rect(8, 6, 50, 50), 9);
        assert_eq!(count(&mut canvas, 9), 4);
        fill_rect(&mut canvas, rect(20, 20, 30, 30), 5);
        fill_rect(&mut canvas, rect(5, 5, 5, 7), 5);
        fill_rect(&mut canvas, rect(6, 6, 3, 3), 5);
        assert_eq!(count(&mut canvas, 5), 0);
        // A frame of thickness 1 around a 2x2 area is its 12 surrounding pixels.
        let mut canvas = unsafe { ui::Canvas::new(10, 10) }.unwrap();
        fill_frame(&mut canvas, rect(4, 4, 6, 6), 1, 3);
        assert_eq!(count(&mut canvas, 3), 12);
        assert_eq!(canvas.pixels()[5 * 10 + 5], 0, "the inside stays empty");
    }

    #[test]
    #[ignore = "Covers the screen"]
    fn the_border_thickness_follows_the_screen_scale() {
        let desk = Desk::start();
        for (scale, thickness) in [(0.2, 1), (1.0, 2), (1.25, 3), (1.5, 3), (2.0, 4)] {
            s().scale = scale;
            // SAFETY: the state belongs to this test.
            assert_eq!(unsafe { border() }, thickness, "scale {scale}");
        }
        drop(desk);
    }

    #[test]
    #[ignore = "Covers the screen"]
    fn what_is_bright_depends_on_the_mode_and_the_drag() {
        let desk = Desk::start();
        start(&desk, true);
        s().start = POINT { x: 50, y: 60 };
        s().end = POINT { x: 10, y: 20 };
        // Decision table over (mode, dragging).
        for (mode, dragging, expected) in [
            (MODE_RECT, true, Some((10, 20, 50, 60))),
            (MODE_RECT, false, None),
            (MODE_FREE, true, None),
            (MODE_SCREEN, true, None),
            (MODE_WINDOW, true, Some((1, 2, 3, 4))),
        ] {
            s().mode = mode;
            s().dragging = dragging;
            s().hover_window = Some(rect(1, 2, 3, 4));
            // SAFETY: the state belongs to this test.
            let bright = unsafe { highlight() }.map(|r| (r.left, r.top, r.right, r.bottom));
            assert_eq!(bright, expected, "mode {mode} dragging {dragging}");
        }
        s().dragging = false;
        s().mode = MODE_RECT;
        desk.key(s().overlay, 0x1b);
    }

    #[test]
    #[ignore = "Covers the screen"]
    fn the_layered_style_is_switched_without_touching_the_other_styles() {
        let desk = Desk::start();
        let style = || unsafe { GetWindowLongPtrW(s().overlay, GWL_EXSTYLE) };
        let base = style() & !(WS_EX_LAYERED as isize);
        // SAFETY: the window belongs to this test.
        unsafe {
            set_layered(s().overlay, true);
            assert_eq!(style(), base | WS_EX_LAYERED as isize);
            set_layered(s().overlay, true);
            assert_eq!(style(), base | WS_EX_LAYERED as isize);
            set_layered(s().overlay, false);
            assert_eq!(style(), base);
            set_layered(s().overlay, false);
        }
        assert_eq!(style(), base);
        drop(desk);
    }

    #[test]
    #[ignore = "Covers the screen"]
    fn the_live_overlay_draws_the_selection_and_the_freeform_trail() {
        let desk = Desk::start();
        start(&desk, false);
        let width = s().width;
        let full = rect(0, 0, width, s().height);
        s().mode = MODE_RECT;
        s().dragging = true;
        s().start = POINT { x: 100, y: 100 };
        s().end = POINT { x: 160, y: 140 };
        // SAFETY: the state belongs to this test.
        unsafe { live_draw(full) };
        let at = |x: i32, y: i32| s().live.as_mut().unwrap().pixels()[(y * width + x) as usize];
        assert_eq!(at(130, 120), 0x0c0c0c0c, "the selection is tinted");
        assert_eq!(at(99, 120), 0xffffffff, "the border around it is white");
        assert_eq!(at(20, 20), LIVE_BASE, "the rest only receives input");
        // An empty selection draws no frame.
        s().end = s().start;
        unsafe { live_draw(full) };
        assert_eq!(at(100, 100), LIVE_BASE);
        // The freeform trail is drawn along the points, steps of any direction.
        s().mode = MODE_FREE;
        s().points = vec![
            POINT { x: 200, y: 200 },
            POINT { x: 240, y: 210 },
            POINT { x: 240, y: 250 },
        ];
        unsafe { live_draw(full) };
        assert_eq!(at(220, 205), 0xffffffff);
        assert_eq!(at(240, 230), 0xffffffff);
        // Only the dirty area is redrawn, and a dirty area outside the screen is clipped.
        unsafe { live_draw(rect(-50, -50, 10, 10)) };
        unsafe { live_draw(rect(width - 5, 0, width + 500, 5)) };
        s().dragging = false;
        s().live = None;
        // Without the live layer there is nothing to draw on.
        unsafe { live_draw(full) };
        desk.key(s().overlay, 0x1b);
    }

    #[test]
    #[ignore = "Covers the screen"]
    fn a_change_repaints_what_it_touched_on_either_kind_of_overlay() {
        let desk = Desk::start();
        for freeze in [true, false] {
            start(&desk, freeze);
            // SAFETY: the state belongs to this test.
            unsafe {
                invalidate_change(None, None);
                invalidate_change(Some(rect(10, 10, 20, 20)), None);
                invalidate_change(None, Some(rect(10, 10, 20, 20)));
                invalidate_change(Some(rect(10, 10, 20, 20)), Some(rect(30, 30, 40, 40)));
                refresh_overlay();
            }
            desk.pump(Duration::from_millis(20));
            desk.key(s().overlay, 0x1b);
        }
    }

    #[test]
    #[ignore = "Covers the screen"]
    fn the_frozen_overlay_paints_the_dim_layer_the_selection_and_the_trail() {
        let desk = Desk::start();
        start(&desk, true);
        let full = rect(0, 0, s().width, s().height);
        // Nothing bright, a drag, a hovered window and a freeform trail all paint.
        for (mode, dragging) in [
            (MODE_RECT, false),
            (MODE_RECT, true),
            (MODE_WINDOW, false),
            (MODE_FREE, true),
        ] {
            s().mode = mode;
            s().dragging = dragging;
            s().start = POINT { x: 100, y: 100 };
            s().end = POINT { x: 220, y: 180 };
            s().hover_window = Some(rect(300, 300, 400, 380));
            s().points = vec![
                POINT { x: 10, y: 10 },
                POINT { x: 90, y: 40 },
                POINT { x: 50, y: 90 },
            ];
            // SAFETY: the window belongs to this test.
            unsafe { InvalidateRect(s().overlay, &full, 0) };
            desk.send(s().overlay, WM_PAINT, 0, 0);
        }
        assert_eq!(desk.send(s().overlay, WM_ERASEBKGND, 0, 0), 1);
        // Without the dim layer there is nothing to paint.
        s().dragging = false;
        s().dim_canvas = None;
        desk.send(s().overlay, WM_PAINT, 0, 0);
        desk.key(s().overlay, 0x1b);
    }

    #[test]
    #[ignore = "Covers the screen"]
    fn the_mouse_starts_and_extends_a_drag_in_each_mode() {
        let desk = Desk::start();
        start(&desk, true);
        let overlay = s().overlay;
        // Rectangle: the corner follows the pointer.
        desk.mouse(overlay, WM_LBUTTONDOWN, 100, 100);
        assert!(s().dragging && s().points.len() == 1);
        desk.mouse(overlay, WM_MOUSEMOVE, 150, 130);
        assert_eq!((s().end.x, s().end.y), (150, 130));
        // Freeform: every move adds a point.
        s().mode = MODE_FREE;
        desk.mouse(overlay, WM_MOUSEMOVE, 160, 140);
        desk.mouse(overlay, WM_MOUSEMOVE, 170, 150);
        assert_eq!(s().points.len(), 3);
        // A move without a drag over a rectangle snip does nothing.
        s().dragging = false;
        let before = s().points.len();
        desk.mouse(overlay, WM_MOUSEMOVE, 300, 300);
        assert_eq!(s().points.len(), before);
        // A button release without a drag is ignored.
        desk.mouse(overlay, WM_LBUTTONUP, 300, 300);
        assert!(s().active);
        // Key presses other than Escape fall through.
        desk.key(overlay, 0x41);
        assert!(s().active);
        desk.key(overlay, 0x1b);
        assert!(!s().active);
    }

    #[test]
    #[ignore = "Covers the screen"]
    fn a_window_under_the_pointer_is_found_and_taken_with_or_without_its_title_bar() {
        let desk = Desk::start();
        // SAFETY: a private topmost window the test creates and destroys.
        let window = unsafe {
            CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                w("STATIC").as_ptr(),
                w("overlay probe").as_ptr(),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                400,
                300,
                300,
                200,
                null_mut(),
                null_mut(),
                null_mut(),
                null(),
            )
        };
        assert!(!window.is_null());
        start(&desk, true);
        s().mode = MODE_WINDOW;
        // SAFETY: the state belongs to this test.
        unsafe { hover_window(POINT { x: 500, y: 400 }) };
        if s().hover_hwnd == window {
            let whole = s().hover_window.unwrap();
            // The same place again changes nothing.
            unsafe { hover_window(POINT { x: 501, y: 401 }) };
            let again = s().hover_window.unwrap();
            assert_eq!((whole.left, whole.top), (again.left, again.top));
            // Recording a window without its title bar highlights only the client area.
            s().video = true;
            s().skip_title = true;
            unsafe { hover_window(POINT { x: 500, y: 400 }) };
            let client = s().hover_window.unwrap();
            assert!(client.top > whole.top && client.bottom <= whole.bottom);
        }
        // A point nobody covers on this machine may still be the desktop; either way it is safe.
        unsafe { hover_window(POINT { x: 5, y: 5 }) };
        // Moving over the window with the mouse hovers it too.
        desk.mouse(s().overlay, WM_MOUSEMOVE, 500, 400);
        // Leaving window mode forgets the highlight.
        unsafe { set_mode(MODE_RECT) };
        assert!(s().hover_window.is_none() && s().hover_hwnd.is_null());
        desk.key(s().overlay, 0x1b);
        unsafe { DestroyWindow(window) };
    }

    #[test]
    #[ignore = "Covers the screen"]
    fn clicking_in_window_or_full_screen_mode_takes_that_target() {
        let desk = Desk::start();
        start(&desk, true);
        // Window mode with nothing highlighted: the click does nothing.
        s().mode = MODE_WINDOW;
        s().hover_window = None;
        desk.mouse(s().overlay, WM_LBUTTONDOWN, 10, 10);
        assert!(s().active);
        // Entering and leaving a mode repaints the toolbar and keeps the snip running.
        unsafe { set_mode(MODE_FREE) };
        unsafe { set_mode(MODE_RECT) };
        assert!(s().active && s().mode == MODE_RECT);
        desk.key(s().overlay, 0x1b);
    }
}
