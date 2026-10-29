//! The player window: the video above, the transport bar below.

use super::{
    controls::*,
    engine::{Engine, Status},
};
use crate::{
    config,
    i18n::tr,
    settings_ui::{apply_theme, install_fonts, install_visuals, Palette},
    ui, PLAYER_TITLE, PLAYER_WINDOW,
};
use eframe::egui::{
    self, pos2, Align, Align2, Color32, FontFamily, FontId, Key, Layout, Rect, Sense,
    TextureHandle, TextureOptions, Theme, UiBuilder, Vec2,
};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

const BAR: f32 = 72.0;
/// How far the arrow keys seek.
const SKIP: Duration = Duration::from_secs(5);
const VOLUME_WIDTH: f32 = 110.0;

pub fn run(path: PathBuf) {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(PLAYER_TITLE)
            .with_inner_size([960.0, 640.0])
            .with_min_inner_size([480.0, 320.0])
            .with_transparent(ui::supports_mica())
            .with_icon(egui::IconData {
                rgba: ui::icon_rgba(64),
                width: 64,
                height: 64,
            }),
        ..Default::default()
    };
    let _ = eframe::run_native(
        PLAYER_TITLE,
        options,
        Box::new(move |cc| {
            if !install_fonts(&cc.egui_ctx) {
                return Err("No usable system font was found.".into());
            }
            let mica = ui::supports_mica();
            apply_theme(&cc.egui_ctx, config::load_settings().theme);
            install_visuals(&cc.egui_ctx, mica);
            Ok(Box::new(PlayerApp::new(mica, &path)))
        }),
    );
}

pub(super) struct PlayerApp {
    mica: bool,
    framed: Option<bool>,
    /// `None` when the file could not be opened.
    engine: Option<Engine>,
    frame: Option<TextureHandle>,
    /// The volume to return to when unmuting.
    unmuted: f64,
}

impl PlayerApp {
    pub(super) fn new(mica: bool, path: &Path) -> Self {
        let engine = Engine::open(path).ok();
        Self {
            mica,
            framed: None,
            engine,
            frame: None,
            unmuted: 1.0,
        }
    }

    pub(super) fn failed(&mut self) -> bool {
        self.engine
            .as_mut()
            .is_none_or(|engine| engine.status() == Status::Failed)
    }

    #[cfg(test)]
    pub(super) fn has_frame(&self) -> bool {
        self.frame.is_some()
    }

    pub(super) fn playing(&self) -> bool {
        self.engine.as_ref().is_some_and(Engine::playing)
    }

    pub(super) fn position(&self) -> Duration {
        self.engine
            .as_ref()
            .map_or(Duration::ZERO, Engine::position)
    }

    pub(super) fn volume(&self) -> f64 {
        self.engine.as_ref().map_or(0.0, Engine::volume)
    }

    pub(super) fn toggle(&self) {
        let Some(engine) = &self.engine else {
            return;
        };
        if engine.playing() {
            engine.pause();
        } else {
            // Playing from the end starts over.
            if engine.ended() {
                engine.seek(Duration::ZERO);
            }
            engine.play();
        }
    }

    pub(super) fn seek_to(&self, at: Duration) {
        if let Some(engine) = &self.engine {
            engine.seek(at);
        }
    }

    pub(super) fn skip(&self, forward: bool) {
        let at = self.position();
        self.seek_to(if forward {
            at + SKIP
        } else {
            at.saturating_sub(SKIP)
        });
    }

    pub(super) fn set_volume(&mut self, volume: f64) {
        if let Some(engine) = &self.engine {
            engine.set_volume(volume);
        }
    }

    pub(super) fn mute(&mut self) {
        let volume = self.volume();
        if volume > 0.0 {
            self.unmuted = volume;
            self.set_volume(0.0);
        } else {
            self.set_volume(self.unmuted);
        }
    }

