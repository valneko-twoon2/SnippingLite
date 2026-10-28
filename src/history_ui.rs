//! The history window. It runs in its own process (`--history`), like the
//! settings window, so its memory is released when it closes.

use eframe::egui::{
    self, pos2, Align2, Color32, CornerRadius, FontFamily, FontId, Key, Rect, RichText, Sense,
    Stroke, StrokeKind, TextureHandle, TextureOptions, Theme, Vec2,
};
use std::{
    collections::HashMap,
    path::Path,
    ptr::null,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::HWND,
    UI::{
        Shell::ShellExecuteW, WindowsAndMessaging::FindWindowW, WindowsAndMessaging::SW_SHOWNORMAL,
    },
};

use crate::{
    annotate::{self, Mark},
    config,
    history::{self, Entry},
    imaging,
    settings_ui::{install_fonts, install_visuals, semibold, Palette},
    ui, w, HISTORY_TITLE,
};

const CELL_MIN: f32 = 200.0;
const GAP: f32 = 12.0;
const MARGIN: f32 = 24.0;

pub fn run() {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(HISTORY_TITLE)
            .with_inner_size([960.0, 640.0])
            .with_min_inner_size([480.0, 360.0])
            .with_transparent(ui::supports_mica())
            .with_icon(egui::IconData {
                rgba: ui::icon_rgba(64),
                width: 64,
                height: 64,
            }),
        ..Default::default()
    };
    let _ = eframe::run_native(
        HISTORY_TITLE,
        options,
        Box::new(|cc| {
            if !install_fonts(&cc.egui_ctx) {
                return Err("No usable system font was found.".into());
            }
            let mica = ui::supports_mica();
            install_visuals(&cc.egui_ctx, mica);
            Ok(Box::new(HistoryApp::new(mica)))
        }),
    );
}

struct Opened {
    entry: Entry,
    texture: Option<TextureHandle>,
    size: (i32, i32),
    pixels: Vec<u8>,
    /// Strokes drawn since the image was opened or last saved.
    marks: Vec<Mark>,
}

impl Opened {
    /// The image with all marks drawn into it.
    fn flattened(&self) -> Vec<u8> {
        let mut pixels = self.pixels.clone();
        annotate::draw(&mut pixels, self.size.0, self.size.1, &self.marks);
        pixels
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Tool {
    None,
    Pen,
    Highlighter,
}

/// How far back the gallery reaches.
#[derive(Clone, Copy, PartialEq)]
enum Range {
    All,
    Day,
    Week,
    Month,
}

impl Range {
    const ALL: [Range; 4] = [Range::All, Range::Day, Range::Week, Range::Month];

    fn label(self) -> &'static str {
        match self {
            Range::All => "All",
            Range::Day => "Last 24 hours",
            Range::Week => "Last 7 days",
            Range::Month => "Last 30 days",
        }
    }

    fn milliseconds(self) -> Option<u128> {
        const DAY: u128 = 24 * 60 * 60 * 1000;
        match self {
            Range::All => None,
            Range::Day => Some(DAY),
            Range::Week => Some(7 * DAY),
            Range::Month => Some(30 * DAY),
        }
    }
}

struct HistoryApp {
    mica: bool,
    framed: bool,
    entries: Vec<Entry>,
    refreshed: Instant,
    thumbnails: HashMap<u128, Option<TextureHandle>>,
    opened: Option<Opened>,
    confirm_clear: bool,
    recording: bool,
    limit: usize,
    range: Range,
    query: String,
    tool: Tool,
    notice: Option<(Instant, &'static str)>,
}

impl HistoryApp {
    fn new(mica: bool) -> HistoryApp {
        HistoryApp {
            mica,
            framed: false,
            entries: history::list(),
            refreshed: Instant::now(),
            thumbnails: HashMap::new(),
            opened: None,
            confirm_clear: false,
            recording: config::load_settings().history,
            limit: config::load_settings().history_limit,
            range: Range::All,
            query: String::new(),
            tool: Tool::None,
            notice: None,
        }
    }

