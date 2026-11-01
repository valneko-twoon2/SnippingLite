use super::*;

const SCALES: [f32; 5] = [1.0, 1.25, 1.5, 1.75, 2.0];

fn at(x: i32, y: i32) -> i32 {
    button_at(POINT { x, y }, 1.0)
}

#[test]
fn every_button_is_found_at_its_center_on_every_scale() {
    for scale in SCALES {
        for index in 0..GLYPHS.len() as i32 {
            let [l, t, r, b] = toolbar_button(index, scale);
            let center = POINT {
                x: ((l + r) / 2.0) as i32,
                y: ((t + b) / 2.0) as i32,
            };
            assert_eq!(button_at(center, scale), index, "scale {scale}");
        }
    }
}

#[test]
fn button_edges_and_the_gaps_that_move_the_toolbar() {
    // The camera and video buttons touch: the shared edge belongs to video.
    assert_eq!(at(7, 24), -1);
    assert_eq!(at(8, 24), CAMERA_BUTTON);
    assert_eq!(at(43, 24), CAMERA_BUTTON);
    assert_eq!(at(44, 24), VIDEO_BUTTON);
    assert_eq!(at(79, 24), VIDEO_BUTTON);
    assert_eq!(at(80, 24), -1);
    assert_eq!(at(92, 24), AREA_BUTTON);
    assert_eq!(at(139, 24), AREA_BUTTON);
    assert_eq!(at(188, 24), TEXT_BUTTON);
    assert_eq!(at(223, 24), TEXT_BUTTON);
    assert_eq!(at(367, 24), CLOSE_BUTTON);
    assert_eq!(at(368, 24), -1);
    // Above and below the buttons.
    assert_eq!(at(26, 7), -1);
    assert_eq!(at(26, 8), CAMERA_BUTTON);
    assert_eq!(at(26, 39), CAMERA_BUTTON);
    assert_eq!(at(26, 40), -1);
    // Every gap between groups is free for dragging.
    for x in [86, 144, 186, 228, 236, 278, 320, 328] {
        assert_eq!(at(x, 24), -1, "x {x}");
    }
    // Outside the bar.
    assert_eq!(at(-1, 24), -1);
    assert_eq!(at(BAR_WIDTH as i32, 24), -1);
    assert_eq!(at(26, BAR_HEIGHT as i32), -1);
}

#[test]
fn buttons_never_overlap_and_stay_inside_the_bar() {
    for scale in SCALES {
        let (width, height) = ((BAR_WIDTH * scale) as i32, (BAR_HEIGHT * scale) as i32);
        for y in 0..height {
            for x in 0..width {
                let p = POINT { x, y };
                let hits = (0..GLYPHS.len() as i32)
                    .filter(|&i| ui::contains(toolbar_button(i, scale), p))
                    .count();
                assert!(hits <= 1, "{hits} buttons at {x},{y} on scale {scale}");
            }
        }
        for index in 0..GLYPHS.len() as i32 {
            let [l, t, r, b] = toolbar_button(index, scale);
            assert!(l >= 0.0 && t >= 0.0 && r <= width as f32 && b <= height as f32);
        }
    }
}

use crate::test_support::{answer_menu, desk::Desk, stub_window};
use std::time::Duration;

fn s() -> &'static mut State {
    // SAFETY: the test owns the state and runs on the thread that created it.
    unsafe { state() }
}

fn shown(window: HWND) -> bool {
    // SAFETY: plain window query.
    unsafe { IsWindowVisible(window) != 0 }
}

/// The middle of a button, in toolbar client coordinates.
fn center(index: i32) -> (i32, i32) {
    let [l, t, r, b] = toolbar_button(index, s().scale);
    (((l + r) / 2.0) as i32, ((t + b) / 2.0) as i32)
}

fn press(desk: &Desk, index: i32) {
    let (x, y) = center(index);
    desk.click(s().toolbar, x, y);
}

fn start(desk: &Desk) {
    // SAFETY: the windows belong to this test.
    unsafe { begin_capture() };
    assert!(s().active);
    desk.pump(Duration::from_millis(20));
}

