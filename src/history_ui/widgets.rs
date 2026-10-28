//! Small controls shared by the gallery and the opened screenshot.

use super::*;

/// A round color choice; the selected one gets a ring.
pub(super) fn swatch(
    ui: &mut egui::Ui,
    rgb: [u8; 3],
    selected: bool,
    p: &Palette,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::Button, ui.is_enabled(), selected, tr("Color"))
    });
    let painter = ui.painter();
    painter.circle_filled(
        rect.center(),
        9.0,
        Color32::from_rgb(rgb[0], rgb[1], rgb[2]),
    );
    painter.circle_stroke(rect.center(), 9.0, Stroke::new(1.0, p.stroke));
    if selected || response.hovered() {
        painter.circle_stroke(rect.center(), 12.0, Stroke::new(2.0, p.text));
    }
    response
}

/// A toggle button that is filled with the accent color while selected.
pub(super) fn chip(ui: &mut egui::Ui, label: &str, p: &Palette, selected: bool) -> egui::Response {
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
pub(super) fn text_button(
    ui: &mut egui::Ui,
    label: &str,
    p: &Palette,
    danger: bool,
) -> egui::Response {
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