    /// The entries that match the range and the search text.
    fn visible(&self) -> Vec<Entry> {
        let now = imaging::timestamp().unwrap_or(0);
        let query = self.query.trim().replace(['-', '.'], "/");
        self.entries
            .iter()
            .filter(|e| {
                self.range
                    .milliseconds()
                    .is_none_or(|span| e.id + span >= now)
                    && (query.is_empty()
                        || format!("{} {}", date_label(e.id), time_label(e.id)).contains(&query))
            })
            .cloned()
            .collect()
    }

    fn refresh(&mut self) {
        let entries = history::list();
        if entries != self.entries {
            self.entries = entries;
            let alive: Vec<u128> = self.entries.iter().map(|e| e.id).collect();
            self.thumbnails.retain(|id, _| alive.contains(id));
        }
        let settings = config::load_settings();
        self.recording = settings.history;
        self.limit = settings.history_limit;
        self.refreshed = Instant::now();
    }

    fn thumbnail(&mut self, ctx: &egui::Context, entry: &Entry) -> Option<TextureHandle> {
        self.thumbnails
            .entry(entry.id)
            .or_insert_with(|| load_texture(ctx, &entry.thumbnail).map(|(t, _, _)| t))
            .clone()
    }

    fn open(&mut self, ctx: &egui::Context, entry: Entry) {
        let Some((width, height, pixels)) = imaging::decode(&entry.image) else {
            self.notice = Some((Instant::now(), "Could not open the image."));
            return;
        };
        let texture = texture_from(ctx, &entry.image.to_string_lossy(), width, height, &pixels);
        self.opened = Some(Opened {
            entry,
            texture: Some(texture),
            size: (width, height),
            pixels,
            marks: Vec::new(),
        });
    }

    /// Moves the opened image by `step` entries (positive is older).
    fn step(&mut self, ctx: &egui::Context, step: isize) {
        let Some(opened) = &self.opened else {
            return;
        };
        let entries = self.visible();
        let Some(index) = entries.iter().position(|e| e.id == opened.entry.id) else {
            return;
        };
        let target = index as isize + step;
        if target >= 0 {
            if let Some(entry) = entries.get(target as usize).cloned() {
                self.open(ctx, entry);
            }
        }
    }
}

fn texture_from(
    ctx: &egui::Context,
    name: &str,
    width: i32,
    height: i32,
    bgra: &[u8],
) -> TextureHandle {
    let mut rgba = bgra.to_vec();
    for pixel in rgba.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    let image = egui::ColorImage::from_rgba_unmultiplied([width as usize, height as usize], &rgba);
    ctx.load_texture(name, image, TextureOptions::LINEAR)
}

fn load_texture(ctx: &egui::Context, path: &Path) -> Option<(TextureHandle, i32, i32)> {
    let (width, height, pixels) = imaging::decode(path)?;
    Some((
        texture_from(ctx, &path.to_string_lossy(), width, height, &pixels),
        width,
        height,
    ))
}

fn window() -> HWND {
    // SAFETY: the title is a valid, nul-terminated UTF-16 string.
    unsafe { FindWindowW(null(), w(HISTORY_TITLE).as_ptr()) }
}

fn show_in_folder(path: &Path) {
    let parameters = w(&format!("/select,\"{}\"", path.display()));
    // SAFETY: all strings are valid, nul-terminated UTF-16.
    unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            w("open").as_ptr(),
            w("explorer.exe").as_ptr(),
            parameters.as_ptr(),
            null(),
            SW_SHOWNORMAL,
        );
    }
}

fn date_label(id: u128) -> String {
    let (year, month, day, _, _) = history::local_time(id);
    format!("{year}/{month:02}/{day:02}")
}

fn time_label(id: u128) -> String {
    let (_, _, _, hour, minute) = history::local_time(id);
    format!("{hour:02}:{minute:02}")
}

