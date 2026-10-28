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
    config::{self, DELAYS, HISTORY_LIMITS},
    imaging, ui, w, MAIN, NEW, SETTINGS_TITLE, TRAY_HISTORY, TRAY_RELOAD,
};

const SEMIBOLD: &str = "semibold";
/// Downward shift of Japanese glyphs as a fraction of the font size.
const JAPANESE_Y_OFFSET: f32 = 0.2;
const WIDTH: f32 = 560.0;
const HEIGHT: f32 = 760.0;
const MARGIN: f32 = 24.0;
const ROW: f32 = 48.0;

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

struct SettingsApp {
    copy: bool,
    save: bool,
    ask: bool,
    delay: u32,
    history: bool,
    history_limit: usize,
    countdown: bool,
    freeze: bool,
    screenshot_folder: String,
    video_folder: String,
    hotkey_ok: bool,
    notice: Option<Instant>,
    framed: bool,
    mica: bool,
}

impl SettingsApp {
    fn new(hotkey_ok: bool, mica: bool) -> SettingsApp {
        let settings = config::load_settings();
        SettingsApp {
            copy: settings.copy,
            save: settings.save,
            ask: settings.ask,
            delay: settings.delay,
            history: settings.history,
            history_limit: settings.history_limit,
            countdown: settings.countdown,
            freeze: settings.freeze,
            screenshot_folder: settings.screenshot_folder,
            video_folder: settings.video_folder,
            hotkey_ok,
            notice: None,
            framed: false,
            mica,
        }
    }

    fn settings(&self) -> config::Settings {
        config::Settings {
            copy: self.copy,
            save: self.save,
            ask: self.ask,
            delay: self.delay,
            history: self.history,
            history_limit: self.history_limit,
            countdown: self.countdown,
            freeze: self.freeze,
            screenshot_folder: self.screenshot_folder.clone(),
            video_folder: self.video_folder.clone(),
        }
    }

