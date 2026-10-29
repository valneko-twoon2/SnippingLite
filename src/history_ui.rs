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
    UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
};

use crate::i18n::tr;
use crate::{
    annotate::{self, Mark},
    config,
    history::{self, Entry},
    imaging,
    settings_ui::{apply_theme, install_fonts, install_visuals, semibold, Palette},
    ui, w, HISTORY_TITLE, HISTORY_WINDOW,
};

mod detail;
mod gallery;
mod widgets;
use widgets::*;

const CELL_MIN: f32 = 200.0;
const GAP: f32 = 12.0;
const MARGIN: f32 = 24.0;

pub fn run(latest: bool) {
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
            apply_theme(&cc.egui_ctx, config::load_settings().theme);
            install_visuals(&cc.egui_ctx, mica);
            Ok(Box::new(HistoryApp::new(mica, latest)))
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
    /// The crop area being chosen, in image pixels (start and end corner).
    crop: Option<((f32, f32), (f32, f32))>,
    /// The image was cropped and the change is not stored yet.
    cropped: bool,
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
    Crop,
    Picker,
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
            Range::All => tr("All"),
            Range::Day => tr("Last 24 hours"),
            Range::Week => tr("Last 7 days"),
            Range::Month => tr("Last 30 days"),
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
    framed: Option<bool>,
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
    /// Indexes into `annotate::COLORS` and the radius tables.
    color: usize,
    thickness: usize,
    /// Open the newest screenshot on the first frame.
    open_latest: bool,
    notice: Option<(Instant, &'static str)>,
    /// Text recognition in progress for the opened image.
    ocr: Option<std::sync::mpsc::Receiver<Result<String, crate::ocr::Error>>>,
}

impl HistoryApp {
    fn new(mica: bool, latest: bool) -> HistoryApp {
        let (color, thickness) = config::load_pen();
        HistoryApp {
            open_latest: latest,
            mica,
            framed: None,
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
            color,
            thickness,
            notice: None,
            ocr: None,
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
            self.notice = Some((Instant::now(), tr("Could not open the image.")));
            return;
        };
        let texture = texture_from(ctx, &entry.image.to_string_lossy(), width, height, &pixels);
        self.ocr = None;
        self.opened = Some(Opened {
            entry,
            texture: Some(texture),
            size: (width, height),
            pixels,
            marks: Vec::new(),
            crop: None,
            cropped: false,
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
    // SAFETY: this only queries the tagged document window.
    unsafe { ui::find_app_window(HISTORY_WINDOW) }
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
        if self.framed != Some(dark) {
            self.framed = Some(dark);
            // SAFETY: the handle is only passed to DWM attribute calls.
            unsafe {
                let hwnd = ui::identify_app_window(HISTORY_WINDOW);
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
        if std::mem::take(&mut self.open_latest) {
            if let Some(entry) = self.entries.first().cloned() {
                self.open(&ctx, entry);
            }
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
