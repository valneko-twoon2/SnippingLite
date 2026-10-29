//! Settings window. It runs in its own process (`--settings`) so the egui and
//! OpenGL memory is released when the window closes and the tray process stays small.

use eframe::egui::{
    self, pos2, Align, Align2, Color32, CornerRadius, FontData, FontDefinitions, FontFamily,
    FontId, FontTweak, Layout, RichText, Sense, Stroke, StrokeKind, Theme, UiBuilder, Vec2,
};
use std::{
    fs,
    path::{Path, PathBuf},
    ptr::null,
    sync::Arc,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{
        AllowSetForegroundWindow, FindWindowW, PostMessageW, ASFW_ANY, WM_COMMAND,
    },
};

use crate::i18n::tr;
use crate::{
    config::{self, DELAYS, FRAME_RATES, HISTORY_LIMITS},
    imaging, ui, w, MAIN, SETTINGS_TITLE, SETTINGS_WINDOW, TRAY_HISTORY, TRAY_RELOAD,
};

mod app;
mod expander;
mod icons;
mod widgets;
use app::*;
use expander::*;
use icons::*;
use widgets::*;

const SEMIBOLD: &str = "semibold";
/// Downward shift of Japanese glyphs as a fraction of the font size.
const JAPANESE_Y_OFFSET: f32 = 0.2;
const WIDTH: f32 = 696.0;
const HEIGHT: f32 = 760.0;
const MARGIN: f32 = 56.0;
const ROW: f32 = 64.0;

pub(crate) struct Palette {
    /// Translucent colors let the Mica backdrop show through.
    pub(crate) mica: bool,
    pub(crate) bg: Color32,
    pub(crate) card: Color32,
    pub(crate) stroke: Color32,
    pub(crate) text: Color32,
    pub(crate) secondary: Color32,
    pub(crate) field: Color32,
    pub(crate) accent: Color32,
    pub(crate) on_accent: Color32,
    pub(crate) warning: Color32,
}

impl Palette {
    pub(crate) fn new(dark: bool, mica: bool) -> Palette {
        let mut p = Palette::opaque(dark);
        p.mica = mica;
        if mica {
            p.bg = Color32::TRANSPARENT;
            p.card = if dark {
                Color32::from_rgba_unmultiplied(255, 255, 255, 13)
            } else {
                Color32::from_rgba_unmultiplied(255, 255, 255, 179)
            };
            p.stroke = if dark {
                Color32::from_rgba_unmultiplied(0, 0, 0, 56)
            } else {
                Color32::from_rgba_unmultiplied(0, 0, 0, 15)
            };
        }
        p
    }

    fn opaque(dark: bool) -> Palette {
        if dark {
            Palette {
                mica: false,
                bg: Color32::from_rgb(0x20, 0x20, 0x20),
                card: Color32::from_rgb(0x2b, 0x2b, 0x2b),
                stroke: Color32::from_rgb(0x1d, 0x1d, 0x1d),
                text: Color32::WHITE,
                secondary: Color32::from_rgb(0xc5, 0xc5, 0xc5),
                field: Color32::from_rgb(0x33, 0x33, 0x33),
                accent: Color32::from_rgb(0x4c, 0xc2, 0xff),
                on_accent: Color32::BLACK,
                warning: Color32::from_rgb(0xff, 0x99, 0xa4),
            }
        } else {
            Palette {
                mica: false,
                bg: Color32::from_rgb(0xf3, 0xf3, 0xf3),
                card: Color32::from_rgb(0xfb, 0xfb, 0xfb),
                stroke: Color32::from_rgb(0xe5, 0xe5, 0xe5),
                text: Color32::from_rgb(0x1a, 0x1a, 0x1a),
                secondary: Color32::from_rgb(0x61, 0x61, 0x61),
                field: Color32::WHITE,
                accent: Color32::from_rgb(0x00, 0x67, 0xc0),
                on_accent: Color32::WHITE,
                warning: Color32::from_rgb(0xc4, 0x2b, 0x1c),
            }
        }
    }
}

pub fn run(hotkey_ok: bool) {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(SETTINGS_TITLE)
            .with_inner_size([WIDTH, HEIGHT])
            .with_resizable(false)
            .with_maximize_button(false)
            .with_transparent(ui::supports_mica())
            .with_icon(egui::IconData {
                rgba: ui::icon_rgba(64),
                width: 64,
                height: 64,
            }),
        ..Default::default()
    };
    let _ = eframe::run_native(
        SETTINGS_TITLE,
        options,
        Box::new(move |cc| {
            if !install_fonts(&cc.egui_ctx) {
                return Err("No usable system font was found.".into());
            }
            let mica = ui::supports_mica();
            apply_theme(&cc.egui_ctx, config::load_settings().theme);
            install_visuals(&cc.egui_ctx, mica);
            Ok(Box::new(SettingsApp::new(hotkey_ok, mica)))
        }),
    );
}