#[test]
#[ignore = "Covers the screen"]
fn the_space_between_the_buttons_moves_the_toolbar() {
    let desk = Desk::start();
    s().scale = 1.0;
    // SAFETY: the windows belong to this test.
    unsafe { place_toolbar(POINT { x: 0, y: 0 }) };
    let mut window: RECT = unsafe { std::mem::zeroed() };
    unsafe { GetWindowRect(s().toolbar, &mut window) };
    let at = |x: i32, y: i32| {
        let screen = ((window.top + y) as isize) << 16 | ((window.left + x) as u16 as isize);
        desk.send(s().toolbar, WM_NCHITTEST, 0, screen)
    };
    // Equivalence: on a button the bar is a client; in a gap or outside a button it is a caption.
    assert_eq!(at(26, 24), HTCLIENT as LRESULT);
    assert_eq!(at(350, 24), HTCLIENT as LRESULT);
    assert_eq!(at(86, 24), HTCAPTION as LRESULT);
    assert_eq!(at(26, 2), HTCAPTION as LRESULT);
    assert_eq!(desk.send(s().toolbar, WM_NCLBUTTONDBLCLK, 0, 0), 0);
    assert_eq!(
        desk.send(s().toolbar, WM_MOUSEACTIVATE, 0, 0),
        MA_NOACTIVATE as LRESULT
    );
    assert_eq!(desk.send(s().toolbar, WM_ERASEBKGND, 0, 0), 1);
}

#[test]
#[ignore = "Covers the screen"]
fn a_new_toolbar_starts_at_the_top_center_and_a_kept_one_stays_where_it_was_moved() {
    let desk = Desk::start();
    for scale in SCALES {
        s().scale = scale;
        s().toolbar_open = false;
        // SAFETY: the windows belong to this test.
        let mut monitor: MONITORINFO = unsafe { std::mem::zeroed() };
        monitor.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        unsafe {
            GetMonitorInfoW(
                MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTONEAREST),
                &mut monitor,
            );
            place_toolbar(POINT { x: 0, y: 0 });
        }
        let mut rect: RECT = unsafe { std::mem::zeroed() };
        unsafe { GetWindowRect(s().toolbar, &mut rect) };
        let width = (BAR_WIDTH * scale).round() as i32;
        assert_eq!(rect.right - rect.left, width, "scale {scale}");
        assert_eq!(rect.bottom - rect.top, (BAR_HEIGHT * scale).round() as i32);
        assert_eq!(
            rect.left,
            (monitor.rcMonitor.left + monitor.rcMonitor.right - width) / 2
        );
        assert_eq!(
            rect.top,
            monitor.rcMonitor.top + (12.0 * scale).round() as i32
        );
    }
    // A kept toolbar that was dragged elsewhere keeps its corner.
    s().scale = 1.0;
    s().toolbar_open = true;
    unsafe {
        SetWindowPos(
            s().toolbar,
            null_mut(),
            123,
            234,
            10,
            10,
            SWP_NOACTIVATE | SWP_NOZORDER,
        );
        place_toolbar(POINT { x: 0, y: 0 });
    }
    let mut rect: RECT = unsafe { std::mem::zeroed() };
    unsafe { GetWindowRect(s().toolbar, &mut rect) };
    assert_eq!((rect.left, rect.top), (123, 234));
    // The backdrop is applied once per theme.
    assert_eq!(s().toolbar_dark, Some(s().theme.dark));
    drop(desk);
}

#[test]
#[ignore = "Covers the screen"]
fn a_kept_toolbar_comes_back_where_it_was_and_is_raised_once() {
    let desk = Desk::start_with(|settings| settings.keep_toolbar = true);
    s().toolbar_open = true;
    // SAFETY: the windows belong to this test.
    unsafe {
        SetWindowPos(
            s().toolbar,
            null_mut(),
            300,
            80,
            10,
            10,
            SWP_NOACTIVATE | SWP_NOZORDER,
        );
        show_kept_toolbar();
    }
    assert!(shown(s().toolbar));
    let mut rect: RECT = unsafe { std::mem::zeroed() };
    unsafe { GetWindowRect(s().toolbar, &mut rect) };
    assert_eq!((rect.left, rect.top), (300, 80));
    // The timer raises it once; a snip in progress or a hidden toolbar is left alone.
    desk.pump(Duration::from_millis(400));
    s().active = true;
    desk.send(s().toolbar, WM_TIMER, RAISE_TIMER, 0);
    s().active = false;
}

