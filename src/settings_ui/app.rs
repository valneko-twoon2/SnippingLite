//! The settings window state and layout.

use super::*;

mod sections;

pub(super) struct SettingsApp {
    copy: bool,
    save: bool,
    ask: bool,
    delay: u32,
    history: bool,
    history_limit: usize,
    countdown: bool,
    skip_title: bool,
    record_cursor: bool,
    record_fps: u32,
    freeze: bool,
    notify: bool,
    keep_toolbar: bool,
    outline: bool,
    outline_color: usize,
    outline_width: usize,
    mic: bool,
    system_audio: bool,
    app_audio: bool,
    copy_video: bool,
    save_video: bool,
    screenshot_folder: String,
    video_folder: String,
    hotkey_ok: bool,
    notice: Option<Instant>,
    /// 録画の保存先を両方オフにしようとした時刻。
    video_notice: Option<Instant>,
    theme: config::AppTheme,
    framed: Option<bool>,
    mica: bool,
}

impl SettingsApp {
    pub(super) fn new(hotkey_ok: bool, mica: bool) -> SettingsApp {
        let settings = config::load_settings();
        SettingsApp {
            copy: settings.copy,
            save: settings.save,
            ask: settings.ask,
            delay: settings.delay,
            history: settings.history,
            history_limit: settings.history_limit,
            countdown: settings.countdown,
            skip_title: settings.skip_title,
            record_cursor: settings.record_cursor,
            record_fps: settings.record_fps,
            freeze: settings.freeze,
            notify: settings.notify,
            keep_toolbar: settings.keep_toolbar,
            outline: settings.outline,
            outline_color: settings.outline_color,
            outline_width: settings.outline_width,
            mic: settings.mic,
            system_audio: settings.system_audio,
            app_audio: settings.app_audio,
            copy_video: settings.copy_video,
            save_video: settings.save_video,
            theme: settings.theme,
            screenshot_folder: settings.screenshot_folder,
            video_folder: settings.video_folder,
            hotkey_ok,
            notice: None,
            video_notice: None,
            framed: None,
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
            skip_title: self.skip_title,
            record_cursor: self.record_cursor,
            record_fps: self.record_fps,
            freeze: self.freeze,
            notify: self.notify,
            keep_toolbar: self.keep_toolbar,
            outline: self.outline,
            outline_color: self.outline_color,
            outline_width: self.outline_width,
            mic: self.mic,
            system_audio: self.system_audio,
            app_audio: self.app_audio,
            copy_video: self.copy_video,
            save_video: self.save_video,
            theme: self.theme,
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
        if self.framed != Some(dark) {
            self.framed = Some(dark);
            // SAFETY: the title is a valid UTF-16 string and the handle is
            // only passed to DWM attribute calls.
            unsafe {
                let hwnd = ui::identify_app_window(SETTINGS_WINDOW);
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
        let bounds = ui.max_rect();
        let header = egui::Rect::from_min_size(
            bounds.min + Vec2::new(8.0, 16.0),
            Vec2::new(bounds.width() - 16.0, 40.0),
        );
        ui.painter().text(
            pos2(bounds.left() + MARGIN, header.center().y),
            Align2::LEFT_CENTER,
            tr("Settings"),
            semibold(20.0),
            p.text,
        );
        let mut rect = bounds.shrink2(Vec2::new(MARGIN, 0.0));
        rect.min.y = header.bottom() + 12.0;
        let mut body = ui.new_child(UiBuilder::new().max_rect(rect));
        let ui = &mut body;
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        let before = self.settings();
        let recent =
            |notice: Option<Instant>| notice.is_some_and(|t| t.elapsed() < Duration::from_secs(4));
        let warning = recent(self.notice);
        let video_warning = recent(self.video_notice);
        let folder_action = egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                let width = ui.available_width() - 14.0;
                self.sections(ui, &p, width, (warning, video_warning))
            })
            .inner;
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
                    let owner = unsafe { ui::find_app_window(SETTINGS_WINDOW) };
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
        if !self.copy_video && !self.save_video {
            // 録画も、コピーと保存のどちらかは残す。
            (self.copy_video, self.save_video) = (before.copy_video, before.save_video);
            if !before.copy_video && !before.save_video {
                self.save_video = true;
            }
            self.video_notice = Some(Instant::now());
        }
        if self.settings() != before {
            if self.theme != before.theme {
                apply_theme(&ctx, self.theme);
            }
            self.commit();
        }
        if warning || video_warning {
            ctx.request_repaint_after(Duration::from_secs(4));
        }
    }
}