/// Loads Segoe UI, as in the Windows 11 settings app, falling back to other
/// Windows fonts, plus a Japanese font. egui ships no fonts here, so returns false if none is found.
pub(crate) fn install_fonts(ctx: &egui::Context) -> bool {
    let mut fonts = FontDefinitions::empty();
    let folder = std::env::var_os("WINDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
        .join("Fonts");
    let mut load = |files: &[&str], name: &str, tweak: FontTweak| {
        let bytes = files.iter().find_map(|f| fs::read(folder.join(f)).ok())?;
        fonts.font_data.insert(
            name.to_owned(),
            Arc::new(FontData::from_owned(bytes).tweak(tweak)),
        );
        Some(name.to_owned())
    };
    let regular = load(
        &["segoeui.ttf", "tahoma.ttf", "arial.ttf", "micross.ttf"],
        "regular",
        FontTweak::default(),
    );
    let semibold = load(&["seguisb.ttf"], "semibold", FontTweak::default());
    // Mixing Segoe UI with a Japanese fallback misaligns the baselines, so the
    // Japanese font leads and also draws the Latin text; Segoe UI is the fallback
    // for systems without one.
    // Yu Gothic sits high in its line box, so its glyphs are shifted down to look
    // vertically centered in rows and buttons.
    let centered = || FontTweak {
        y_offset_factor: JAPANESE_Y_OFFSET,
        ..FontTweak::default()
    };
    let japanese = load(
        &["YuGothM.ttc", "meiryo.ttc", "msgothic.ttc"],
        "japanese",
        centered(),
    );
    let japanese_bold = load(&["YuGothB.ttc", "meiryob.ttc"], "japanese_bold", centered());
    let Some(regular) = regular else {
        return false;
    };
    let body: Vec<String> = japanese
        .iter()
        .cloned()
        .chain(Some(regular.clone()))
        .collect();
    fonts
        .families
        .insert(FontFamily::Proportional, body.clone());
    fonts.families.insert(FontFamily::Monospace, body);
    fonts.families.insert(
        FontFamily::Name(SEMIBOLD.into()),
        japanese_bold
            .or(japanese)
            .into_iter()
            .chain(semibold)
            .chain(Some(regular))
            .collect(),
    );
    ctx.set_fonts(fonts);
    true
}

/// Applies the chosen app theme to `ui::theme()` and to the egui context.
pub(crate) fn apply_theme(ctx: &egui::Context, choice: config::AppTheme) {
    ui::set_theme_choice(choice);
    ctx.set_theme(match choice {
        config::AppTheme::System => egui::ThemePreference::System,
        config::AppTheme::Light => egui::ThemePreference::Light,
        config::AppTheme::Dark => egui::ThemePreference::Dark,
    });
}

pub(crate) fn install_visuals(ctx: &egui::Context, mica: bool) {
    for theme in [Theme::Dark, Theme::Light] {
        let dark = theme == Theme::Dark;
        let p = Palette::new(dark, mica);
        let mut v = if dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        v.panel_fill = p.bg;
        v.window_fill = p.card;
        v.extreme_bg_color = p.field;
        v.override_text_color = Some(p.text);
        v.window_corner_radius = CornerRadius::same(8);
        v.menu_corner_radius = CornerRadius::same(8);
        for widget in [
            &mut v.widgets.noninteractive,
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
            &mut v.widgets.open,
        ] {
            widget.corner_radius = CornerRadius::same(4);
        }
        v.widgets.inactive.bg_fill = p.field;
        v.widgets.inactive.weak_bg_fill = p.field;
        v.widgets.hovered.bg_fill = if dark {
            Color32::from_rgb(0x3a, 0x3a, 0x3a)
        } else {
            Color32::from_rgb(0xf6, 0xf6, 0xf6)
        };
        v.widgets.hovered.weak_bg_fill = v.widgets.hovered.bg_fill;
        v.widgets.inactive.bg_stroke = Stroke::new(1.0, p.stroke);
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, p.stroke);
        v.selection.bg_fill = p.accent;
        v.selection.stroke = Stroke::new(1.0, p.on_accent);
        ctx.set_visuals_of(theme, v);
    }
    ctx.all_styles_mut(|style| {
        style.text_styles.insert(
            egui::TextStyle::Body,
            FontId::new(14.0, FontFamily::Proportional),
        );
        style.text_styles.insert(
            egui::TextStyle::Button,
            FontId::new(14.0, FontFamily::Proportional),
        );
        style.text_styles.insert(
            egui::TextStyle::Small,
            FontId::new(12.0, FontFamily::Proportional),
        );
        style.interaction.tooltip_delay = 0.4;
    });
}

pub(crate) fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
}

#[cfg(test)]
mod tests;