#[test]
#[ignore = "Covers the screen"]
fn hovering_and_pressing_follow_the_pointer() {
    let desk = Desk::start();
    start(&desk);
    assert_eq!(s().hover_button, -1);
    let (x, y) = center(HISTORY_BUTTON);
    desk.mouse(s().toolbar, WM_MOUSEMOVE, x, y);
    assert_eq!(s().hover_button, HISTORY_BUTTON);
    // Moving within the same button changes nothing.
    desk.mouse(s().toolbar, WM_MOUSEMOVE, x + 1, y);
    assert_eq!(s().hover_button, HISTORY_BUTTON);
    desk.mouse(s().toolbar, WM_LBUTTONDOWN, x, y);
    assert_eq!(s().pressed_button, HISTORY_BUTTON);
    // Releasing on another button is not a click: nothing happens and the press is forgotten.
    let (ox, oy) = center(SETTINGS_BUTTON);
    desk.mouse(s().toolbar, WM_LBUTTONUP, ox, oy);
    assert_eq!(s().pressed_button, -1);
    assert!(s().active);
    // Leaving the toolbar clears the highlight; so does pressing in a gap.
    desk.send(s().toolbar, WM_MOUSELEAVE, 0, 0);
    assert_eq!((s().hover_button, s().pressed_button), (-1, -1));
    desk.mouse(s().toolbar, WM_LBUTTONDOWN, 86, 24);
    assert_eq!(s().pressed_button, -1);
    desk.mouse(s().toolbar, WM_LBUTTONUP, 86, 24);
    assert!(s().active);
    desk.key(s().overlay, 0x1b);
}

#[test]
#[ignore = "Covers the screen"]
fn the_camera_and_video_buttons_choose_what_the_snip_makes() {
    let desk = Desk::start();
    start(&desk);
    assert!(!s().video);
    press(&desk, VIDEO_BUTTON);
    assert!(s().video && s().active);
    press(&desk, CAMERA_BUTTON);
    assert!(!s().video && s().active);
    // In window mode the highlighted bounds are worked out again for the new choice.
    s().mode = MODE_WINDOW;
    press(&desk, VIDEO_BUTTON);
    assert!(s().video && s().mode == MODE_WINDOW);
    press(&desk, CAMERA_BUTTON);
    desk.key(s().overlay, 0x1b);
    // On a kept toolbar between snips the same buttons start a new snip.
    s().toolbar_open = true;
    press(&desk, VIDEO_BUTTON);
    assert!(s().video && s().active);
    desk.key(s().overlay, 0x1b);
    s().toolbar_open = true;
    press(&desk, CAMERA_BUTTON);
    assert!(!s().video && s().active);
    desk.key(s().overlay, 0x1b);
}

#[test]
#[ignore = "Covers the screen"]
fn the_close_button_hides_the_toolbar_for_good() {
    let desk = Desk::start_with(|settings| settings.keep_toolbar = true);
    start(&desk);
    assert!(s().toolbar_open);
    press(&desk, CLOSE_BUTTON);
    assert!(!s().toolbar_open && !s().active);
    assert!(!shown(s().toolbar) && !shown(s().overlay));
}

#[test]
#[ignore = "Covers the screen"]
fn settings_and_history_open_their_windows_and_end_the_snip() {
    let desk = Desk::start();
    let settings = stub_window(crate::SETTINGS_WINDOW);
    let history = stub_window(crate::HISTORY_WINDOW);
    // ペンのボタンは画面を開かず、オン・オフを切り替えるだけ（markup のテストで確かめる）。
    for (button, window, other) in [
        (SETTINGS_BUTTON, settings, history),
        (HISTORY_BUTTON, history, settings),
    ] {
        // SAFETY: the windows belong to this test.
        unsafe {
            ShowWindow(window, SW_HIDE);
            ShowWindow(other, SW_HIDE);
        }
        start(&desk);
        press(&desk, button);
        assert!(!s().active, "button {button}");
        assert!(shown(window), "button {button}");
        assert!(!shown(other), "button {button}");
    }
    unsafe {
        DestroyWindow(settings);
        DestroyWindow(history);
    }
}