    /// Writes the settings and tells the tray process to reload them.
    fn commit(&self) {
        let _ = config::save_settings(&self.settings());
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

pub(crate) fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
}

impl eframe::App for SettingsApp {
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        if self.mica {
            [0.0; 4]
        } else {
            visuals.panel_fill.to_normalized_gamma_f32()
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let dark = ctx.theme() == Theme::Dark;
        let p = Palette::new(dark, self.mica);
        if !self.framed {
            self.framed = true;
            // SAFETY: the title is a valid UTF-16 string and the handle is
            // only passed to DWM attribute calls.
            unsafe {
                let hwnd = FindWindowW(null(), w(SETTINGS_TITLE).as_ptr());
                if !hwnd.is_null() {
                    ui::disable_transitions(hwnd);
                    if self.mica {
                        ui::mica(hwnd, dark);
                    } else {
                        ui::window_theme(hwnd, &ui::theme());
                    }
                }
            }
        }
        if !self.mica {
            ui.painter().rect_filled(ui.max_rect(), 0.0, p.bg);
        }
        let rect = ui.max_rect().shrink2(Vec2::new(MARGIN, 0.0));
        let mut body = ui.new_child(UiBuilder::new().max_rect(rect));
        let ui = &mut body;
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        ui.add_space(20.0);
        ui.label(RichText::new("Snipping Lite").font(semibold(20.0)));
        ui.add_space(4.0);
        ui.label(
            RichText::new(tr("Capture screenshots with Alt + Shift + S"))
                .size(12.0)
                .color(p.secondary),
        );
        ui.add_space(12.0);
        if accent_button(ui, tr("New snip"), &p).clicked() {
            // Let the tray process bring the capture overlay to the foreground.
            // SAFETY: the call has no pointer arguments.
            unsafe { AllowSetForegroundWindow(ASFW_ANY) };
            post(WM_COMMAND, NEW);
        }
        ui.add_space(16.0);
        let before = self.settings();
        let warning = self
            .notice
            .is_some_and(|t| t.elapsed() < Duration::from_secs(4));
        let mut folder_action = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                let width = ui.available_width() - 14.0;
                section(ui, &p, tr("Snipping"), false);
                card(ui, &p, width, |ui| {
                    let note = if warning {
                        (
                            tr("Choose at least one destination for screenshots."),
                            p.warning,
                        )
                    } else {
                        (
                            tr("Screenshots are copied to the clipboard automatically"),
                            p.secondary,
                        )
                    };
                    let title = tr("Automatically copy screenshots");
                    row(ui, &p, title, Some(note), |ui| {
                        switch(ui, &mut self.copy, title, &p);
                    });
                    divider(ui, &p);
                    let title = tr("Automatically save screenshots");
                    row(ui, &p, title, None, |ui| {
                        switch(ui, &mut self.save, title, &p);
                    });
                    if self.save {
                        divider(ui, &p);
                        let current = config::screenshot_folder(&self.settings());
                        folder_action = folder_row(ui, &p, tr("Screenshot folder"), &current)
                            .map(|action| (false, action));
                    }
                    divider(ui, &p);
                    let title = tr("Choose a file name when saving");
                    row(ui, &p, title, None, |ui| {
                        switch(ui, &mut self.ask, title, &p);
                    });
                    divider(ui, &p);
                    row(ui, &p, tr("Capture delay"), None, |ui| {
                        let text = match self.delay {
                            0 => tr("None").to_owned(),
                            n => tr("{n} seconds").replace("{n}", &n.to_string()),
                        };
                        egui::ComboBox::from_id_salt("delay")
                            .selected_text(text)
                            .width(124.0)
                            .show_ui(ui, |ui| {
                                for d in DELAYS {
                                    let label = match d {
                                        0 => tr("None").to_owned(),
                                        n => tr("{n} seconds").replace("{n}", &n.to_string()),
                                    };
                                    ui.selectable_value(&mut self.delay, d, label);
                                }
                            });
                    });
                    divider(ui, &p);
                    let title = tr("Dim and freeze the screen while snipping");
                    let note = (
                        tr("Turn off to keep videos and apps running while you select"),
                        p.secondary,
                    );
                    row(ui, &p, title, Some(note), |ui| {
                        switch(ui, &mut self.freeze, title, &p);
                    });
                });
                section(ui, &p, tr("Screen recording"), true);
                card(ui, &p, width, |ui| {
                    let title = tr("Countdown before recording");
                    row(ui, &p, title, None, |ui| {
                        switch(ui, &mut self.countdown, title, &p);
                    });
                    divider(ui, &p);
                    let current = config::video_folder(&self.settings());
                    if let Some(action) = folder_row(ui, &p, tr("Recording folder"), &current) {
                        folder_action = Some((true, action));
                    }
                });
                section(ui, &p, tr("History"), true);
                card(ui, &p, width, |ui| {
                    let title = tr("Keep a history of screenshots");
                    row(ui, &p, title, None, |ui| {
                        switch(ui, &mut self.history, title, &p);
                    });
                    divider(ui, &p);
                    row(ui, &p, tr("History size"), None, |ui| {
                        egui::ComboBox::from_id_salt("history_limit")
                            .selected_text(
                                tr("{n} screenshots")
                                    .replace("{n}", &self.history_limit.to_string()),
                            )
                            .width(124.0)
                            .show_ui(ui, |ui| {
                                for n in HISTORY_LIMITS {
                                    ui.selectable_value(
                                        &mut self.history_limit,
                                        n,
                                        tr("{n} screenshots").replace("{n}", &n.to_string()),
                                    );
                                }
                            });
                    });
                    divider(ui, &p);
                    row(ui, &p, tr("Open history"), None, |ui| {
                        if outline_button(ui, tr("Open"), &p).clicked() {
                            // SAFETY: the call has no pointer arguments.
                            unsafe { AllowSetForegroundWindow(ASFW_ANY) };
                            post(WM_COMMAND, TRAY_HISTORY);
                        }
                    });
                });
                section(ui, &p, tr("Shortcuts"), true);
                card(ui, &p, width, |ui| {
                    let note = (!self.hotkey_ok).then_some((
                        tr("Shortcut unavailable: another application is using it."),
                        p.warning,
                    ));
                    row(
                        ui,
                        &p,
                        tr("Start a snip with Alt + Shift + S"),
                        note,
                        |_| {},
                    );
                });
                ui.add_space(20.0);
            });
        if let Some((video, action)) = folder_action {
            let settings = self.settings();
            let current = if video {
                config::video_folder(&settings)
            } else {
                config::screenshot_folder(&settings)
            };
            match action {
                FolderAction::Open => imaging::open_folder(&current),
                FolderAction::Change => {
                    // SAFETY: the title is a valid UTF-16 string; the handle only owns the dialog.
                    let owner = unsafe { FindWindowW(null(), w(SETTINGS_TITLE).as_ptr()) };
                    if let Some(path) = imaging::choose_folder(owner, &current) {
                        let text = path.to_string_lossy().into_owned();
                        if video {
                            self.video_folder = text;
                        } else {
                            self.screenshot_folder = text;
                        }
                    }
                }
            }
        }
        if !self.copy && !self.save {
            // At least one destination must stay enabled.
            (self.copy, self.save) = (before.copy, before.save);
            if !before.copy && !before.save {
                self.copy = true;
            }
            self.notice = Some(Instant::now());
        }
        if self.settings() != before {
            self.commit();
        }
        if warning {
            ctx.request_repaint_after(Duration::from_secs(4));
        }
    }
}

const ROW_TALL: f32 = 64.0;

