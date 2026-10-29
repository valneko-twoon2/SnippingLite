//! The settings window state and layout.

use super::*;

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
    mic: bool,
    system_audio: bool,
    app_audio: bool,
    ask_video: bool,
    screenshot_folder: String,
    video_folder: String,
    hotkey_ok: bool,
    notice: Option<Instant>,
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
            mic: settings.mic,
            system_audio: settings.system_audio,
            app_audio: settings.app_audio,
            ask_video: settings.ask_video,
            theme: settings.theme,
            screenshot_folder: settings.screenshot_folder,
            video_folder: settings.video_folder,
            hotkey_ok,
            notice: None,
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
            mic: self.mic,
            system_audio: self.system_audio,
            app_audio: self.app_audio,
            ask_video: self.ask_video,
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
                    divider(ui, &p);
                    let title = tr("Show notifications");
                    let note = (tr("After a screenshot or recording is saved"), p.secondary);
                    row(ui, &p, title, Some(note), |ui| {
                        switch(ui, &mut self.notify, title, &p);
                    });
                    divider(ui, &p);
                    let title = tr("Keep the toolbar open after a snip");
                    let note = (
                        tr("Close it with the × button. Drag an empty part to move it"),
                        p.secondary,
                    );
                    row(ui, &p, title, Some(note), |ui| {
                        switch(ui, &mut self.keep_toolbar, title, &p);
                    });
                });
                section(ui, &p, tr("Screen recording"), true);
                card(ui, &p, width, |ui| {
                    let title = tr("Countdown before recording");
                    row(ui, &p, title, None, |ui| {
                        switch(ui, &mut self.countdown, title, &p);
                    });
                    divider(ui, &p);
                    let title = tr("Include the mouse cursor in recordings");
                    row(ui, &p, title, None, |ui| {
                        switch(ui, &mut self.record_cursor, title, &p);
                    });
                    divider(ui, &p);
                    let title = tr("Include microphone input in recordings");
                    row(ui, &p, title, None, |ui| {
                        switch(ui, &mut self.mic, title, &p);
                    });
                    divider(ui, &p);
                    let title = tr("Include system audio in recordings");
                    row(ui, &p, title, None, |ui| {
                        switch(ui, &mut self.system_audio, title, &p);
                    });
                    // Only meaningful while system audio is recorded at all.
                    if self.system_audio {
                        divider(ui, &p);
                        let title = tr("Record only the sound of the window's app");
                        let note = (
                            tr("Turn off to record all system audio when recording a window"),
                            p.secondary,
                        );
                        row(ui, &p, title, Some(note), |ui| {
                            switch(ui, &mut self.app_audio, title, &p);
                        });
                    }
                    divider(ui, &p);
                    row(ui, &p, tr("Recording frame rate"), None, |ui| {
                        egui::ComboBox::from_id_salt("record_fps")
                            .selected_text(
                                tr("{n} fps").replace("{n}", &self.record_fps.to_string()),
                            )
                            .width(124.0)
                            .show_ui(ui, |ui| {
                                for fps in FRAME_RATES {
                                    ui.selectable_value(
                                        &mut self.record_fps,
                                        fps,
                                        tr("{n} fps").replace("{n}", &fps.to_string()),
                                    );
                                }
                            });
                    });
                    divider(ui, &p);
                    let title = tr("Leave out the title bar when recording a window");
                    row(
                        ui,
                        &p,
                        title,
                        Some((tr("Click a window to record it"), p.secondary)),
                        |ui| {
                            switch(ui, &mut self.skip_title, title, &p);
                        },
                    );
                    divider(ui, &p);
                    let title = tr("Ask for a file name when saving a recording");
                    row(ui, &p, title, None, |ui| {
                        switch(ui, &mut self.ask_video, title, &p);
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
                section(ui, &p, tr("Appearance"), true);
                card(ui, &p, width, |ui| {
                    row(ui, &p, tr("App theme"), None, |ui| {
                        egui::ComboBox::from_id_salt("app_theme")
                            .selected_text(self.theme.label())
                            .width(180.0)
                            .show_ui(ui, |ui| {
                                for choice in config::AppTheme::ALL {
                                    ui.selectable_value(&mut self.theme, choice, choice.label());
                                }
                            });
                    });
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
        if self.settings() != before {
            if self.theme != before.theme {
                apply_theme(&ctx, self.theme);
            }
            self.commit();
        }
        if warning {
            ctx.request_repaint_after(Duration::from_secs(4));
        }
    }
}