impl eframe::App for HistoryApp {
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
            // SAFETY: the handle is only passed to DWM attribute calls.
            unsafe {
                let hwnd = window();
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
        if self.refreshed.elapsed() > Duration::from_secs(2) {
            self.refresh();
        }
        ctx.request_repaint_after(Duration::from_secs(2));

        let rect = ui.max_rect().shrink2(Vec2::new(MARGIN, 0.0));
        let mut body = ui.new_child(egui::UiBuilder::new().max_rect(rect));
        if self.opened.is_some() {
            self.detail(&ctx, &mut body, &p);
        } else {
            self.gallery(&ctx, &mut body, &p);
        }
    }
}

impl HistoryApp {
    fn header(&mut self, ui: &mut egui::Ui, p: &Palette) {
        ui.add_space(20.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("History").font(semibold(20.0)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if self.confirm_clear {
                    if text_button(ui, "Cancel", p, false).clicked() {
                        self.confirm_clear = false;
                    }
                    if text_button(ui, "Delete all", p, true).clicked() {
                        history::clear();
                        self.confirm_clear = false;
                        self.refresh();
                    }
                    ui.label(
                        RichText::new("Delete every screenshot in the history?")
                            .size(12.0)
                            .color(p.warning),
                    );
                } else if !self.entries.is_empty() && text_button(ui, "Clear", p, false).clicked() {
                    self.confirm_clear = true;
                }
            });
        });
        ui.add_space(4.0);
        let shown = self.visible().len();
        let text = match (self.entries.len(), shown) {
            (0, _) => "No screenshots yet".to_owned(),
            (total, shown) if shown == total => format!(
                "{total} screenshots, newest first (the oldest beyond {} are removed)",
                self.limit
            ),
            (total, shown) => format!("{shown} of {total} screenshots match"),
        };
        ui.label(RichText::new(text).size(12.0).color(p.secondary));
        if !self.recording {
            ui.label(
                RichText::new("History recording is turned off in Settings.")
                    .size(12.0)
                    .color(p.warning),
            );
        }
        if !self.entries.is_empty() {
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                for range in Range::ALL {
                    if chip(ui, range.label(), p, self.range == range).clicked() {
                        self.range = range;
                    }
                    ui.add_space(6.0);
                }
                ui.add_space(8.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.query)
                        .hint_text("Search by date, e.g. 2026/10/28")
                        .desired_width(220.0),
                );
            });
        }
        ui.add_space(12.0);
    }

    fn gallery(&mut self, ctx: &egui::Context, ui: &mut egui::Ui, p: &Palette) {
        self.header(ui, p);
        if self.entries.is_empty() {
            ui.add_space(40.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new("Captures appear here after you take them.").color(p.secondary),
                );
            });
            return;
        }
        let entries = self.visible();
        if entries.is_empty() {
            ui.add_space(40.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("No screenshots match.").color(p.secondary));
            });
            return;
        }
        let mut open = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let available = ui.available_width();
                let columns = (((available + GAP) / (CELL_MIN + GAP)).floor() as usize).max(1);
                let width = (available - GAP * (columns - 1) as f32) / columns as f32;
                let height = width * 0.62;
                let mut day = String::new();
                let mut index = 0;
                while index < entries.len() {
                    let label = date_label(entries[index].id);
                    if label != day {
                        if !day.is_empty() {
                            ui.add_space(8.0);
                        }
                        ui.label(RichText::new(&label).font(semibold(14.0)));
                        ui.add_space(8.0);
                        day = label;
                    }
                    let mut row = Vec::new();
                    while row.len() < columns
                        && index < entries.len()
                        && date_label(entries[index].id) == day
                    {
                        row.push(index);
                        index += 1;
                    }
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(GAP, 0.0);
                        for &i in &row {
                            let entry = &entries[i];
                            let response = self.cell(ctx, ui, entry, Vec2::new(width, height), p);
                            if response.clicked() {
                                open = Some(entry.clone());
                            }
                        }
                    });
                    ui.add_space(GAP);
                }
            });
        if let Some(entry) = open {
            self.open(ctx, entry);
        }
    }

    fn cell(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        entry: &Entry,
        size: Vec2,
        p: &Palette,
    ) -> egui::Response {
        let (rect, response) = ui.allocate_exact_size(size, Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::Role::Button,
                ui.is_enabled(),
                format!(
                    "Screenshot {} {}",
                    date_label(entry.id),
                    time_label(entry.id)
                ),
            )
        });
        let painter = ui.painter();
        let radius = CornerRadius::same(8);
        painter.rect_filled(rect, radius, p.card);
        if ui.is_rect_visible(rect) {
            if let Some(texture) = self.thumbnail(ctx, entry) {
                let source = texture.size_vec2();
                let scale = (rect.width() / source.x).min(rect.height() / source.y);
                let shown = Rect::from_center_size(rect.center(), source * scale);
                ui.painter().image(
                    texture.id(),
                    shown,
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
        }
        let painter = ui.painter();
        let label = painter.layout_no_wrap(
            time_label(entry.id),
            FontId::new(12.0, FontFamily::Proportional),
            Color32::WHITE,
        );
        let tag = Rect::from_min_size(
            rect.left_bottom() + Vec2::new(8.0, -8.0 - label.size().y - 4.0),
            label.size() + Vec2::new(12.0, 4.0),
        );
        painter.rect_filled(tag, CornerRadius::same(4), Color32::from_black_alpha(170));
        painter.galley(tag.min + Vec2::new(6.0, 2.0), label, Color32::WHITE);
        let stroke = if response.hovered() || response.has_focus() {
            Stroke::new(2.0, p.accent)
        } else {
            Stroke::new(1.0, p.stroke)
        };
        painter.rect_stroke(rect, radius, stroke, StrokeKind::Inside);
        response
    }

    /// Handles pen input over the image and previews all marks.
    fn draw_marks(&mut self, ui: &mut egui::Ui, shown: Rect, scale: f32) {
        let tool = self.tool;
        let Some(opened) = self.opened.as_mut() else {
            return;
        };
        if tool != Tool::None {
            let response = ui.interact(
                shown,
                ui.make_persistent_id("canvas"),
                Sense::click_and_drag(),
            );
            if response.hovered() {
                ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::Crosshair);
            }
            let point =
                |pos: egui::Pos2| ((pos.x - shown.min.x) / scale, (pos.y - shown.min.y) / scale);
            if response.drag_started() {
                if let Some(pos) = response.interact_pointer_pos() {
                    opened.marks.push(Mark {
                        highlight: tool == Tool::Highlighter,
                        points: vec![point(pos)],
                    });
                }
            } else if response.dragged() {
                if let (Some(pos), Some(mark)) =
                    (response.interact_pointer_pos(), opened.marks.last_mut())
                {
                    mark.points.push(point(pos));
                }
            }
        }
        let painter = ui.painter().with_clip_rect(shown);
        for mark in &opened.marks {
            let (color, opacity, radius) = annotate::brush(mark.highlight);
            let color = Color32::from_rgba_unmultiplied(
                color[0],
                color[1],
                color[2],
                (opacity * 255.0) as u8,
            );
            let points: Vec<egui::Pos2> = mark
                .points
                .iter()
                .map(|&(x, y)| shown.min + Vec2::new(x, y) * scale)
                .collect();
            if let [only] = points[..] {
                painter.circle_filled(only, radius * scale, color);
            } else {
                painter.add(egui::Shape::line(
                    points,
                    Stroke::new(radius * 2.0 * scale, color),
                ));
            }
        }
    }

    /// Writes the marks into the stored image and refreshes its thumbnail.
    fn save_marks(&mut self, ctx: &egui::Context) {
        let Some(opened) = &mut self.opened else {
            return;
        };
        let pixels = opened.flattened();
        let (width, height) = opened.size;
        if history::replace(&opened.entry, &pixels, width, height).is_ok() {
            opened.texture = Some(texture_from(
                ctx,
                &opened.entry.image.to_string_lossy(),
                width,
                height,
                &pixels,
            ));
            opened.pixels = pixels;
            opened.marks.clear();
            self.thumbnails.remove(&opened.entry.id);
            self.notice = Some((Instant::now(), "Saved the changes."));
        } else {
            self.notice = Some((Instant::now(), "Could not save the changes."));
        }
    }

    fn detail(&mut self, ctx: &egui::Context, ui: &mut egui::Ui, p: &Palette) {
        let escape = ctx.input(|i| i.key_pressed(Key::Escape));
        let newer = ctx.input(|i| i.key_pressed(Key::ArrowLeft));
        let older = ctx.input(|i| i.key_pressed(Key::ArrowRight));
        if escape {
            self.opened = None;
            return;
        }
        if newer {
            self.step(ctx, -1);
        }
        if older {
            self.step(ctx, 1);
        }
        let Some(opened) = &self.opened else {
            return;
        };
        let (id, image, (width, height)) =
            (opened.entry.id, opened.entry.image.clone(), opened.size);
        let mut action = None;
        ui.add_space(20.0);
        ui.horizontal(|ui| {
            if text_button(ui, "Back", p, false).clicked() {
                action = Some(Action::Back);
            }
            ui.add_space(8.0);
            ui.label(
                RichText::new(format!("{}  {}", date_label(id), time_label(id)))
                    .font(semibold(14.0)),
            );
            ui.label(
                RichText::new(format!("   {width} x {height}"))
                    .size(12.0)
                    .color(p.secondary),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if text_button(ui, "Delete", p, true).clicked() {
                    action = Some(Action::Delete);
                }
                if text_button(ui, "Show in folder", p, false).clicked() {
                    action = Some(Action::Folder);
                }
                if text_button(ui, "Save as...", p, false).clicked() {
                    action = Some(Action::SaveAs);
                }
                if text_button(ui, "Copy", p, false).clicked() {
                    action = Some(Action::Copy);
                }
            });
        });
        ui.add_space(8.0);
        let mut undo = false;
        let mut persist = false;
        ui.horizontal(|ui| {
            for (tool, label) in [(Tool::Pen, "Pen"), (Tool::Highlighter, "Highlighter")] {
                if chip(ui, label, p, self.tool == tool).clicked() {
                    self.tool = if self.tool == tool { Tool::None } else { tool };
                }
                ui.add_space(6.0);
            }
            let marked = self.opened.as_ref().is_some_and(|o| !o.marks.is_empty());
            if marked {
                ui.add_space(8.0);
                undo = text_button(ui, "Undo", p, false).clicked();
                ui.add_space(6.0);
                persist = text_button(ui, "Save changes", p, false).clicked();
            }
        });
        ui.add_space(8.0);
        let area = ui.available_rect_before_wrap();
        let room = Rect::from_min_max(area.min, pos2(area.max.x, area.max.y - 28.0));
        ui.painter()
            .rect_filled(room, CornerRadius::same(8), p.card);
        let mut canvas = None;
        if let Some(texture) = self.opened.as_ref().and_then(|o| o.texture.as_ref()) {
            let source = texture.size_vec2();
            let inner = room.shrink(8.0);
            let scale = (inner.width() / source.x)
                .min(inner.height() / source.y)
                .min(1.0);
            let shown = Rect::from_center_size(inner.center(), source * scale);
            ui.painter().image(
                texture.id(),
                shown,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
            canvas = Some((shown, scale));
        }
        if let Some((shown, scale)) = canvas {
            self.draw_marks(ui, shown, scale);
        }
        if undo {
            if let Some(opened) = &mut self.opened {
                opened.marks.pop();
            }
        }
        if persist {
            self.save_marks(ctx);
        }
        let hint = match self.notice {
            Some((at, text)) if at.elapsed() < Duration::from_secs(4) => (text, p.warning),
            _ => (
                "Left and right arrow keys browse, Esc goes back.",
                p.secondary,
            ),
        };
        ui.painter().text(
            pos2(room.left(), room.bottom() + 6.0),
            Align2::LEFT_TOP,
            hint.0,
            FontId::new(12.0, FontFamily::Proportional),
            hint.1,
        );
        match action {
            Some(Action::Back) => self.opened = None,
            Some(Action::Delete) => {
                if let Some(opened) = self.opened.take() {
                    history::remove_entry(&opened.entry);
                    self.refresh();
                }
            }
            Some(Action::Folder) => show_in_folder(&image),
            Some(Action::Copy) => {
                if let Some(opened) = &self.opened {
                    let ok = imaging::copy_to_clipboard(
                        window(),
                        &opened.flattened(),
                        opened.size.0,
                        opened.size.1,
                    );
                    self.notice = Some((
                        Instant::now(),
                        if ok {
                            "Copied to the clipboard."
                        } else {
                            "Could not copy the image."
                        },
                    ));
                }
            }
            Some(Action::SaveAs) => {
                if let (Some(opened), Ok(folder)) = (&self.opened, imaging::screenshot_folder()) {
                    let name = format!("Snipping Lite {id}.png");
                    if let Some(path) = imaging::choose_path(window(), &folder, &name, true) {
                        let saved = imaging::encode(
                            &path,
                            &opened.flattened(),
                            opened.size.0,
                            opened.size.1,
                        );
                        self.notice = Some((
                            Instant::now(),
                            if saved.is_ok() {
                                "Saved the image."
                            } else {
                                "Could not save the image."
                            },
                        ));
                    }
                }
            }
            None => {}
        }
    }
}