    fn pull_frame(&mut self, ctx: &egui::Context) {
        let Some(engine) = &mut self.engine else {
            return;
        };
        if let Ok(Some(picture)) = engine.frame() {
            // The decoder's BGRA becomes egui's RGBA.
            let mut rgba = picture.pixels;
            for pixel in rgba.as_chunks_mut::<4>().0 {
                pixel.swap(0, 2);
                pixel[3] = 255;
            }
            let image =
                egui::ColorImage::from_rgba_unmultiplied([picture.width, picture.height], &rgba);
            match &mut self.frame {
                Some(texture) => texture.set(image, TextureOptions::LINEAR),
                None => {
                    self.frame = Some(ctx.load_texture("frame", image, TextureOptions::LINEAR));
                }
            }
        }
    }

    fn video(&mut self, ui: &mut egui::Ui, p: &Palette, area: Rect) {
        ui.painter().rect_filled(area, 0.0, Color32::BLACK);
        let failed = self.failed();
        let Some(texture) = &self.frame else {
            if failed {
                ui.painter().text(
                    area.center(),
                    Align2::CENTER_CENTER,
                    tr("This video cannot be played."),
                    FontId::new(14.0, FontFamily::Proportional),
                    p.secondary,
                );
            }
            return;
        };
        let size = texture.size_vec2();
        let scale = (area.width() / size.x).min(area.height() / size.y);
        let shown = Rect::from_center_size(area.center(), size * scale);
        ui.painter().image(
            texture.id(),
            shown,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        if ui
            .interact(area, ui.make_persistent_id("video"), Sense::click())
            .clicked()
        {
            self.toggle();
        }
    }

    fn bar(&mut self, ui: &mut egui::Ui, p: &Palette, area: Rect) {
        // Opaque even over the Mica backdrop, so the video edge stays clean.
        let solid = Palette::new(ui.ctx().theme() == Theme::Dark, false);
        ui.painter().rect_filled(area, 0.0, solid.card);
        let duration = self
            .engine
            .as_ref()
            .map_or(Duration::ZERO, Engine::duration);
        let position = self.position().min(duration);
        let mut child = ui.new_child(
            UiBuilder::new()
                .max_rect(area.shrink2(Vec2::new(16.0, 0.0)))
                .layout(Layout::left_to_right(Align::Center)),
        );
        let ui = &mut child;
        let label = if self.playing() {
            tr("Pause")
        } else {
            tr("Play")
        };
        if play_button(ui, p, self.playing(), label) {
            self.toggle();
        }
        let text = format!("{} / {}", clock(position), clock(duration));
        ui.label(egui::RichText::new(text).color(p.text));
        let seek_width = (ui.available_width() - VOLUME_WIDTH - 32.0 - 24.0).max(60.0);
        let fraction = if duration.is_zero() {
            0.0
        } else {
            (position.as_secs_f32() / duration.as_secs_f32()).clamp(0.0, 1.0)
        };
        if let Some(chosen) = slider(ui, p, seek_width, fraction, tr("Seek")) {
            self.seek_to(duration.mul_f32(chosen));
        }
        let muted = self.volume() <= 0.0;
        if speaker_button(ui, p, muted, tr("Volume")) {
            self.mute();
        }
        if let Some(chosen) = slider(ui, p, VOLUME_WIDTH, self.volume() as f32, tr("Volume")) {
            self.set_volume(f64::from(chosen));
        }
    }
}

impl eframe::App for PlayerApp {
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
                let hwnd = ui::identify_app_window(PLAYER_WINDOW);
                if !hwnd.is_null() {
                    ui::disable_transitions(hwnd);
                    ui::window_theme(hwnd, &ui::theme());
                }
            }
        }
        if ctx.input(|i| i.key_pressed(Key::Space)) {
            self.toggle();
        }
        if ctx.input(|i| i.key_pressed(Key::ArrowRight)) {
            self.skip(true);
        }
        if ctx.input(|i| i.key_pressed(Key::ArrowLeft)) {
            self.skip(false);
        }
        self.pull_frame(&ctx);
        // Frames arrive from the player's own thread; poll for them.
        ctx.request_repaint_after(Duration::from_millis(if self.playing() { 16 } else { 100 }));
        let all = ui.max_rect();
        let bar = Rect::from_min_max(pos2(all.left(), all.bottom() - BAR), all.max);
        let video = Rect::from_min_max(all.min, pos2(all.right(), bar.top()));
        self.video(ui, &p, video);
        self.bar(ui, &p, bar);
    }
}
