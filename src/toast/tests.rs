use super::*;
use crate::{
    config::AppTheme,
    test_support::{answer_menu, desk::Desk, stub_window},
};
use std::time::Duration;

fn current() -> &'static mut Toast {
    // SAFETY: the test owns the state and runs on the thread that created it.
    unsafe {
        state()
            .toast
            .as_deref_mut()
            .expect("a notification is shown")
    }
}

fn client(hwnd: HWND) -> RECT {
    // SAFETY: plain window query.
    unsafe {
        let mut rect: RECT = std::mem::zeroed();
        GetClientRect(hwnd, &mut rect);
        rect
    }
}

fn left_edge(hwnd: HWND) -> i32 {
    // SAFETY: plain window query.
    unsafe {
        let mut rect: RECT = std::mem::zeroed();
        GetWindowRect(hwnd, &mut rect);
        rect.left
    }
}

#[test]
fn display_names_are_the_last_path_component() {
    assert_eq!(
        display_name(std::path::Path::new("D:\\Shots\\a b.png")),
        "a b.png"
    );
    assert_eq!(display_name(std::path::Path::new("a.png")), "a.png");
    assert_eq!(display_name(std::path::Path::new("")), "");
}

#[test]
fn the_close_button_stays_inside_the_toast_on_every_scale() {
    for step in 4..=16 {
        let scale = step as f32 / 4.0;
        let width = (WIDTH * scale).round() as i32;
        let button = close_button(scale, width);
        assert!(button.left > 0 && button.right < width && button.top >= 0);
        assert_eq!(button.bottom - button.top, (32.0 * scale).round() as i32);
    }
}

#[test]
#[ignore = "Shows notification windows"]
fn what_the_notification_says_follows_where_the_image_went() {
    let desk = Desk::start();
    let path = desk.sandbox.path().join("Snip 1.png");
    // Decision table over (copied, saved): the title, the body and the button.
    for (copied, saved, shown, title, body, can_open) in [
        (
            true,
            true,
            true,
            "Snip copied to clipboard",
            "Saved automatically to the Screenshots folder",
            true,
        ),
        (true, false, true, "Snip copied to clipboard", "", true),
        (false, true, true, "Snip saved", "Snip 1.png", true),
    ] {
        // SAFETY: the windows belong to this test.
        unsafe {
            show(&[0; 4], 1, 1, copied, saved.then(|| path.clone()));
            assert_eq!(visible(), shown);
        }
        let toast = current();
        assert_eq!(toast.title, tr(title), "{copied} {saved}");
        assert_eq!(toast.body, tr(body), "{copied} {saved}");
        assert_eq!(toast.can_open, can_open);
        assert!(toast.edit, "screenshots are edited in the history window");
    }
    // Nothing was copied or saved: there is nothing to announce.
    unsafe {
        close();
        show(&[0; 4], 1, 1, false, None);
        assert!(!visible());
    }
    drop(desk);
}

#[test]
#[ignore = "Shows notification windows"]
fn a_recording_notification_opens_the_file_and_a_new_one_replaces_the_old() {
    let desk = Desk::start();
    let path = desk.sandbox.path().join("Snipping Lite 1.mp4");
    // SAFETY: the windows belong to this test.
    unsafe {
        show_recording(&[0; 4], 1, 1, path.clone());
        let first = current().hwnd;
        assert_eq!(current().title, tr("Recording saved"));
        assert_eq!(current().body, "Snipping Lite 1.mp4");
        assert!(!current().edit && current().can_open);
        show_recording(&[0; 4], 1, 1, path);
        assert_ne!(current().hwnd, first);
        assert_eq!(IsWindow(first), 0, "the old notification is gone");
        close();
        assert!(!visible());
        // Closing twice is harmless.
        close();
    }
    drop(desk);
}

#[test]
#[ignore = "Shows notification windows"]
fn a_notification_without_history_cannot_edit_and_has_no_button_without_a_file() {
    let desk = Desk::start_with(|settings| settings.history = false);
    // SAFETY: the windows belong to this test.
    unsafe {
        show(&[0; 4], 1, 1, true, None);
        assert!(!current().edit && !current().can_open);
        let rect = client(current().hwnd);
        // Height with an empty body is the shorter variant.
        assert_eq!(rect.bottom, (136.0 * current().scale).round() as i32);
        show(&[0; 4], 1, 1, true, Some(desk.sandbox.path().join("a.png")));
        assert!(current().can_open && !current().edit);
        assert_eq!(
            client(current().hwnd).bottom,
            (156.0 * current().scale).round() as i32
        );
    }
    drop(desk);
}

