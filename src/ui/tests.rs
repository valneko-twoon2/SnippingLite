use super::*;

use crate::test_support::{CAPTIONS, THEME};

#[test]
fn document_roles_distinguish_windows_with_the_same_caption() {
    let _turn = CAPTIONS.lock().unwrap_or_else(|e| e.into_inner());
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

fn test_window(title: &str) -> HWND {
    use windows_sys::Win32::UI::WindowsAndMessaging::{CreateWindowExW, WS_POPUP};
    // SAFETY: a private, hidden window of the system STATIC class.
    unsafe {
        CreateWindowExW(
            0,
            w("STATIC").as_ptr(),
            w(title).as_ptr(),
            WS_POPUP,
            0,
            0,
            40,
            30,
            null_mut(),
            null_mut(),
            null_mut(),
            null(),
        )
    }
}

#[test]
fn a_window_process_tags_only_its_own_window() {
    use windows_sys::Win32::UI::WindowsAndMessaging::DestroyWindow;
    let _turn = CAPTIONS.lock().unwrap_or_else(|e| e.into_inner());
    let mine = test_window(crate::SETTINGS_TITLE);
    // SAFETY: the window is created and destroyed here.
    unsafe {
        let found = identify_app_window("SnippingLite.Test.Identity");
        assert_eq!(found, mine);
        assert_eq!(find_app_window("SnippingLite.Test.Identity"), mine);
        DestroyWindow(mine);
        // Boundary: with no window of that caption left, nothing is found or tagged.
        assert!(identify_app_window("SnippingLite.Test.Identity").is_null());
        assert!(find_app_window("SnippingLite.Test.Identity").is_null());
    }
}

#[test]
fn the_theme_follows_the_choice() {
    let _guard = THEME.lock().unwrap_or_else(|e| e.into_inner());
    set_theme_choice(AppTheme::Light);
    let light = theme();
    set_theme_choice(AppTheme::Dark);
    let dark = theme();
    set_theme_choice(AppTheme::System);
    let system = theme();
    assert!(!light.dark && dark.dark);
    assert_eq!(system.dark, system_dark());
    // Golden: the text and background colors the windows are drawn with.
    assert_eq!(
        (light.text, light.bg_ref, light.accent),
        (0xe4000000, 0x00f3f3f3, 0xff0067c0)
    );
    assert_eq!(
        (dark.text, dark.bg_ref, dark.accent),
        (0xffffffff, 0x00202020, 0xff4cc2ff)
    );
    assert_ne!(light.text_ref, dark.text_ref);
}

#[test]
fn the_screen_scale_is_a_plausible_dpi_ratio() {
    // SAFETY: plain system queries.
    let scale = unsafe { scale_at(POINT { x: 0, y: 0 }) };
    assert!((1.0..=4.0).contains(&scale), "{scale}");
    // A point far outside every monitor still resolves to the nearest one.
    let far = unsafe {
        scale_at(POINT {
            x: -100_000,
            y: 100_000,
        })
    };
    assert!((1.0..=4.0).contains(&far));
}

#[test]
fn window_styling_applies_to_a_real_window_without_failing() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{DestroyWindow, GetWindowRect};
    let window = test_window("styling");
    assert!(!window.is_null());
    // SAFETY: the window is private and destroyed at the end.
    unsafe {
        let theme = theme();
        acrylic(window, true);
        mica(window, false);
        window_theme(window, &theme);
        disable_transitions(window);
        cloak(window, true);
        cloak(window, false);
        let mut before: RECT = std::mem::zeroed();
        GetWindowRect(window, &mut before);
        raise(window);
        let mut after: RECT = std::mem::zeroed();
        GetWindowRect(window, &mut after);
        // Raising changes the stacking order only, never the position or size.
        assert_eq!(
            (before.left, before.top, before.right, before.bottom),
            (after.left, after.top, after.right, after.bottom)
        );
        DestroyWindow(window);
    }
    // Windows 11 22H2 or later has the Mica backdrop; any answer is valid on other builds.
    let _ = supports_mica();
}

