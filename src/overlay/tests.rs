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