/// A section heading above a card.
fn section(ui: &mut egui::Ui, p: &Palette, title: &str, gap: bool) {
    ui.add_space(if gap { 24.0 } else { 4.0 });
    ui.label(RichText::new(title).font(semibold(14.0)).color(p.text));
    ui.add_space(8.0);
}

/// A rounded card that holds the rows of one section.
fn card(ui: &mut egui::Ui, p: &Palette, width: f32, content: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(1.0, p.stroke))
        .corner_radius(8.0)
        .show(ui, |ui| {
            ui.set_width(width - 2.0);
            content(ui);
        });
}

/// A settings row: the title (and an optional note below it) on the left and
/// the control on the right.
fn row(
    ui: &mut egui::Ui,
    p: &Palette,
    title: &str,
    note: Option<(&str, Color32)>,
    content: impl FnOnce(&mut egui::Ui),
) {
    let height = if note.is_some() { ROW_TALL } else { ROW };
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    let offset = if note.is_some() { -10.0 } else { 0.0 };
    ui.painter().text(
        rect.left_center() + Vec2::new(16.0, offset),
        Align2::LEFT_CENTER,
        title,
        FontId::new(14.0, FontFamily::Proportional),
        p.text,
    );
    if let Some((text, color)) = note {
        ui.painter().text(
            rect.left_center() + Vec2::new(16.0, 11.0),
            Align2::LEFT_CENTER,
            text,
            FontId::new(12.0, FontFamily::Proportional),
            color,
        );
    }
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(rect.shrink2(Vec2::new(16.0, 0.0)))
            .layout(Layout::right_to_left(Align::Center)),
    );
    content(&mut child);
}

enum FolderAction {
    Change,
    Open,
}

/// A folder row: the title, the path with a Change link, and an Open folder button.
fn folder_row(ui: &mut egui::Ui, p: &Palette, title: &str, path: &Path) -> Option<FolderAction> {
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_TALL), Sense::hover());
    let left = 32.0;
    ui.painter().text(
        rect.left_center() + Vec2::new(left, -10.0),
        Align2::LEFT_CENTER,
        title,
        FontId::new(14.0, FontFamily::Proportional),
        p.text,
    );
    let font = FontId::new(12.0, FontFamily::Proportional);
    // Long paths keep their end, where the folder name is.
    let room = rect.width() - left - 190.0;
    let mut shown = path.display().to_string();
    while shown.chars().count() > 8
        && ui
            .painter()
            .layout_no_wrap(shown.clone(), font.clone(), p.secondary)
            .size()
            .x
            > room
    {
        shown = format!("…{}", shown.chars().skip(2).collect::<String>());
    }
    let text_rect = ui.painter().text(
        rect.left_center() + Vec2::new(left, 11.0),
        Align2::LEFT_CENTER,
        shown,
        font.clone(),
        p.secondary,
    );
    let link = ui.painter().text(
        pos2(text_rect.right() + 8.0, text_rect.center().y),
        Align2::LEFT_CENTER,
        tr("Change"),
        font,
        p.accent,
    );
    let mut action = None;
    let response = ui.interact(
        link.expand(4.0),
        ui.make_persistent_id((title, "change")),
        Sense::click(),
    );
    if response.hovered() {
        ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::PointingHand);
    }
    if response.clicked() {
        action = Some(FolderAction::Change);
    }
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(rect.shrink2(Vec2::new(16.0, 0.0)))
            .layout(Layout::right_to_left(Align::Center)),
    );
    if outline_button(&mut child, tr("Open folder"), p).clicked() {
        action = Some(FolderAction::Open);
    }
    action
}

/// A bordered text button, as the Windows 11 settings use for secondary actions.
fn outline_button(ui: &mut egui::Ui, label: &str, p: &Palette) -> egui::Response {
    let font = FontId::new(13.0, FontFamily::Proportional);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font.clone(), p.text);
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(galley.size().x + 24.0, 30.0), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), label));
    let fill = if response.is_pointer_button_down_on() {
        p.stroke
    } else if response.hovered() {
        p.field
    } else {
        Color32::TRANSPARENT
    };
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(4), fill);
    painter.rect_stroke(
        rect,
        CornerRadius::same(4),
        Stroke::new(1.0, p.stroke),
        StrokeKind::Inside,
    );
    painter.text(rect.center(), Align2::CENTER_CENTER, label, font, p.text);
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

/// A toggle switch with its On or Off label to the left, as in the Windows settings.
fn switch(ui: &mut egui::Ui, on: &mut bool, label: &str, p: &Palette) {
    toggle(ui, on, label, p);
    ui.add_space(10.0);
    let text = if *on { tr("On") } else { tr("Off") };
    ui.label(RichText::new(text).size(13.0).color(p.text));
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