#[test]
fn the_app_icon_exists_at_every_size_in_use() {
    for size in [16u32, 20, 24, 32, 48, 64] {
        let rgba = icon_rgba(size);
        assert_eq!(rgba.len(), (size * size * 4) as usize, "{size}");
        assert!(rgba.chunks(4).any(|p| p[3] > 0), "{size} is not empty");
        // SAFETY: loads and destroys an icon owned by this test.
        unsafe {
            let icon = app_icon(size);
            assert!(!icon.is_null(), "{size}");
            windows_sys::Win32::UI::WindowsAndMessaging::DestroyIcon(icon);
        }
    }
    // Contract: resized icons use straight alpha, so no channel exceeds what its alpha allows
    // for fully opaque pixels, and transparent pixels carry no color.
    for pixel in icon_rgba(32).chunks(4).filter(|p| p[3] == 0) {
        assert_eq!(&pixel[..3], &[0, 0, 0]);
    }
}

#[test]
fn rectangles_round_to_the_nearest_pixel() {
    let r = rect(0.4, 0.5, 9.5, 9.49);
    assert_eq!((r.left, r.top, r.right, r.bottom), (0, 1, 10, 9));
    let r = rect(-0.5, -0.4, 0.0, 2.6);
    assert_eq!((r.left, r.top, r.right, r.bottom), (-1, 0, 0, 3));
}

#[test]
fn a_font_is_created_and_released() {
    // SAFETY: the font is dropped without being selected anywhere.
    let font = unsafe { Font::new(TEXT_FONT, 14.0, FW_NORMAL as i32) };
    assert!(!font.0.is_null());
    drop(font);
    drop(Font(null_mut()));
}

#[test]
fn a_canvas_needs_at_least_one_pixel() {
    // SAFETY: plain GDI allocation.
    unsafe {
        for (width, height) in [(0, 5), (5, 0), (-1, 5), (5, -1), (0, 0)] {
            assert!(Canvas::new(width, height).is_none(), "{width}x{height}");
        }
        let mut canvas = Canvas::new(1, 1).unwrap();
        assert_eq!(
            (canvas.width, canvas.height, canvas.pixels().len()),
            (1, 1, 1)
        );
        assert!(!canvas.dc().is_null());
    }
}

#[test]
fn a_rounded_rectangle_is_solid_inside_and_clear_outside() {
    // SAFETY: plain GDI allocation.
    let mut canvas = unsafe { Canvas::new(20, 20) }.unwrap();
    canvas.fill_round_rect([2.0, 2.0, 18.0, 18.0], 6.0, 0xffff0000);
    let at = |canvas: &mut Canvas, x: usize, y: usize| canvas.pixels()[y * 20 + x];
    assert_eq!(at(&mut canvas, 10, 10), 0xffff0000, "center");
    assert_eq!(at(&mut canvas, 0, 0), 0, "outside");
    assert_eq!(at(&mut canvas, 19, 19), 0, "outside");
    // The corner is cut away by the radius, while the middle of an edge is solid.
    assert_eq!(at(&mut canvas, 2, 2), 0, "rounded corner");
    assert_eq!(at(&mut canvas, 10, 2), 0xffff0000, "edge middle");
    // Anti-aliasing: somewhere along the arc a pixel is partly covered.
    let partial = canvas
        .pixels()
        .iter()
        .any(|&p| (p >> 24) != 0 && (p >> 24) != 255);
    assert!(partial);
    // A rectangle larger than the canvas and an empty one do not fail.
    canvas.fill_round_rect([-50.0, -50.0, 80.0, 80.0], 500.0, 0xff00ff00);
    canvas.fill_round_rect([5.0, 5.0, 5.0, 5.0], 3.0, 0xff0000ff);
    canvas.fill_round_rect([5.0, 5.0, 8.0, 8.0], -3.0, 0xff0000ff);
    assert_eq!(at(&mut canvas, 15, 15), 0xff00ff00);
}

