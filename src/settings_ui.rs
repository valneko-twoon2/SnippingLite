//! Settings window. It runs in its own process (`--settings`) so the egui and
//! OpenGL memory is released when the window closes and the tray process stays small.

use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId,
    Layout, RichText, Sense, Stroke, StrokeKind, Theme, UiBuilder, Vec2,
};
use std::{
    fs,
    path::PathBuf,
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

use crate::{
    config::{self, DELAYS},
    ui, w, MAIN, NEW, SETTINGS_TITLE, TRAY_RELOAD,
};

const SEMIBOLD: &str = "semibold";
const WIDTH: f32 = 440.0;
const HEIGHT: f32 = 424.0;
const MARGIN: f32 = 24.0;
const ROW: f32 = 48.0;

struct Palette {
    bg: Color32,
    card: Color32,
    stroke: Color32,
    text: Color32,
    secondary: Color32,
    field: Color32,
    accent: Color32,
    on_accent: Color32,
    warning: Color32,
}

impl Palette {
    fn new(dark: bool) -> Palette {
        if dark {
            Palette {
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
            .with_maximize_button(false),
        ..Default::default()
    };
    let _ = eframe::run_native(
        SETTINGS_TITLE,
        options,
        Box::new(move |cc| {
            if !install_fonts(&cc.egui_ctx) {
                return Err("No usable system font was found.".into());
            }
            install_visuals(&cc.egui_ctx);
            Ok(Box::new(SettingsApp::new(hotkey_ok)))
        }),
    );
}

/// Loads Segoe UI, as in the Windows 11 settings app, falling back to other
/// Windows fonts. egui ships no fonts here, so returns false if none is found.
fn install_fonts(ctx: &egui::Context) -> bool {
    let mut fonts = FontDefinitions::empty();
    let folder = std::env::var_os("WINDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
        .join("Fonts");
    let mut load = |files: &[&str], name: &str| {
        let bytes = files.iter().find_map(|f| fs::read(folder.join(f)).ok())?;
        fonts
            .font_data
            .insert(name.to_owned(), Arc::new(FontData::from_owned(bytes)));
        Some(name.to_owned())
    };
    let regular = load(
        &["segoeui.ttf", "tahoma.ttf", "arial.ttf", "micross.ttf"],
        "regular",
    );
    let semibold = load(&["seguisb.ttf"], "semibold");
    let Some(regular) = regular else {
        return false;
    };
    fonts
        .families
        .insert(FontFamily::Proportional, vec![regular.clone()]);
    fonts
        .families
        .insert(FontFamily::Monospace, vec![regular.clone()]);
    fonts.families.insert(
        FontFamily::Name(SEMIBOLD.into()),
        semibold.into_iter().chain(Some(regular)).collect(),
    );
    ctx.set_fonts(fonts);
    true
}

fn install_visuals(ctx: &egui::Context) {
    for theme in [Theme::Dark, Theme::Light] {
        let dark = theme == Theme::Dark;
        let p = Palette::new(dark);
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

struct SettingsApp {
    copy: bool,
    save: bool,
    ask: bool,
    delay: u32,
    hotkey_ok: bool,
    notice: Option<Instant>,
    framed: bool,
}

impl SettingsApp {
    fn new(hotkey_ok: bool) -> SettingsApp {
        let (copy, save, ask, delay) = config::load_settings();
        SettingsApp {
            copy,
            save,
            ask,
            delay,
            hotkey_ok,
            notice: None,
            framed: false,
        }
    }

    /// Writes the settings and tells the tray process to reload them.
    fn commit(&self) {
        let _ = config::save_settings(self.copy, self.save, self.ask, self.delay);
        post(TRAY_RELOAD, 0);
    }
}

fn tray_window() -> HWND {
    // SAFETY: the class name is a valid, nul-terminated UTF-16 string.
    unsafe { FindWindowW(w(MAIN).as_ptr(), null()) }
}

fn post(message: u32, wparam: usize) {
    let tray = tray_window();
    if !tray.is_null() {
        // SAFETY: posting a message to a window handle is always sound.
        unsafe { PostMessageW(tray, message, wparam, 0) };
    }
}

fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
}

impl eframe::App for SettingsApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let dark = ctx.theme() == Theme::Dark;
        let p = Palette::new(dark);
        if !self.framed {
            self.framed = true;
            // SAFETY: the title is a valid UTF-16 string and the handle is
            // only passed to DWM attribute calls.
            unsafe {
                let hwnd = FindWindowW(null(), w(SETTINGS_TITLE).as_ptr());
                if !hwnd.is_null() {
                    ui::disable_transitions(hwnd);
                    ui::window_theme(hwnd, &ui::theme());
                }
            }
        }
        ui.painter().rect_filled(ui.max_rect(), 0.0, p.bg);
        let rect = ui.max_rect().shrink2(Vec2::new(MARGIN, 0.0));
        let mut body = ui.new_child(UiBuilder::new().max_rect(rect));
        let ui = &mut body;
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        ui.add_space(20.0);
        ui.label(RichText::new("Snipping Lite").font(semibold(20.0)));
        ui.add_space(4.0);
        ui.label(
            RichText::new("Capture screenshots with Alt + Shift + S")
                .size(12.0)
                .color(p.secondary),
        );
        ui.add_space(12.0);
        if accent_button(ui, "New snip", &p).clicked() {
            // Let the tray process bring the capture overlay to the foreground.
            // SAFETY: the call has no pointer arguments.
            unsafe { AllowSetForegroundWindow(ASFW_ANY) };
            post(WM_COMMAND, NEW);
        }
        ui.add_space(20.0);
        ui.label(RichText::new("Settings").font(semibold(14.0)));
        ui.add_space(8.0);

        let before = (self.copy, self.save, self.ask, self.delay);
        let width = ui.available_width();
        egui::Frame::new()
            .fill(p.card)
            .stroke(Stroke::new(1.0, p.stroke))
            .corner_radius(8.0)
            .show(ui, |ui| {
                ui.set_width(width - 2.0);
                row(ui, &p, "Automatically copy screenshots", |ui| {
                    toggle(ui, &mut self.copy, "Automatically copy screenshots", &p);
                });
                divider(ui, &p);
                row(ui, &p, "Automatically save screenshots", |ui| {
                    toggle(ui, &mut self.save, "Automatically save screenshots", &p);
                });
                divider(ui, &p);
                row(ui, &p, "Choose a file name when saving", |ui| {
                    toggle(ui, &mut self.ask, "Choose a file name when saving", &p);
                });
                divider(ui, &p);
                row(ui, &p, "Capture delay", |ui| {
                    let text = match self.delay {
                        0 => "None".to_owned(),
                        n => format!("{n} seconds"),
                    };
                    egui::ComboBox::from_id_salt("delay")
                        .selected_text(text)
                        .width(124.0)
                        .show_ui(ui, |ui| {
                            for d in DELAYS {
                                let label = match d {
                                    0 => "None".to_owned(),
                                    n => format!("{n} seconds"),
                                };
                                ui.selectable_value(&mut self.delay, d, label);
                            }
                        });
                });
            });
        if !self.copy && !self.save {
            // At least one destination must stay enabled.
            (self.copy, self.save) = (before.0, before.1);
            if !before.0 && !before.1 {
                self.copy = true;
            }
            self.notice = Some(Instant::now());
        }
        if (self.copy, self.save, self.ask, self.delay) != before {
            self.commit();
        }

        ui.add_space(12.0);
        ui.label(
            RichText::new("Saved images: Pictures\\Screenshots (PNG or BMP)")
                .size(12.0)
                .color(p.secondary),
        );
        ui.add_space(2.0);
        let warning = self
            .notice
            .is_some_and(|t| t.elapsed() < Duration::from_secs(4));
        let (status, color) = if warning {
            (
                "Choose at least one destination for screenshots.",
                p.warning,
            )
        } else if self.hotkey_ok {
            ("Shortcut: Alt + Shift + S", p.secondary)
        } else {
            (
                "Shortcut unavailable: another application is using it.",
                p.warning,
            )
        };
        ui.label(RichText::new(status).size(12.0).color(color));
        if warning {
            ctx.request_repaint_after(Duration::from_secs(4));
        }
    }
}

/// A settings row: the label on the left and the control on the right.
fn row(ui: &mut egui::Ui, p: &Palette, label: &str, content: impl FnOnce(&mut egui::Ui)) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW), Sense::hover());
    ui.painter().text(
        rect.left_center() + Vec2::new(16.0, 0.0),
        Align2::LEFT_CENTER,
        label,
        FontId::new(14.0, FontFamily::Proportional),
        p.text,
    );
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(rect.shrink2(Vec2::new(16.0, 0.0)))
            .layout(Layout::right_to_left(Align::Center)),
    );
    content(&mut child);
}