#[test]
#[ignore = "Shows notification windows"]
fn the_notification_slides_in_waits_and_slides_out() {
    let desk = Desk::start();
    // SAFETY: the windows belong to this test.
    unsafe { show(&[0; 4], 1, 1, true, None) };
    let hwnd = current().hwnd;
    let (left, right) = (current().left, current().right);
    assert!(left < right);
    assert!(
        desk.pump_until(Duration::from_secs(3), || left_edge(hwnd) == left),
        "it slides to its place"
    );
    assert!(!current().closing);
    // The dismiss timer starts the slide out; the window is removed at the end of it.
    desk.send(hwnd, WM_TIMER, DISMISS, 0);
    assert!(current().closing);
    // A second request while closing changes nothing.
    desk.send(hwnd, WM_TIMER, DISMISS, 0);
    assert!(
        desk.pump_until(Duration::from_secs(3), || unsafe { !visible() }),
        "it is removed when it has slid out"
    );
    assert_eq!(unsafe { IsWindow(hwnd) }, 0);
    // Messages for a window that is not the current notification are left to Windows.
    assert_eq!(desk.send(hwnd, WM_TIMER, DISMISS, 0), 0);
}

#[test]
#[ignore = "Shows notification windows"]
fn pointing_at_the_notification_highlights_and_pauses_it() {
    let desk = Desk::start();
    // SAFETY: the windows belong to this test.
    unsafe { show(&[0; 4], 1, 1, true, Some(desk.sandbox.path().join("a.png"))) };
    let hwnd = current().hwnd;
    let s = current().scale;
    let close_at = (320.0 * s) as i32;
    let spot = |x: f32, y: f32| ((x * s) as i32, (y * s) as i32);
    // Equivalence classes of the pointer position, and the hover flags each one sets.
    let table = [
        (spot(334.0, 24.0), (true, false, false)), // the close button
        (spot(200.0, 120.0), (false, true, false)), // the action button
        (spot(300.0, 24.0), (false, false, true)), // the menu button
        (spot(20.0, 80.0), (false, false, false)), // the text
    ];
    let _ = close_at;
    for ((x, y), (close, action, menu)) in table {
        desk.mouse(hwnd, WM_MOUSEMOVE, x, y);
        let toast = current();
        assert!(toast.hover);
        assert_eq!(
            (toast.close_hover, toast.action_hover, toast.menu_hover),
            (close, action, menu),
            "{x},{y}"
        );
        // Painting works in every state.
        unsafe {
            InvalidateRect(hwnd, null(), 0);
            UpdateWindow(hwnd);
        }
    }
    // Moving inside the same spot changes nothing; leaving clears all of it.
    desk.mouse(hwnd, WM_MOUSEMOVE, 21, 81);
    assert!(current().hover);
    desk.send(hwnd, WM_MOUSELEAVE, 0, 0);
    let toast = current();
    assert!(!toast.hover && !toast.close_hover && !toast.action_hover && !toast.menu_hover);
    // Leaving a notification that is closing does not bring the dismiss timer back.
    toast.closing = true;
    desk.send(hwnd, WM_MOUSELEAVE, 0, 0);
    assert_eq!(desk.send(hwnd, WM_ERASEBKGND, 0, 0), 1);
    assert_eq!(
        desk.send(hwnd, WM_MOUSEACTIVATE, 0, 0),
        MA_NOACTIVATE as LRESULT
    );
}

#[test]
#[ignore = "Shows notification windows"]
fn painting_covers_every_theme_and_button_layout() {
    for theme in [AppTheme::Light, AppTheme::Dark] {
        let desk = Desk::start();
        crate::ui::set_theme_choice(theme);
        for (copied, path) in [(true, None), (true, Some("a.png")), (false, Some("b.png"))] {
            let path = path.map(|name| desk.sandbox.path().join(name));
            // SAFETY: the windows belong to this test.
            unsafe {
                show(&[0; 4], 1, 1, copied, path);
                let hwnd = current().hwnd;
                for (action, close_hover) in [(false, false), (true, false), (false, true)] {
                    let toast = current();
                    toast.action_hover = action;
                    toast.close_hover = close_hover;
                    InvalidateRect(hwnd, null(), 0);
                    UpdateWindow(hwnd);
                }
            }
        }
        crate::ui::set_theme_choice(AppTheme::System);
    }
}

