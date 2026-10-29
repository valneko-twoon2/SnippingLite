//! An opened screenshot: actions, pen marks and crop.

use super::*;

impl HistoryApp {
    /// Handles pen input over the image and previews all marks.
    fn draw_marks(&mut self, ui: &mut egui::Ui, shown: Rect, scale: f32) {
        let tool = self.tool;
        let (color, thickness) = (self.color, self.thickness);
        let Some(opened) = self.opened.as_mut() else {
            return;
        };
        if !matches!(tool, Tool::None | Tool::Picker) {
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
            if tool == Tool::Crop {
                if let Some(pos) = response.interact_pointer_pos() {
                    if response.drag_started() {
                        opened.crop = Some((point(pos), point(pos)));
                    } else if response.dragged() {
                        if let Some((_, end)) = &mut opened.crop {
                            *end = point(pos);
                        }
                    }
                }
            } else if response.drag_started() {
                if let Some(pos) = response.interact_pointer_pos() {
                    let highlight = tool == Tool::Highlighter;
                    opened.marks.push(Mark {
                        highlight,
                        color: annotate::COLORS[color],
                        radius: if highlight {
                            annotate::HIGHLIGHTER_RADII[thickness]
                        } else {
                            annotate::PEN_RADII[thickness]
                        },
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
            let radius = mark.radius;
            let color = Color32::from_rgba_unmultiplied(
                mark.color[0],
                mark.color[1],
                mark.color[2],
                (annotate::opacity(mark.highlight) * 255.0) as u8,
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
        if let Some(((ax, ay), (bx, by))) = opened.crop {
            let area = Rect::from_two_pos(
                shown.min + Vec2::new(ax, ay) * scale,
                shown.min + Vec2::new(bx, by) * scale,
            )
            .intersect(shown);
            let dim = Color32::from_black_alpha(130);
            let strips = [
                Rect::from_min_max(shown.min, pos2(shown.max.x, area.min.y)),
                Rect::from_min_max(pos2(shown.min.x, area.max.y), shown.max),
                Rect::from_min_max(pos2(shown.min.x, area.min.y), pos2(area.min.x, area.max.y)),
                Rect::from_min_max(pos2(area.max.x, area.min.y), pos2(shown.max.x, area.max.y)),
            ];
            for strip in strips {
                painter.rect_filled(strip, 0.0, dim);
            }
            painter.rect_stroke(
                area,
                0.0,
                Stroke::new(1.5, Color32::WHITE),
                StrokeKind::Inside,
            );
        }
    }

    /// Copies the text of a finished recognition, or reports why there is none.
    fn finish_recognition(&mut self, ctx: &egui::Context) {
        let Some(receiver) = &self.ocr else {
            return;
        };
        let message = match receiver.try_recv() {
            Ok(Ok(text)) if text.trim().is_empty() => tr("No text was found in the image."),
            Ok(Ok(text)) => {
                if imaging::copy_text(window(), &text) {
                    tr("Copied the text.")
                } else {
                    tr("Could not copy the text.")
                }
            }
            Ok(Err(error)) => error.message(),
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(Duration::from_millis(100));
                return;
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => crate::ocr::Error::Failed.message(),
        };
        self.ocr = None;
        self.notice = Some((Instant::now(), message));
    }

    /// Shows the color under the pointer; a click copies it as `#RRGGBB`.
    fn pick_color(&mut self, ui: &mut egui::Ui, shown: Rect, scale: f32, p: &Palette) {
        let response = ui.interact(shown, ui.make_persistent_id("picker"), Sense::click());
        let Some(pos) = response.hover_pos() else {
            return;
        };
        ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::Crosshair);
        let Some(opened) = &self.opened else {
            return;
        };
        let pixel =
            annotate::pixel_of((pos.x - shown.min.x) / scale, (pos.y - shown.min.y) / scale);
        let Some(color) = annotate::color_at(&opened.pixels, opened.size.0, opened.size.1, pixel)
        else {
            return;
        };
        let text = annotate::hex(color);
        // The readout follows the pointer and stays on the screen at the right edge.
        let size = Vec2::new(108.0, 32.0);
        let mut min = pos + Vec2::new(16.0, 16.0);
        if min.x + size.x > ui.max_rect().right() {
            min.x = pos.x - 16.0 - size.x;
        }
        let card = Rect::from_min_size(min, size);
        let painter = ui.painter();
        painter.rect_filled(card, CornerRadius::same(4), p.card);
        painter.rect_stroke(
            card,
            CornerRadius::same(4),
            Stroke::new(1.0, p.stroke),
            StrokeKind::Inside,
        );
        let chip = Rect::from_min_size(card.min + Vec2::new(4.0, 4.0), Vec2::splat(24.0));
        painter.rect_filled(
            chip,
            CornerRadius::same(3),
            Color32::from_rgb(color[0], color[1], color[2]),
        );
        painter.rect_stroke(
            chip,
            CornerRadius::same(3),
            Stroke::new(1.0, p.stroke),
            StrokeKind::Inside,
        );
        painter.text(
            pos2(chip.right() + 8.0, card.center().y),
            Align2::LEFT_CENTER,
            &text,
            FontId::new(13.0, FontFamily::Monospace),
            p.text,
        );
        if response.clicked() {
            let copied = imaging::copy_text(window(), &text);
            self.notice = Some((
                Instant::now(),
                if copied {
                    tr("Copied the color.")
                } else {
                    tr("Could not copy the color.")
                },
            ));
        }
    }

    /// Cuts the image down to the chosen area; marks are drawn in first.
    fn apply_crop(&mut self, ctx: &egui::Context) {
        let Some(opened) = &mut self.opened else {
            return;
        };
        let Some(((ax, ay), (bx, by))) = opened.crop.take() else {
            return;
        };
        let area = (
            ax.min(bx).round() as i32,
            ay.min(by).round() as i32,
            ax.max(bx).round() as i32,
            ay.max(by).round() as i32,
        );
        let flat = opened.flattened();
        let Some((width, height, pixels)) =
            annotate::crop(&flat, opened.size.0, opened.size.1, area)
        else {
            return;
        };
        opened.texture = Some(texture_from(
            ctx,
            &format!("{}#crop", opened.entry.image.to_string_lossy()),
            width,
            height,
            &pixels,
        ));
        opened.pixels = pixels;
        opened.size = (width, height);
        opened.marks.clear();
        opened.cropped = true;
        self.tool = Tool::None;
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
            opened.cropped = false;
            self.thumbnails.remove(&opened.entry.id);
            self.notice = Some((Instant::now(), tr("Saved the changes.")));
        } else {
            self.notice = Some((Instant::now(), tr("Could not save the changes.")));
        }
    }

    pub(super) fn detail(&mut self, ctx: &egui::Context, ui: &mut egui::Ui, p: &Palette) {
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
        self.finish_recognition(ctx);
        let mut action = None;
        ui.add_space(20.0);
        ui.horizontal(|ui| {
            if text_button(ui, tr("Back"), p, false).clicked() {
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
                if text_button(ui, tr("Delete"), p, true).clicked() {
                    action = Some(Action::Delete);
                }
                if text_button(ui, tr("Show in folder"), p, false).clicked() {
                    action = Some(Action::Folder);
                }
                if text_button(ui, tr("Save as..."), p, false).clicked() {
                    action = Some(Action::SaveAs);
                }
                if text_button(ui, tr("Copy"), p, false).clicked() {
                    action = Some(Action::Copy);
                }
                if text_button(ui, tr("Copy text"), p, false).clicked() {
                    action = Some(Action::CopyText);
                }
            });
        });
        ui.add_space(8.0);
        let mut undo = false;
        let mut persist = false;
        let mut apply = false;
        ui.horizontal_wrapped(|ui| {
            let tools = [
                (Tool::Pen, tr("Pen")),
                (Tool::Highlighter, tr("Highlighter")),
                (Tool::Crop, tr("Crop")),
                (Tool::Picker, tr("Pick color")),
            ];
            for (tool, label) in tools {
                if chip(ui, label, p, self.tool == tool).clicked() {
                    self.tool = if self.tool == tool { Tool::None } else { tool };
                }
                ui.add_space(6.0);
            }
            if matches!(self.tool, Tool::Pen | Tool::Highlighter) {
                ui.add_space(8.0);
                for (index, rgb) in annotate::COLORS.iter().enumerate() {
                    if swatch(ui, *rgb, self.color == index, p).clicked() {
                        self.color = index;
                        config::save_pen(self.color, self.thickness);
                    }
                }
                ui.add_space(8.0);
                for (index, label) in [tr("Thin"), tr("Medium"), tr("Thick")]
                    .into_iter()
                    .enumerate()
                {
                    if chip(ui, label, p, self.thickness == index).clicked() {
                        self.thickness = index;
                        config::save_pen(self.color, self.thickness);
                    }
                    ui.add_space(6.0);
                }
            }
            let (marked, cropped, selecting) =
                self.opened.as_ref().map_or((false, false, false), |o| {
                    (!o.marks.is_empty(), o.cropped, o.crop.is_some())
                });
            if self.tool == Tool::Crop && selecting {
                ui.add_space(8.0);
                apply = text_button(ui, tr("Apply crop"), p, false).clicked();
            }
            if marked {
                ui.add_space(8.0);
                undo = text_button(ui, tr("Undo"), p, false).clicked();
            }
            if marked || cropped {
                ui.add_space(6.0);
                persist = text_button(ui, tr("Save changes"), p, false).clicked();
            }
        });
        if apply {
            self.apply_crop(ctx);
        }
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
            if self.tool == Tool::Picker {
                self.pick_color(ui, shown, scale, p);
            }
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
                tr("Left and right arrow keys browse, Esc goes back."),
                p.secondary,
            ),
        };
        ui.painter().text(
            // The hint sits in the 28 px band below the image, centered vertically.
            pos2(room.left(), room.bottom() + 14.0),
            Align2::LEFT_CENTER,
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
                            tr("Copied to the clipboard.")
                        } else {
                            tr("Could not copy the image.")
                        },
                    ));
                }
            }
            Some(Action::CopyText) => {
                if self.ocr.is_none() {
                    if let Some(opened) = &self.opened {
                        self.ocr = Some(crate::ocr::spawn(
                            opened.flattened(),
                            opened.size.0,
                            opened.size.1,
                        ));
                        self.notice = Some((Instant::now(), tr("Recognizing text...")));
                    }
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
                                tr("Saved the image.")
                            } else {
                                tr("Could not save the image.")
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
    CopyText,
    SaveAs,
}