#[test]
#[ignore = "Shows a menu"]
fn the_area_menu_changes_the_mode_while_snipping() {
    let desk = Desk::start();
    start(&desk);
    for (index, mode) in [(1, MODE_WINDOW), (3, MODE_FREE), (0, MODE_RECT)] {
        let answer = answer_menu(index);
        press(&desk, AREA_BUTTON);
        assert!(answer.join().unwrap());
        assert_eq!(s().mode, mode, "item {index}");
        assert!(s().active);
    }
    // Leaving the menu with Escape changes nothing.
    let answer = answer_menu(usize::MAX);
    press(&desk, AREA_BUTTON);
    assert!(answer.join().unwrap());
    assert_eq!(s().mode, MODE_RECT);
    // Full screen takes the picture on the spot.
    let answer = answer_menu(2);
    press(&desk, AREA_BUTTON);
    assert!(answer.join().unwrap());
    assert!(!s().active);
    assert_eq!(history::list().len(), 1);
}

#[test]
#[ignore = "Shows a menu"]
fn the_area_menu_of_a_kept_toolbar_starts_a_snip_in_the_chosen_mode() {
    let desk = Desk::start_with(|settings| settings.keep_toolbar = true);
    s().toolbar_open = true;
    let answer = answer_menu(3);
    press(&desk, AREA_BUTTON);
    assert!(answer.join().unwrap());
    assert!(s().active && s().mode == MODE_FREE);
    desk.key(s().overlay, 0x1b);
    // Nothing chosen: no snip starts.
    s().toolbar_open = true;
    let answer = answer_menu(usize::MAX);
    press(&desk, AREA_BUTTON);
    assert!(answer.join().unwrap());
    assert!(!s().active);
}

#[test]
#[ignore = "Covers the screen"]
fn finishing_a_move_gives_the_keyboard_back_to_the_overlay() {
    let desk = Desk::start();
    // Idle: nothing to give back.
    desk.send(s().toolbar, WM_EXITSIZEMOVE, 0, 0);
    start(&desk);
    desk.send(s().toolbar, WM_EXITSIZEMOVE, 0, 0);
    assert!(s().active);
    desk.key(s().overlay, 0x1b);
}

#[test]
#[ignore = "Covers the screen"]
fn the_toolbar_paints_every_state_in_both_themes() {
    for dark in [false, true] {
        let desk = Desk::start();
        start(&desk);
        s().theme.dark = dark;
        for mode in [MODE_RECT, MODE_FREE, MODE_WINDOW, MODE_SCREEN] {
            s().mode = mode;
            for video in [false, true] {
                s().video = video;
                for (hover, pressed) in
                    [(-1, -1), (AREA_BUTTON, -1), (CAMERA_BUTTON, CAMERA_BUTTON)]
                {
                    s().hover_button = hover;
                    s().pressed_button = pressed;
                    desk.send(s().toolbar, WM_PAINT, 0, 0);
                }
            }
        }
        desk.key(s().overlay, 0x1b);
    }
}

#[test]
#[ignore = "Covers the screen"]
fn the_text_button_and_the_pen_button_exclude_each_other() {
    let desk = Desk::start();
    s().markup = false;
    s().extract = false;
    press(&desk, TEXT_BUTTON);
    assert!(s().extract && !s().markup);
    // The pen turns the text button off, and the text button turns the pen off.
    press(&desk, EDIT_BUTTON);
    assert!(s().markup && !s().extract);
    press(&desk, TEXT_BUTTON);
    assert!(s().extract && !s().markup);
    press(&desk, TEXT_BUTTON);
    assert!(!s().extract && !s().markup);
    // Neither one changes the snip that is running.
    start(&desk);
    press(&desk, TEXT_BUTTON);
    assert!(s().active && s().extract);
    desk.key(s().overlay, 0x1b);
    s().extract = false;
    config::save_markup(false);
}

#[test]
#[ignore = "Covers the screen"]
fn a_kept_toolbar_stays_on_the_screen_while_a_snip_starts_and_ends() {
    use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowDisplayAffinity;
    let desk = Desk::start_with(|settings| settings.keep_toolbar = true);
    let mut affinity = 0;
    // SAFETY: plain query of the toolbar window.
    unsafe { GetWindowDisplayAffinity(s().toolbar, &mut affinity) };
    assert_eq!(
        affinity, WDA_EXCLUDEFROMCAPTURE,
        "left out of every capture"
    );
    start(&desk);
    assert!(shown(s().toolbar));
    // Pressing the camera again and ending the snip never hide it: that made it flicker.
    press(&desk, CAMERA_BUTTON);
    assert!(s().active && shown(s().toolbar));
    desk.key(s().overlay, 0x1b);
    desk.pump(Duration::from_millis(50));
    assert!(!s().active && shown(s().toolbar));
}
