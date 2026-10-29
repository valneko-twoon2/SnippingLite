use super::*;

#[test]
fn the_palette_has_a_fixed_look_per_theme_and_backdrop() {
    // Decision table: dark or light, with or without the translucent Mica backdrop.
    let light = Palette::new(false, false);
    assert!(!light.mica);
    assert_eq!(light.bg, Color32::from_rgb(0xf3, 0xf3, 0xf3));
    assert_eq!(light.accent, Color32::from_rgb(0x00, 0x67, 0xc0));
    let dark = Palette::new(true, false);
    assert_eq!(dark.bg, Color32::from_rgb(0x20, 0x20, 0x20));
    assert_eq!(dark.on_accent, Color32::BLACK);
    let light_mica = Palette::new(false, true);
    assert!(light_mica.mica);
    assert_eq!(light_mica.bg, Color32::TRANSPARENT);
    assert_eq!(
        light_mica.card,
        Color32::from_rgba_unmultiplied(255, 255, 255, 179)
    );
    assert_eq!(
        light_mica.stroke,
        Color32::from_rgba_unmultiplied(0, 0, 0, 15)
    );
    let dark_mica = Palette::new(true, true);
    assert_eq!(
        dark_mica.card,
        Color32::from_rgba_unmultiplied(255, 255, 255, 13)
    );
    assert_eq!(
        dark_mica.stroke,
        Color32::from_rgba_unmultiplied(0, 0, 0, 56)
    );
    // The text colors do not depend on the backdrop.
    assert_eq!(dark_mica.text, dark.text);
    assert_eq!(light_mica.warning, light.warning);
}

#[test]
fn fonts_are_found_in_the_windows_folder_or_the_install_fails() {
    let mut sandbox = Sandbox::new();
    let ctx = egui::Context::default();
    assert!(install_fonts(&ctx));
    // A Windows folder without fonts leaves the window with nothing to draw text with.
    let empty = sandbox.path().join("windows");
    sandbox.set("WINDIR", &empty);
    assert!(!install_fonts(&ctx));
}

#[test]
fn the_app_theme_reaches_both_egui_and_the_native_surfaces() {
    let _guard = THEME.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    for (choice, preference, dark) in [
        (
            config::AppTheme::Light,
            egui::ThemePreference::Light,
            Some(false),
        ),
        (
            config::AppTheme::Dark,
            egui::ThemePreference::Dark,
            Some(true),
        ),
        (
            config::AppTheme::System,
            egui::ThemePreference::System,
            None,
        ),
    ] {
        apply_theme(&ctx, choice);
        assert_eq!(ctx.options(|o| o.theme_preference), preference);
        if let Some(dark) = dark {
            assert_eq!(ui::theme().dark, dark);
        }
    }
}

#[test]
fn visuals_follow_the_palette_in_both_themes() {
    let ctx = egui::Context::default();
    for mica in [false, true] {
        install_visuals(&ctx, mica);
        let dark = ctx.style_of(Theme::Dark).visuals.clone();
        let light = ctx.style_of(Theme::Light).visuals.clone();
        assert_eq!(dark.panel_fill, Palette::new(true, mica).bg);
        assert_eq!(light.window_fill, Palette::new(false, mica).card);
        assert_eq!(dark.override_text_color, Some(Color32::WHITE));
        assert_eq!(light.selection.bg_fill, Palette::new(false, mica).accent);
        assert_eq!(light.window_corner_radius, CornerRadius::same(8));
        assert_eq!(dark.widgets.hovered.corner_radius, CornerRadius::same(4));
    }
    assert_eq!(semibold(20.0).size, 20.0);
    assert_eq!(semibold(12.0).family, FontFamily::Name(SEMIBOLD.into()));
}