#[test]
fn the_surface_has_a_border_and_a_fill_in_both_themes() {
    for (dark, fill, border) in [
        (true, 0xff202020, 0xff414141),
        (false, 0xfff3f3f3, 0xffd0d0d0),
    ] {
        // SAFETY: plain GDI allocation.
        let mut canvas = unsafe { Canvas::new(60, 40) }.unwrap();
        canvas.surface(dark, 1.0);
        let pixels = canvas.pixels();
        assert_eq!(pixels[20 * 60 + 30], fill, "dark={dark}");
        assert_eq!(pixels[20 * 60], border, "dark={dark}");
        // Doubling the scale thickens the border and rounds more of the corner.
        canvas.surface(dark, 2.0);
        assert_eq!(canvas.pixels()[20 * 60 + 1], border, "dark={dark}");
    }
}

#[test]
fn text_is_drawn_inside_its_rectangle_only() {
    // SAFETY: all GDI objects are local and the text bounds are clamped to the canvas.
    unsafe {
        let mut canvas = Canvas::new(100, 30).unwrap();
        canvas.pixels().fill(0xff000000);
        let font = Font::new(TEXT_FONT, 16.0, FW_NORMAL as i32);
        canvas.text(
            "Hello",
            &font,
            rect(10.0, 5.0, 60.0, 25.0),
            DT_VCENTER | DT_SINGLELINE,
            0xffffffff,
        );
        let snapshot = canvas.pixels().to_vec();
        let lit = |x: usize, y: usize| snapshot[y * 100 + x] != 0xff000000;
        let inside = (10..60).any(|x| (5..25).any(|y| lit(x, y)));
        let outside = (60..100).any(|x| (0..30).any(|y| lit(x, y)));
        assert!(inside && !outside);
        // A rectangle beyond the canvas is clamped; an empty one draws nothing.
        canvas.text(
            "x",
            &font,
            rect(-20.0, -20.0, 500.0, 500.0),
            DT_VCENTER | DT_SINGLELINE,
            0xffffffff,
        );
        canvas.text(
            "x",
            &font,
            rect(50.0, 5.0, 50.0, 25.0),
            DT_VCENTER | DT_SINGLELINE,
            0xffffffff,
        );
        canvas.text(
            "x",
            &font,
            rect(200.0, 5.0, 300.0, 25.0),
            DT_VCENTER | DT_SINGLELINE,
            0xffffffff,
        );
    }
}

#[test]
fn the_icon_is_composited_and_clipped_at_the_canvas_edges() {
    // SAFETY: plain GDI allocation.
    let mut canvas = unsafe { Canvas::new(40, 40) }.unwrap();
    canvas.app_icon(4, 4, 32);
    assert!(canvas.pixels().iter().any(|&p| p != 0));
    assert_eq!(canvas.pixels()[0], 0, "outside the icon");
    // Partly outside the canvas on every side must not write out of bounds.
    let mut canvas = unsafe { Canvas::new(40, 40) }.unwrap();
    for (x, y) in [(-16, -16), (30, 30), (-16, 30), (30, -16)] {
        canvas.app_icon(x, y, 32);
    }
}

#[test]
fn a_canvas_is_copied_to_a_device_context() {
    // SAFETY: both canvases are local; `present` copies between their device contexts.
    unsafe {
        let mut source = Canvas::new(8, 8).unwrap();
        source.pixels().fill(0xff123456);
        let mut target = Canvas::new(8, 8).unwrap();
        source.present(target.dc());
        assert!(target.pixels().iter().all(|&p| p & 0x00ffffff == 0x123456));
    }
}

#[test]
fn over_and_blend_cover_their_extreme_cases() {
    // Transparent and opaque sources, and zero coverage, leave a known result.
    assert_eq!(blend(0xff102030, 0), 0xff102030);
    assert_eq!(blend(0x12345678, 0xffaabbcc), 0xffaabbcc);
    assert_eq!(over(0xff102030, 0xffffffff, 0.0), 0xff102030);
    assert_eq!(over(0xff102030, 0xff112233, 1.0), 0xff112233);
    assert_eq!(over(0, 0x00ffffff, 1.0), 0);
}

mod dpi;