fn divider(ui: &mut egui::Ui, p: &Palette) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, p.stroke);
}

/// A Windows 11 toggle switch.
fn toggle(ui: &mut egui::Ui, on: &mut bool, label: &str, p: &Palette) -> egui::Response {
    let (rect, mut response) = ui.allocate_exact_size(Vec2::new(40.0, 20.0), Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::CheckBox, ui.is_enabled(), *on, label)
    });
    let t = ui.ctx().animate_bool_responsive(response.id, *on);
    let radius = CornerRadius::same(10);
    let painter = ui.painter();
    if *on || t > 0.0 {
        painter.rect_filled(rect, radius, p.accent.gamma_multiply(t));
    }
    if t < 1.0 {
        painter.rect_stroke(
            rect,
            radius,
            Stroke::new(1.0, p.secondary.gamma_multiply(1.0 - t)),
            StrokeKind::Inside,
        );
    }
    let knob = if response.hovered() || response.has_focus() {
        7.0
    } else {
        6.0
    };
    let x = egui::lerp((rect.left() + 10.0)..=(rect.right() - 10.0), t);
    let color = if *on { p.on_accent } else { p.secondary };
    painter.circle_filled(egui::pos2(x, rect.center().y), knob, color);
    if response.has_focus() {
        painter.rect_stroke(
            rect.expand(3.0),
            CornerRadius::same(13),
            Stroke::new(2.0, p.text),
            StrokeKind::Outside,
        );
    }
    response
}

/// The accent colored primary button.
fn accent_button(ui: &mut egui::Ui, label: &str, p: &Palette) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(120.0, 32.0), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), label));
    let fill = if response.is_pointer_button_down_on() {
        p.accent.gamma_multiply(0.8)
    } else if response.hovered() {
        p.accent.gamma_multiply(0.9)
    } else {
        p.accent
    };
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(4), fill);
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::new(14.0, FontFamily::Proportional),
        p.on_accent,
    );
    if response.has_focus() {
        painter.rect_stroke(
            rect.expand(2.0),
            CornerRadius::same(6),
            Stroke::new(2.0, p.text),
            StrokeKind::Outside,
        );
    }
    response
}
