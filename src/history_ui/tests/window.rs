use super::*;

#[test]
#[ignore = "Shows a dialog"]
fn save_as_writes_the_picture_where_the_dialog_says() {
    let sandbox = Sandbox::new();
    let shots = sandbox.path().join("saved");
    fs::create_dir_all(&shots).unwrap();
    config::save_settings(&config::Settings {
        screenshot_folder: shots.to_string_lossy().into_owned(),
        ..config::parse_settings("")
    })
    .unwrap();
    let entry = seed(now(), 120, 80);
    let mut viewer = Viewer::open();
    let cell = viewer.screen.images()[0].center();
    viewer.click(cell);
    let answer = answer_dialog(2);
    viewer.click_text(tr("Save as..."));
    assert!(answer.join().unwrap());
    assert!(viewer.app.notice.is_none(), "cancelling says nothing");
    assert!(!shots
        .join(format!("Snipping Lite {}.png", entry.id))
        .exists());
    let answer = answer_dialog(1);
    viewer.click_text(tr("Save as..."));
    assert!(answer.join().unwrap());
    assert!(shots
        .join(format!("Snipping Lite {}.png", entry.id))
        .is_file());
    assert_eq!(viewer.app.notice.map(|n| n.1), Some(tr("Saved the image.")));
}

#[test]
fn show_in_folder_selects_the_file_in_explorer() {
    let _sandbox = Sandbox::new();
    let entry = seed(now(), 120, 80);
    let mut viewer = Viewer::open();
    let cell = viewer.screen.images()[0].center();
    viewer.click(cell);
    crate::test_support::take_shell_opened();
    viewer.click_text(tr("Show in folder"));
    assert_eq!(
        crate::test_support::take_shell_opened(),
        vec![(
            "explorer.exe".to_owned(),
            Some(format!("/select,\"{}\"", entry.image.display()))
        )]
    );
}

#[test]
fn hovering_and_keyboard_focus_highlight_the_controls() {
    let _sandbox = Sandbox::new();
    seed(now(), 300, 200);
    let mut viewer = Viewer::open();
    // Hover a range chip, then walk the focus over the chips and the cell with Tab.
    let chip = viewer.screen.find(Range::Day.label()).unwrap().center();
    viewer.act(|s, show| s.hover(chip, show));
    for _ in 0..8 {
        viewer.act(|s, show| s.key(Key::Tab, egui::Modifiers::NONE, show));
    }
    let cell = viewer.screen.images()[0].center();
    viewer.act(|s, show| s.hover(cell, show));
    viewer.click(cell);
    viewer.click_text(tr("Pen"));
    let swatch = viewer.screen.find(tr("Thin")).unwrap().center() - egui::vec2(120.0, 0.0);
    viewer.act(|s, show| s.hover(swatch, show));
    for _ in 0..12 {
        viewer.act(|s, show| s.key(Key::Tab, egui::Modifiers::NONE, show));
    }
    let back = viewer.screen.find(tr("Back")).unwrap().center();
    viewer.act(|s, show| s.hover(back, show));
    viewer.click(back);
    assert!(viewer.app.opened.is_none());
}

#[test]
fn the_window_frame_follows_the_theme_and_the_backdrop() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, GetPropW, WS_POPUP,
    };
    let _captions = CAPTIONS.lock().unwrap_or_else(|e| e.into_inner());
    let _sandbox = Sandbox::new();
    for mica in [false, true] {
        // SAFETY: a private, hidden window that stands in for the history window.
        unsafe {
            let hwnd = CreateWindowExW(
                0,
                w("STATIC").as_ptr(),
                w(HISTORY_TITLE).as_ptr(),
                WS_POPUP,
                0,
                0,
                10,
                10,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            );
            let viewer = Viewer::with(HistoryApp::new(mica, false));
            assert!(!GetPropW(hwnd, w(HISTORY_WINDOW).as_ptr()).is_null());
            let clear = viewer.app.clear_color(&egui::Visuals::dark());
            assert_eq!(clear == [0.0; 4], mica);
            DestroyWindow(hwnd);
        }
    }
}

#[test]
fn the_history_window_is_not_found_when_it_is_not_open() {
    let _captions = CAPTIONS.lock().unwrap_or_else(|e| e.into_inner());
    assert!(window().is_null());
}