enum Action {
    Back,
    Delete,
    Folder,
    Copy,
    SaveAs,
}

/// A toggle button that is filled with the accent color while selected.
fn chip(ui: &mut egui::Ui, label: &str, p: &Palette, selected: bool) -> egui::Response {
    let font = FontId::new(13.0, FontFamily::Proportional);
    let color = if selected { p.on_accent } else { p.text };
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font.clone(), color);
    let size = Vec2::new(galley.size().x + 24.0, 30.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::Button, ui.is_enabled(), selected, label)
    });
    let fill = if selected {
        p.accent
    } else if response.hovered() {
        p.card
    } else {
        Color32::TRANSPARENT
    };
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(15), fill);
    if !selected {
        painter.rect_stroke(
            rect,
            CornerRadius::same(15),
            Stroke::new(1.0, p.stroke),
            StrokeKind::Inside,
        );
    }
    painter.text(rect.center(), Align2::CENTER_CENTER, label, font, color);
    if response.has_focus() {
        painter.rect_stroke(
            rect.expand(2.0),
            CornerRadius::same(17),
            Stroke::new(2.0, p.text),
            StrokeKind::Outside,
        );
    }
    response
}

/// A rounded text button; `danger` colors it as a destructive action.
fn text_button(ui: &mut egui::Ui, label: &str, p: &Palette, danger: bool) -> egui::Response {
    let font = FontId::new(13.0, FontFamily::Proportional);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font.clone(), p.text);
    let size = Vec2::new(galley.size().x + 24.0, 30.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), label));
    let fill = if response.is_pointer_button_down_on() {
        p.stroke
    } else if response.hovered() {
        p.card
    } else {
        Color32::TRANSPARENT
    };
    let color = if danger { p.warning } else { p.text };
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(4), fill);
    painter.rect_stroke(
        rect,
        CornerRadius::same(4),
        Stroke::new(1.0, p.stroke),
        StrokeKind::Inside,
    );
    painter.text(rect.center(), Align2::CENTER_CENTER, label, font, color);
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
