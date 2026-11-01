//! The sections of the settings page, in the order of the Windows Snipping Tool.
//! The settings only this app has stay in the section they belong to.

use super::*;
use crate::{about, annotate};

/// Where users send feedback.
const FEEDBACK_URL: &str = "https://github.com/valneko-twoon2/SnippingLite/issues";

type Folder = Option<(bool, FolderAction)>;

impl SettingsApp {
    /// Draws every section and returns the folder button that was pressed, if any.
    pub(super) fn sections(
        &mut self,
        ui: &mut egui::Ui,
        p: &Palette,
        width: f32,
        (warning, video_warning): (bool, bool),
    ) -> Folder {
        let mut folder = None;
        section(ui, p, tr("Snipping"), false);
        card(ui, p, width, |ui| {
            self.snipping(ui, p, warning, &mut folder)
        });
        section(ui, p, tr("Screen recording"), true);
        card(ui, p, width, |ui| {
            self.recording(ui, p, video_warning, &mut folder)
        });
        section(ui, p, tr("History"), true);
        card(ui, p, width, |ui| self.history_cards(ui, p));
        section(ui, p, tr("Shortcuts"), true);
        card(ui, p, width, |ui| {
            let note = (!self.hotkey_ok).then_some((
                tr("Shortcut unavailable: another application is using it."),
                p.warning,
            ));
            row(ui, p, tr("Start a snip with Alt + Shift + S"), note, |_| {});
        });
        section(ui, p, tr("Appearance"), true);
        card(ui, p, width, |ui| self.appearance(ui, p));
        self.about(ui, p);
        ui.add_space(20.0);
        folder
    }

    fn snipping(&mut self, ui: &mut egui::Ui, p: &Palette, warning: bool, folder: &mut Folder) {
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
        row(ui, p, title, Some(note), |ui| {
            switch(ui, &mut self.copy, title, p);
        });
        divider(ui, p);
        let current = config::screenshot_folder(&self.settings());
        expander(
            ui,
            p,
            "save",
            tr("Automatically save screenshots"),
            None,
            Some(&mut self.save),
            |ui| {
                if let Some(action) = folder_row(ui, p, tr("Screenshot folder"), &current, false) {
                    *folder = Some((false, action));
                }
            },
        );
        divider(ui, p);
        self.border(ui, p);
        divider(ui, p);
        let title = tr("Dim and freeze the screen while snipping");
        let note = (
            tr("Turn off to keep videos and apps running while you select"),
            p.secondary,
        );
        row(ui, p, title, Some(note), |ui| {
            switch(ui, &mut self.freeze, title, p);
        });
        divider(ui, p);
        let title = tr("Show notifications");
        let note = (tr("After a screenshot or recording is saved"), p.secondary);
        row(ui, p, title, Some(note), |ui| {
            switch(ui, &mut self.notify, title, p);
        });
        divider(ui, p);
        let title = tr("Keep the toolbar open after a snip");
        let note = (
            tr("Close it with the × button. Drag an empty part to move it"),
            p.secondary,
        );
        row(ui, p, title, Some(note), |ui| {
            switch(ui, &mut self.keep_toolbar, title, p);
        });
    }

    /// The border switch with its preview, color and thickness.
    fn border(&mut self, ui: &mut egui::Ui, p: &Palette) {
        let (color, width) = (self.outline_color, self.outline_width);
        let mut picked = (color, width);
        let id = ui.make_persistent_id("border-colors");
        let mut choosing = ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
        expander(
            ui,
            p,
            "border",
            tr("Add a border to screenshots"),
            None,
            Some(&mut self.outline),
            |ui| {
                border_preview(ui, color, width);
                let line = |ui: &mut egui::Ui| {
                    let (rect, _) = ui
                        .allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
                    ui.painter().rect_filled(rect, 0.0, p.stroke);
                };
                line(ui);
                plain_row(ui, p, 16.0, tr("Border color"), |ui| {
                    let rgb = annotate::COLORS[color];
                    if swatch(ui, p, rgb, 40.0, false).clicked() {
                        choosing = !choosing;
                    }
                });
                if choosing {
                    plain_row(ui, p, 16.0, "", |ui| {
                        for (index, rgb) in annotate::COLORS.into_iter().enumerate().rev() {
                            ui.add_space(8.0);
                            if swatch(ui, p, rgb, 28.0, index == color).clicked() {
                                picked.0 = index;
                                choosing = false;
                            }
                        }
                    });
                }
                line(ui);
                plain_row(ui, p, 16.0, tr("Border thickness"), |ui| {
                    let label = tr("Border thickness");
                    let steps = annotate::OUTLINE_WIDTHS.len();
                    if let Some(step) = step_slider(ui, p, steps, width, label) {
                        picked.1 = step;
                    }
                });
            },
        );
        ui.data_mut(|d| d.insert_temp(id, choosing));
        (self.outline_color, self.outline_width) = picked;
    }