#[test]
#[ignore = "Shows notification windows"]
fn the_menu_button_offers_settings_and_dismiss() {
    let desk = Desk::start();
    let settings = stub_window(crate::SETTINGS_WINDOW);
    // SAFETY: the windows belong to this test.
    unsafe {
        ShowWindow(settings, SW_HIDE);
        show(&[0; 4], 1, 1, true, None);
    }
    let hwnd = current().hwnd;
    let s = current().scale;
    let at = ((300.0 * s) as i32, (24.0 * s) as i32);
    desk.mouse(hwnd, WM_MOUSEMOVE, at.0, at.1);
    assert!(current().menu_hover);
    // Choosing Settings opens the settings window and closes the notification.
    let answer = answer_menu(0);
    desk.mouse(hwnd, WM_LBUTTONUP, at.0, at.1);
    assert!(answer.join().unwrap());
    assert!(
        unsafe { IsWindowVisible(settings) } != 0,
        "the settings window was brought forward"
    );
    assert!(current().closing);
    desk.pump(Duration::from_millis(50));
    unsafe { close() };
    // Choosing Dismiss only closes it.
    unsafe { show(&[0; 4], 1, 1, true, None) };
    let hwnd = current().hwnd;
    desk.mouse(hwnd, WM_MOUSEMOVE, at.0, at.1);
    unsafe { ShowWindow(settings, SW_HIDE) };
    let answer = answer_menu(1);
    desk.mouse(hwnd, WM_LBUTTONUP, at.0, at.1);
    assert!(answer.join().unwrap());
    assert!(unsafe { IsWindowVisible(settings) } == 0);
    assert!(current().closing);
    // Leaving the menu without a choice leaves the notification as it is.
    unsafe { close() };
    unsafe { show(&[0; 4], 1, 1, true, None) };
    let hwnd = current().hwnd;
    desk.mouse(hwnd, WM_MOUSEMOVE, at.0, at.1);
    let answer = answer_menu(usize::MAX);
    desk.mouse(hwnd, WM_LBUTTONUP, at.0, at.1);
    assert!(answer.join().unwrap());
    assert!(!current().closing);
    unsafe { DestroyWindow(settings) };
}

#[test]
#[ignore = "Shows notification windows"]
fn clicking_the_notification_opens_the_editor_or_the_file_and_the_x_just_closes() {
    let desk = Desk::start();
    let history = stub_window(crate::HISTORY_WINDOW);
    // SAFETY: the windows belong to this test.
    unsafe {
        ShowWindow(history, SW_HIDE);
        show(&[0; 4], 1, 1, true, Some(desk.sandbox.path().join("a.png")));
    }
    let hwnd = current().hwnd;
    let s = current().scale;
    let in_text = ((20.0 * s) as i32, (80.0 * s) as i32);
    // A click on the body of a screenshot notification opens the history editor.
    desk.mouse(hwnd, WM_MOUSEMOVE, in_text.0, in_text.1);
    desk.mouse(hwnd, WM_LBUTTONUP, in_text.0, in_text.1);
    assert!(unsafe { IsWindowVisible(history) } != 0);
    assert!(current().closing);
    unsafe { close() };
    // The close button dismisses without opening anything.
    unsafe {
        ShowWindow(history, SW_HIDE);
        show(&[0; 4], 1, 1, true, None);
    }
    let hwnd = current().hwnd;
    let x = ((334.0 * s) as i32, (24.0 * s) as i32);
    desk.mouse(hwnd, WM_MOUSEMOVE, x.0, x.1);
    desk.mouse(hwnd, WM_LBUTTONUP, x.0, x.1);
    assert!(unsafe { IsWindowVisible(history) } == 0);
    assert!(current().closing);
    unsafe { close() };
    // Without history there is no editor to open: the click only closes it.
    unsafe { state().history = false };
    unsafe { show(&[0; 4], 1, 1, true, None) };
    let hwnd = current().hwnd;
    desk.mouse(hwnd, WM_MOUSEMOVE, in_text.0, in_text.1);
    desk.mouse(hwnd, WM_LBUTTONUP, in_text.0, in_text.1);
    assert!(unsafe { IsWindowVisible(history) } == 0);
    assert!(current().closing);
    unsafe { close() };
    // A recording notification opens the file itself.
    let video = desk.sandbox.path().join("missing.mp4");
    unsafe { show_recording(&[0; 4], 1, 1, video.clone()) };
    let hwnd = current().hwnd;
    desk.mouse(hwnd, WM_MOUSEMOVE, in_text.0, in_text.1);
    crate::test_support::take_shell_opened();
    desk.mouse(hwnd, WM_LBUTTONUP, in_text.0, in_text.1);
    assert_eq!(
        crate::test_support::take_shell_opened(),
        vec![(video.to_string_lossy().into_owned(), None)]
    );
    assert!(current().closing);
    unsafe {
        close();
        DestroyWindow(history);
    }
}
