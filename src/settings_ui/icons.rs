//! The small line icons in front of the recording rows.

use super::*;

#[derive(Clone, Copy)]
pub(super) enum Icon {
    Microphone,
    Speaker,
}

/// A row with an icon before its title, drawn like the other cards.
pub(super) fn icon_row(
    ui: &mut egui::Ui,
    p: &Palette,
    icon: Icon,
    title: &str,
    content: impl FnOnce(&mut egui::Ui),
) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(4), p.card);
    draw(ui, pos2(rect.left() + 26.0, rect.center().y), icon, p.text);
    ui.painter().text(
        rect.left_center() + Vec2::new(48.0, 0.0),
        Align2::LEFT_CENTER,
        title,
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

fn draw(ui: &egui::Ui, c: egui::Pos2, icon: Icon, color: Color32) {
    let stroke = Stroke::new(1.2, color);
    let painter = ui.painter();
    match icon {
        Icon::Microphone => {
            let capsule = egui::Rect::from_center_size(pos2(c.x, c.y - 2.0), Vec2::new(5.0, 9.0));
            painter.rect_stroke(capsule, CornerRadius::same(3), stroke, StrokeKind::Middle);
            let cup = vec![
                pos2(c.x - 4.5, c.y + 0.5),
                pos2(c.x - 4.5, c.y + 2.0),
                pos2(c.x - 2.5, c.y + 4.5),
                pos2(c.x + 2.5, c.y + 4.5),
                pos2(c.x + 4.5, c.y + 2.0),
                pos2(c.x + 4.5, c.y + 0.5),
            ];
            painter.add(egui::Shape::line(cup, stroke));
            painter.line_segment([pos2(c.x, c.y + 4.5), pos2(c.x, c.y + 7.0)], stroke);
        }
        Icon::Speaker => {
            let screen = egui::Rect::from_center_size(pos2(c.x, c.y - 1.0), Vec2::new(14.0, 10.0));
            painter.rect_stroke(screen, CornerRadius::same(1), stroke, StrokeKind::Middle);
            let cone = vec![
                pos2(c.x + 1.0, c.y + 4.0),
                pos2(c.x - 1.0, c.y + 6.0),
                pos2(c.x - 1.0, c.y + 2.0),
            ];
            painter.add(egui::Shape::convex_polygon(cone, color, Stroke::NONE));
            painter.line_segment(
                [pos2(c.x + 3.0, c.y + 3.0), pos2(c.x + 3.0, c.y + 5.0)],
                stroke,
            );
        }
    }
}
