use super::*;
use crate::test_support::{
    answer_dialog, clipboard_image, desk::Desk, hold_clipboard, stub_window,
};
use crate::window_capture::test_support::CoveredWindow;
use std::time::Duration;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;

fn s() -> &'static mut State {
    // SAFETY: the test owns the state and runs on the thread that created it.
    unsafe { state() }
}

fn begin(desk: &Desk) {
    // SAFETY: the windows belong to this test.
    unsafe { begin_capture() };
    assert!(s().active, "the snip started");
    desk.pump(Duration::from_millis(30));
}

fn shots(desk: &Desk) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(desk.sandbox.path().join("shots"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|f| f.path())
        .collect();
    files.sort();
    files
}

fn visible(window: HWND) -> bool {
    // SAFETY: plain window query.
    unsafe { IsWindowVisible(window) != 0 }
}

fn pixel(pixels: &[u8], width: i32, x: i32, y: i32) -> [u8; 4] {
    let at = ((y * width + x) * 4) as usize;
    [pixels[at], pixels[at + 1], pixels[at + 2], pixels[at + 3]]
}

#[test]
#[ignore = "Covers the screen"]
fn a_selection_goes_to_the_clipboard_the_folder_the_history_and_a_notification() {
    // Both the frozen and the live overlay take the same picture.
    for freeze in [true, false] {
        let desk = Desk::start_with(|settings| {
            settings.freeze = freeze;
            settings.copy = true;
            settings.save = true;
            settings.history = true;
            settings.notify = true;
        });
        begin(&desk);
        assert_eq!(s().live.is_some(), !freeze);
        assert!(visible(s().overlay) && visible(s().toolbar));
        desk.drag(
            s().overlay,
            (200, 200),
            &[(250, 230), (300, 260)],
            (360, 280),
        );
        assert!(!s().active, "freeze={freeze}");
        assert!(!visible(s().overlay) && !visible(s().toolbar));
        let files = shots(&desk);
        assert_eq!(files.len(), 1, "freeze={freeze}");
        let (width, height, pixels) = imaging::decode(&files[0]).unwrap();
        assert_eq!((width, height), (160, 80));
        assert!(pixels.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
        let (clip_width, clip_height, clip) =
            clipboard_image().expect("the image is on the clipboard");
        assert_eq!((clip_width, clip_height), (160, 80));
        assert_eq!(clip.len(), pixels.len());
        let entries = history::list();
        assert_eq!(entries.len(), 1);
        assert_eq!(imaging::decode(&entries[0].image).unwrap().0, 160);
        assert!(unsafe { toast::visible() });
    }
}

#[test]
#[ignore = "Covers the screen"]
fn only_the_chosen_destinations_receive_the_picture() {
    // Decision table over (copy, save, history, notify).
    for (copy, save, history_on, notify, files, entries, toast_shown) in [
        (true, false, false, false, 0, 0, false),
        (false, true, false, false, 1, 0, false),
        (true, true, true, true, 1, 1, true),
        (false, true, true, true, 1, 1, true),
    ] {
        let desk = Desk::start_with(|settings| {
            settings.copy = copy;
            settings.save = save;
            settings.history = history_on;
            settings.notify = notify;
        });
        begin(&desk);
        desk.drag(s().overlay, (100, 100), &[], (180, 160));
        assert_eq!(shots(&desk).len(), files, "{copy} {save}");
        assert_eq!(history::list().len(), entries, "{copy} {save}");
        assert_eq!(unsafe { toast::visible() }, toast_shown, "{copy} {save}");
    }
}

#[test]
#[ignore = "Covers the screen"]
fn turning_the_history_on_while_running_records_the_next_snip() {
    let desk = Desk::start_with(|settings| settings.history = false);
    let mut settings = config::load_settings();
    settings.history = true;
    config::save_settings(&settings).unwrap();
    desk.send(desk.main, TRAY_RELOAD, 0, 0);
    begin(&desk);
    desk.drag(s().overlay, (100, 100), &[], (180, 160));
    assert_eq!(history::list().len(), 1);
}

#[test]
#[ignore = "Covers the screen"]
fn a_selection_without_area_keeps_the_overlay_open_until_escape() {
    let desk = Desk::start();
    begin(&desk);
    desk.click(s().overlay, 300, 300);
    assert!(s().active, "nothing was selected");
    assert!(shots(&desk).is_empty());
    desk.key(s().overlay, VK_ESCAPE);
    assert!(!s().active);
    assert!(!visible(s().overlay));
    // The shortcut again starts a fresh snip, and does nothing while one is running.
    begin(&desk);
    unsafe { begin_capture() };
    assert!(s().active);
}

#[test]
#[ignore = "Covers the screen"]
fn a_freeform_shape_keeps_its_outline_and_clears_the_rest() {
    let desk = Desk::start_with(|settings| settings.outline = true);
    begin(&desk);
    // SAFETY: the windows belong to this test.
    unsafe { set_mode(MODE_FREE) };
    desk.drag(
        s().overlay,
        (200, 200),
        &[(250, 200), (300, 200), (280, 240)],
        (250, 280),
    );
    assert!(!s().active);
    let entry = history::list().remove(0);
    let (width, height, pixels) = imaging::decode(&entry.image).unwrap();
    assert_eq!((width, height), (100, 80));
    // Inside the shape the picture is opaque; the corner outside it is transparent, and the
    // border is not drawn around a shape that has no straight edges.
    assert_eq!(pixel(&pixels, width, 50, 10)[3], 255);
    assert_eq!(pixel(&pixels, width, 0, 79), [255, 255, 255, 0]);
    assert_eq!(pixel(&pixels, width, 99, 79)[3], 0);
}

#[test]
#[ignore = "Covers the screen"]
fn a_freeform_stroke_of_fewer_than_three_points_selects_nothing() {
    let desk = Desk::start();
    begin(&desk);
    unsafe { set_mode(MODE_FREE) };
    desk.drag(s().overlay, (200, 200), &[], (220, 200));
    assert!(s().active);
    assert!(history::list().is_empty());
    desk.key(s().overlay, VK_ESCAPE);
}

#[test]
#[ignore = "Covers the screen"]
fn the_border_is_drawn_inside_rectangles() {
    for (index, width_index, border) in [(0, 0, 1), (1, 1, 2), (4, 2, 4)] {
        let desk = Desk::start_with(|settings| {
            settings.outline = true;
            settings.outline_color = index;
            settings.outline_width = width_index;
        });
        begin(&desk);
        desk.drag(s().overlay, (200, 200), &[], (260, 240));
        let entry = history::list().remove(0);
        let (width, height, pixels) = imaging::decode(&entry.image).unwrap();
        assert_eq!((width, height), (60, 40));
        let color = annotate::COLORS[index];
        let expected = [color[2], color[1], color[0], 255];
        // Boundary: the last pixel of the border is colored, the first one inside is not.
        assert_eq!(pixel(&pixels, width, 0, 0), expected);
        assert_eq!(pixel(&pixels, width, border - 1, 20), expected);
        assert_eq!(pixel(&pixels, width, width - 1, height - 1), expected);
        assert_ne!(pixel(&pixels, width, border, 20), expected);
        drop(desk);
    }
}

#[test]
#[ignore = "Covers the screen"]
fn the_whole_screen_can_be_taken_in_one_step() {
    let desk = Desk::start();
    begin(&desk);
    let (width, height) = (s().width, s().height);
    // SAFETY: the windows belong to this test.
    unsafe { set_mode(MODE_SCREEN) };
    assert!(!s().active);
    let entry = history::list().remove(0);
    let (w, h, _) = imaging::decode(&entry.image).unwrap();
    assert_eq!((w, h), (width, height));
    // In full screen mode a click anywhere on the overlay takes the screen too.
    begin(&desk);
    s().mode = MODE_SCREEN;
    desk.click(s().overlay, 10, 10);
    assert!(!s().active);
    assert_eq!(history::list().len(), 2);
}

#[test]
#[ignore = "Covers the screen"]
fn a_picked_window_is_captured_by_itself() {
    let desk = Desk::start();
    let fixture = CoveredWindow::new();
    begin(&desk);
    unsafe { set_mode(MODE_WINDOW) };
    // The fixture sits on the primary monitor, which is not at the overlay's origin when another monitor is to its left or above.
    let (x, y) = (200 - s().origin.x, 200 - s().origin.y);
    // The cover is the top window at this point; hovering finds a window and highlights it.
    desk.mouse(s().overlay, WM_MOUSEMOVE, x, y);
    assert!(!s().hover_hwnd.is_null());
    let rect = s().hover_window.expect("a window is highlighted");
    desk.mouse(s().overlay, WM_LBUTTONDOWN, x, y);
    assert!(!s().active);
    let entry = history::list().remove(0);
    let (width, height, _) = imaging::decode(&entry.image).unwrap();
    assert_eq!(
        (width, height),
        (rect.right - rect.left, rect.bottom - rect.top)
    );
    drop(fixture);
}

#[test]
#[ignore = "Covers the screen"]
fn a_window_that_disappears_before_the_click_is_reported() {
    let desk = Desk::start();
    let fixture = CoveredWindow::new();
    begin(&desk);
    unsafe { set_mode(MODE_WINDOW) };
    // Which window sits under the pointer depends on the desktop, so pick the fixture itself.
    s().hover_hwnd = fixture.target;
    s().hover_window = Some(RECT {
        left: 200,
        top: 200,
        right: 300,
        bottom: 300,
    });
    drop(fixture);
    let answer = answer_dialog(2);
    desk.mouse(s().overlay, WM_LBUTTONDOWN, 200, 200);
    assert!(answer.join().unwrap(), "the error is shown");
    assert!(!s().active);
    assert!(history::list().is_empty());
}

#[test]
#[ignore = "Covers the screen"]
fn the_delay_waits_before_taking_the_picture() {
    let desk = Desk::start_with(|settings| settings.delay = 3);
    s().delay = 1;
    // SAFETY: the windows belong to this test.
    unsafe { begin_capture() };
    assert!(s().pending && !s().active);
    // Starting again while waiting does nothing.
    unsafe { begin_capture() };
    assert!(s().pending);
    assert!(desk.pump_until(Duration::from_secs(5), || s().active));
    assert!(!s().pending);
    desk.key(s().overlay, VK_ESCAPE);
}

#[test]
#[ignore = "Covers the screen"]
fn our_own_windows_leave_the_screen_for_the_picture_and_come_back() {
    let desk = Desk::start();
    let settings = stub_window(crate::SETTINGS_WINDOW);
    let history_window = stub_window(crate::HISTORY_WINDOW);
    unsafe {
        ShowWindow(settings, SW_SHOWNOACTIVATE);
        // The history window stays closed and must stay closed.
        ShowWindow(history_window, SW_HIDE);
    }
    begin(&desk);
    assert!(!visible(settings));
    assert_eq!(s().hidden, vec![crate::SETTINGS_WINDOW]);
    desk.key(s().overlay, VK_ESCAPE);
    assert!(visible(settings));
    assert!(!visible(history_window));
    assert!(s().hidden.is_empty());
    unsafe {
        DestroyWindow(settings);
        DestroyWindow(history_window);
    }
}

#[test]
#[ignore = "Covers the screen"]
fn a_kept_toolbar_hides_for_the_picture_and_returns_after_it() {
    let desk = Desk::start_with(|settings| settings.keep_toolbar = true);
    begin(&desk);
    assert!(s().toolbar_open, "the toolbar is kept");
    desk.drag(s().overlay, (200, 200), &[], (260, 240));
    assert!(visible(s().toolbar), "it returns after the snip");
    let first = {
        let mut rect: RECT = unsafe { std::mem::zeroed() };
        unsafe { GetWindowRect(s().toolbar, &mut rect) };
        (rect.left, rect.top)
    };
    // The next snip starts from the kept toolbar and hides it for the picture.
    unsafe { begin_capture() };
    assert!(s().active && s().toolbar_open);
    desk.drag(s().overlay, (200, 200), &[], (260, 240));
    let mut rect: RECT = unsafe { std::mem::zeroed() };
    unsafe { GetWindowRect(s().toolbar, &mut rect) };
    assert_eq!((rect.left, rect.top), first, "it stays where it was");
    // The one-shot timer raises it again; it must not show a toolbar that was closed.
    desk.pump(Duration::from_millis(300));
    s().toolbar_open = false;
    unsafe { ShowWindow(s().toolbar, SW_HIDE) };
    desk.send(s().toolbar, WM_TIMER, RAISE_TIMER, 0);
    assert!(!visible(s().toolbar));
}

#[test]
#[ignore = "Shows a message box"]
fn a_picture_that_cannot_be_saved_or_copied_is_reported() {
    let desk = Desk::start_with(|settings| {
        settings.save = true;
        settings.copy = true;
    });
    let pixels = [10, 20, 30, 255].repeat(4);
    // The folder cannot be created because a file is in the way.
    let blocked = desk.sandbox.path().join("blocked");
    std::fs::write(&blocked, b"x").unwrap();
    let mut settings = config::load_settings();
    settings.screenshot_folder = blocked.join("inside").to_string_lossy().into_owned();
    config::save_settings(&settings).unwrap();
    let answer = answer_dialog(2);
    // SAFETY: the windows belong to this test.
    unsafe { deliver_capture(pixels.clone(), 2, 2, s().copy) };
    assert!(answer.join().unwrap(), "the save error is shown");
    assert!(!unsafe { toast::visible() });
    // The clipboard is held by someone else for longer than the retries last.
    settings.screenshot_folder = desk
        .sandbox
        .path()
        .join("shots")
        .to_string_lossy()
        .into_owned();
    config::save_settings(&settings).unwrap();
    let holder = hold_clipboard(Duration::from_millis(1500));
    let answer = answer_dialog(2);
    unsafe { deliver_capture(pixels, 2, 2, s().copy) };
    assert!(answer.join().unwrap(), "the copy error is shown");
    holder.join().unwrap();
    assert_eq!(shots(&desk).len(), 1, "the file was saved all the same");
}

#[test]
#[ignore = "Shows a dialog"]
fn save_as_lets_the_user_pick_the_file_or_cancel() {
    let desk = Desk::start_with(|settings| {
        settings.ask = true;
        settings.save = true;
        settings.copy = false;
    });
    let pixels = [10, 20, 30, 255].repeat(4);
    let answer = answer_dialog(2);
    // SAFETY: the windows belong to this test.
    unsafe { deliver_capture(pixels.clone(), 2, 2, s().copy) };
    assert!(answer.join().unwrap());
    assert!(shots(&desk).is_empty(), "cancelling saves nothing");
    let answer = answer_dialog(1);
    unsafe { deliver_capture(pixels, 2, 2, s().copy) };
    assert!(answer.join().unwrap());
    let files = shots(&desk);
    assert_eq!(files.len(), 1);
    assert!(files[0].extension().is_some_and(|e| e == "png"));
}

#[test]
fn the_border_is_only_drawn_when_it_is_switched_on() {
    let desk = Desk::start();
    let plain = [1, 2, 3, 255].repeat(36);
    // SAFETY: the state belongs to this test.
    unsafe {
        assert_eq!(with_outline(plain.clone(), 6, 6), plain);
        s().outline = true;
        s().outline_color = 2;
        s().outline_width = 0;
        let drawn = with_outline(plain.clone(), 6, 6);
        let green = annotate::COLORS[2];
        assert_eq!(pixel(&drawn, 6, 0, 0), [green[2], green[1], green[0], 255]);
        assert_eq!(pixel(&drawn, 6, 3, 3), [1, 2, 3, 255]);
    }
    drop(desk);
}

#[test]
fn the_dim_layer_is_half_as_bright_as_the_screen() {
    let desk = Desk::start();
    // SAFETY: the bitmap and its device context belong to this test.
    unsafe {
        let dc = GetDC(null_mut());
        assert!(store_bitmap(dc, 8, 8));
        ReleaseDC(null_mut(), dc);
        let white = CreateSolidBrush(0x00ffffff);
        let area = RECT {
            left: 0,
            top: 0,
            right: 8,
            bottom: 8,
        };
        FillRect(s().screen_dc, &area, white);
        DeleteObject(white);
        assert!(dim(8, 8));
        let canvas = s().dim_canvas.as_mut().unwrap();
        assert!(canvas
            .pixels()
            .iter()
            .all(|&p| p & 0x00ffffff == 0x007f7f7f));
        cleanup();
        assert!(s().screen_dc.is_null() && s().screen.is_null() && s().dim_canvas.is_none());
    }
    drop(desk);
}

mod crash;
mod differential;