    /// Snipping Tool と同じく、コピー、保存（保存先は中に入れる）、マイク、システムオーディオの
    /// 順に並べ、このアプリ独自の項目をその後ろに置く。
    fn recording(&mut self, ui: &mut egui::Ui, p: &Palette, warning: bool, folder: &mut Folder) {
        let note = if warning {
            (
                tr("Choose at least one destination for recordings."),
                p.warning,
            )
        } else {
            (
                tr("Recordings are copied to the clipboard automatically"),
                p.secondary,
            )
        };
        let title = tr("Automatically copy recordings");
        row(ui, p, title, Some(note), |ui| {
            switch(ui, &mut self.copy_video, title, p);
        });
        divider(ui, p);
        let current = config::video_folder(&self.settings());
        expander(
            ui,
            p,
            "save_video",
            tr("Automatically save recordings"),
            None,
            Some(&mut self.save_video),
            |ui| {
                // 両方のカードが開いていても、スクリーンショットの行の操作を上書きしない。
                if let Some(action) = folder_row(ui, p, tr("Recording folder"), &current, false) {
                    *folder = Some((true, action));
                }
            },
        );
        divider(ui, p);
        let title = tr("Include microphone input in recordings");
        icon_row(ui, p, Icon::Microphone, title, |ui| {
            switch(ui, &mut self.mic, title, p);
        });
        divider(ui, p);
        let title = tr("Include system audio in recordings");
        icon_row(ui, p, Icon::Speaker, title, |ui| {
            switch(ui, &mut self.system_audio, title, p);
        });
        // Only meaningful while system audio is recorded at all.
        if self.system_audio {
            divider(ui, p);
            let title = tr("Record only the sound of the window's app");
            let note = (
                tr("Turn off to record all system audio when recording a window"),
                p.secondary,
            );
            row(ui, p, title, Some(note), |ui| {
                switch(ui, &mut self.app_audio, title, p);
            });
        }
        divider(ui, p);
        let title = tr("Countdown before recording");
        row(ui, p, title, None, |ui| {
            switch(ui, &mut self.countdown, title, p);
        });
        divider(ui, p);
        let title = tr("Include the mouse cursor in recordings");
        row(ui, p, title, None, |ui| {
            switch(ui, &mut self.record_cursor, title, p);
        });
        divider(ui, p);
        let title = tr("Leave out the title bar when recording a window");
        let note = (tr("Click a window to record it"), p.secondary);
        row(ui, p, title, Some(note), |ui| {
            switch(ui, &mut self.skip_title, title, p);
        });
    }

    fn history_cards(&mut self, ui: &mut egui::Ui, p: &Palette) {
        let title = tr("Keep a history of screenshots");
        row(ui, p, title, None, |ui| {
            switch(ui, &mut self.history, title, p);
        });
        divider(ui, p);
        row(ui, p, tr("History size"), None, |ui| {
            egui::ComboBox::from_id_salt("history_limit")
                .selected_text(
                    tr("{n} screenshots").replace("{n}", &self.history_limit.to_string()),
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
        divider(ui, p);
        row(ui, p, tr("Open history"), None, |ui| {
            if outline_button(ui, tr("Open"), p).clicked() {
                // SAFETY: the call has no pointer arguments.
                unsafe { AllowSetForegroundWindow(ASFW_ANY) };
                post(WM_COMMAND, TRAY_HISTORY);
            }
        });
    }

    /// The app theme: one card that opens to the three choices.
    fn appearance(&mut self, ui: &mut egui::Ui, p: &Palette) {
        expander(ui, p, "theme", tr("App theme"), None, None, |ui| {
            for choice in config::AppTheme::ALL {
                if radio_row(ui, p, choice.label(), self.theme == choice) {
                    self.theme = choice;
                }
            }
        });
    }

    /// The version, the license and the feedback button at the end of the page.
    fn about(&mut self, ui: &mut egui::Ui, p: &Palette) {
        ui.add_space(24.0);
        let text = |ui: &mut egui::Ui, text: String, size: f32, family: FontFamily| {
            let (rect, _) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), 32.0), Sense::hover());
            ui.painter().text(
                rect.left_center() + Vec2::new(12.0, 0.0),
                Align2::LEFT_CENTER,
                text,
                FontId::new(size, family),
                p.text,
            );
        };
        text(
            ui,
            tr("About this app").to_owned(),
            14.0,
            semibold(14.0).family,
        );
        text(
            ui,
            format!("Snipping Lite {}", env!("CARGO_PKG_VERSION")),
            14.0,
            FontFamily::Proportional,
        );
        text(
            ui,
            "© 2026 valneko-twoon2".to_owned(),
            14.0,
            FontFamily::Proportional,
        );
        ui.add_space(8.0);
        if link(ui, p, tr("License")).clicked() {
            let _ = about::show();
        }
        if link(ui, p, tr("Review third-party software")).clicked() {
            let _ = about::show_credits();
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            if outline_button(ui, tr("Send feedback"), p).clicked() {
                imaging::shell_open(&w(FEEDBACK_URL), None);
            }
        });
    }
}
