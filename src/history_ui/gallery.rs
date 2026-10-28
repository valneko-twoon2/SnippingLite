//! The gallery: date filters and the grid of screenshots.

use super::*;

impl HistoryApp {
    pub(super) fn header(&mut self, ui: &mut egui::Ui, p: &Palette) {
        ui.add_space(20.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr("History")).font(semibold(20.0)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if self.confirm_clear {
                    if text_button(ui, tr("Cancel"), p, false).clicked() {
                        self.confirm_clear = false;
                    }
                    if text_button(ui, tr("Delete all"), p, true).clicked() {
                        history::clear();
                        self.confirm_clear = false;
                        self.refresh();
                    }
                    ui.label(
                        RichText::new(tr("Delete every screenshot in the history?"))
                            .size(12.0)
                            .color(p.warning),
                    );
                } else if !self.entries.is_empty()
                    && text_button(ui, tr("Clear"), p, false).clicked()
                {
                    self.confirm_clear = true;
                }
            });
        });
        ui.add_space(4.0);
        let shown = self.visible().len();
        let text = match (self.entries.len(), shown) {
            (0, _) => tr("No screenshots yet").to_owned(),
            (total, shown) if shown == total => {
                tr("{total} screenshots, newest first (the oldest beyond {limit} are removed)")
                    .replace("{total}", &total.to_string())
                    .replace("{limit}", &self.limit.to_string())
            }
            (total, shown) => tr("{shown} of {total} screenshots match")
                .replace("{shown}", &shown.to_string())
                .replace("{total}", &total.to_string()),
        };
        ui.label(RichText::new(text).size(12.0).color(p.secondary));
        if !self.recording {
            ui.label(
                RichText::new(tr("History recording is turned off in Settings."))
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
                        .hint_text(tr("Search by date, e.g. 2026/10/28"))
                        .desired_width(220.0),
                );
            });
        }
        ui.add_space(12.0);
    }

    pub(super) fn gallery(&mut self, ctx: &egui::Context, ui: &mut egui::Ui, p: &Palette) {
        self.header(ui, p);
        if self.entries.is_empty() {
            ui.add_space(40.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(tr("Captures appear here after you take them."))
                        .color(p.secondary),
                );
            });
            return;
        }
        let entries = self.visible();
        if entries.is_empty() {
            ui.add_space(40.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(tr("No screenshots match.")).color(p.secondary));
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
                tr("Screenshot {date} {time}")
                    .replace("{date}", &date_label(entry.id))
                    .replace("{time}", &time_label(entry.id)),
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
}
