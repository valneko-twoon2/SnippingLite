use super::*;

#[test]
fn document_roles_distinguish_windows_with_the_same_caption() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, WS_POPUP};
    // SAFETY: these are private, hidden test windows using the system STATIC class.
    unsafe {
        let create = || {
            CreateWindowExW(
                0,
                w("STATIC").as_ptr(),
                w(crate::SETTINGS_TITLE).as_ptr(),
                WS_POPUP,
                0,
                0,
                1,
                1,
                null_mut(),
                null_mut(),
                null_mut(),
                null(),
            )
        };
        let settings = create();
        let history = create();
        assert!(!settings.is_null() && !history.is_null());
        SetPropW(
            settings,
            w("SnippingLite.Test.Settings").as_ptr(),
            1usize as HANDLE,
        );
        SetPropW(
            history,
            w("SnippingLite.Test.History").as_ptr(),
            1usize as HANDLE,
        );
        let found_settings = find_app_window("SnippingLite.Test.Settings");
        let found_history = find_app_window("SnippingLite.Test.History");
        DestroyWindow(settings);
        DestroyWindow(history);
        assert_eq!(found_settings, settings);
        assert_eq!(found_history, history);
    }
}

#[test]
fn empty_notification_text_leaves_the_canvas_unchanged() {
    // SAFETY: all GDI objects are local and the text bounds fit the canvas.
    unsafe {
        let mut canvas = Canvas::new(120, 24).expect("Create the notification canvas");
        canvas.pixels().fill(0xff202020);
        let font = Font::new(TEXT_FONT, 14.0, FW_NORMAL as i32);
        canvas.text(
            "",
            &font,
            rect(0.0, 0.0, 120.0, 24.0),
            DT_VCENTER | DT_SINGLELINE,
            0xffffffff,
        );
        assert!(canvas.pixels().iter().all(|pixel| *pixel == 0xff202020));
    }
}

#[test]
fn thumbnail_averages_and_premultiplies() {
    // Two opaque pixels (white and black) and two transparent ones.
    let pixels = [
        255, 255, 255, 255, 0, 0, 0, 255, //
        255, 255, 255, 0, 255, 255, 255, 0,
    ];
    assert_eq!(thumbnail(&pixels, 2, 2, 1, 1), vec![63, 63, 63, 127]);
    assert_eq!(
        thumbnail(&pixels, 2, 2, 2, 1),
        vec![127, 127, 127, 127, 0, 0, 0, 127]
    );
}

#[test]
fn contains_includes_the_top_left_edges_only() {
    let r = [10.0, 20.0, 30.0, 40.0];
    let at = |x, y| contains(r, POINT { x, y });
    // Inside, and the top and left edges.
    assert!(at(20, 30));
    assert!(at(10, 20));
    assert!(at(29, 39));
    // Just outside each edge; the right and bottom edges belong to the next area.
    assert!(!at(9, 30));
    assert!(!at(20, 19));
    assert!(!at(30, 30));
    assert!(!at(20, 40));
    // Fractional edges from scaling round toward the outside of the area.
    assert!(!contains([10.5, 0.0, 20.0, 10.0], POINT { x: 10, y: 5 }));
    assert!(contains([10.5, 0.0, 20.0, 10.0], POINT { x: 11, y: 5 }));
    // An empty or inverted area contains nothing.
    assert!(!contains([10.0, 10.0, 10.0, 20.0], POINT { x: 10, y: 15 }));
    assert!(!contains([20.0, 20.0, 10.0, 10.0], POINT { x: 15, y: 15 }));
    // Negative client coordinates are outside a button at the origin.
    assert!(!contains([0.0, 0.0, 8.0, 8.0], POINT { x: -1, y: 0 }));
}

#[test]
fn blend_keeps_premultiplied_range() {
    assert_eq!(blend(0xffffffff, 0x80000000), 0xff7f7f7f);
    assert_eq!(over(0, 0xffffffff, 0.5), 0x80808080);
}
