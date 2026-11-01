//! Expanders, radio rows, links and the border preview of the settings page, after the
//! Windows Snipping Tool.

use super::*;
use crate::annotate;

/// The title and the optional note of a row, painted from `left` pixels in.
fn heading(
    ui: &egui::Ui,
    rect: egui::Rect,
    p: &Palette,
    left: f32,
    title: &str,
    note: Option<(&str, Color32)>,
) {
    let offset = if note.is_some() { -10.0 } else { 0.0 };
    ui.painter().text(
        rect.left_center() + Vec2::new(left, offset),
        Align2::LEFT_CENTER,
        title,
        FontId::new(14.0, FontFamily::Proportional),
        p.text,
    );
    if let Some((text, color)) = note {
        ui.painter().text(
            rect.left_center() + Vec2::new(left, 11.0),
            Align2::LEFT_CENTER,
            text,
            FontId::new(12.0, FontFamily::Proportional),
            color,
        );
    }
}

/// A row inside an expander: no card of its own, the control on the right.
pub(super) fn plain_row(
    ui: &mut egui::Ui,
    p: &Palette,
    left: f32,
    title: &str,
    content: impl FnOnce(&mut egui::Ui),
) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 48.0), Sense::hover());
    heading(ui, rect, p, left, title, None);
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(rect.shrink2(Vec2::new(16.0, 0.0)))
            .layout(Layout::right_to_left(Align::Center)),
    );
    content(&mut child);
}

fn chevron(ui: &egui::Ui, center: egui::Pos2, open: bool, p: &Palette) {
    let tip = if open { -2.5 } else { 2.5 };
    let points = vec![
        pos2(center.x - 4.5, center.y - tip),
        pos2(center.x, center.y + tip),
        pos2(center.x + 4.5, center.y - tip),
    ];
    ui.painter()
        .add(egui::Shape::line(points, Stroke::new(1.3, p.text)));
}

/// A card that opens to show more rows below it. With `on` it also carries a switch.
/// The expanded state is remembered while the window is open. Snipping Tool と同じく、
/// 設定ウィンドウを開いたときはスイッチがオンでも閉じている。
pub(super) fn expander(
    ui: &mut egui::Ui,
    p: &Palette,
    key: &str,
    title: &str,
    note: Option<(&str, Color32)>,
    on: Option<&mut bool>,
    content: impl FnOnce(&mut egui::Ui),
) {
    let id = ui.make_persistent_id(("expander", key));
    let start = on.as_deref().copied().unwrap_or(false);
    let mut open = ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
    // Turning the switch on opens the card, so its settings are in sight.
    let was_on = ui.data(|d| d.get_temp::<bool>(id.with("on")));
    if was_on == Some(false) && start {
        open = true;
    }
    ui.data_mut(|d| d.insert_temp(id.with("on"), start));
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW), Sense::hover());
    // Registered before the switch, so a click on the switch does not toggle the expander.
    if ui
        .interact(rect, id.with("header"), Sense::click())
        .clicked()
    {
        open = !open;
    }
    let corners = if open {
        CornerRadius {
            nw: 4,
            ne: 4,
            sw: 0,
            se: 0,
        }
    } else {
        CornerRadius::same(4)
    };
    ui.painter().rect_filled(rect, corners, p.card);
    heading(ui, rect, p, 16.0, title, note);
    chevron(ui, pos2(rect.right() - 28.0, rect.center().y), open, p);
    if let Some(on) = on {
        let area = egui::Rect::from_min_max(rect.min, pos2(rect.right() - 16.0, rect.max.y));
        let mut child = ui.new_child(
            UiBuilder::new()
                .max_rect(area)
                .layout(Layout::right_to_left(Align::Center)),
        );
        switch(&mut child, on, title, p);
    }
    ui.data_mut(|d| d.insert_temp(id, open));
    if !open {
        return;
    }
    let (line, _) = ui.allocate_exact_size(Vec2::new(rect.width(), 1.0), Sense::hover());
    ui.painter().rect_filled(line, 0.0, p.stroke);
    egui::Frame::new()
        .fill(p.card)
        .corner_radius(CornerRadius {
            nw: 0,
            ne: 0,
            sw: 4,
            se: 4,
        })
        .show(ui, |ui| {
            ui.set_width(rect.width());
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            content(ui);
        });
}

/// One choice of a group, with the round mark on the left.
pub(super) fn radio_row(ui: &mut egui::Ui, p: &Palette, label: &str, selected: bool) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 44.0), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::RadioButton, ui.is_enabled(), selected, label)
    });
    if response.hovered() {
        ui.painter().rect_filled(rect, 0.0, p.field);
    }
    let center = pos2(rect.left() + 28.0, rect.center().y);
    let painter = ui.painter();
    if selected {
        painter.circle_filled(center, 10.0, p.accent);
        painter.circle_filled(center, 5.0, p.on_accent);
    } else {
        painter.circle_stroke(center, 9.5, Stroke::new(1.0, p.secondary));
    }
    painter.text(
        pos2(rect.left() + 52.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::new(14.0, FontFamily::Proportional),
        p.text,
    );
    response.clicked()
}

/// A text link in the accent color.
pub(super) fn link(ui: &mut egui::Ui, p: &Palette, text: &str) -> egui::Response {
    let font = FontId::new(14.0, FontFamily::Proportional);
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), font.clone(), p.accent);
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(galley.size().x + 36.0, 44.0), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Link, ui.is_enabled(), text));
    if response.hovered() {
        ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::PointingHand);
    }
    let at = pos2(rect.left() + 24.0, rect.center().y);
    let shown = ui
        .painter()
        .text(at, Align2::LEFT_CENTER, text, font, p.accent);
    if response.hovered() {
        ui.painter().hline(
            shown.x_range(),
            shown.bottom() - 1.0,
            Stroke::new(1.0, p.accent),
        );
    }
    response
}

/// The sample picture with the chosen border, as the Snipping Tool previews it.
pub(super) fn border_preview(ui: &mut egui::Ui, color: usize, width: usize) {
    let (area, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 212.0), Sense::hover());
    let picture = egui::Rect::from_center_size(area.center(), Vec2::new(240.0, 140.0));
    let (sky, ground) = (
        Color32::from_rgb(178, 150, 176),
        Color32::from_rgb(236, 150, 118),
    );
    let mut mesh = egui::epaint::Mesh::default();
    mesh.colored_vertex(picture.left_top(), sky);
    mesh.colored_vertex(picture.right_top(), sky);
    mesh.colored_vertex(picture.right_bottom(), ground);
    mesh.colored_vertex(picture.left_bottom(), ground);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    let painter = ui.painter();
    painter.add(egui::Shape::mesh(mesh));
    let at = |x: f32, y: f32| {
        pos2(
            picture.left() + picture.width() * x,
            picture.top() + picture.height() * y,
        )
    };
    for (points, shade) in [
        (
            [at(0.0, 1.0), at(0.32, 0.55), at(0.7, 1.0)],
            Color32::from_rgb(196, 112, 92),
        ),
        (
            [at(0.38, 1.0), at(0.78, 0.5), at(1.0, 1.0)],
            Color32::from_rgb(214, 134, 108),
        ),
    ] {
        painter.add(egui::Shape::convex_polygon(
            points.to_vec(),
            shade,
            Stroke::NONE,
        ));
    }
    let rgb = annotate::COLORS[color];
    // The border is twice as thick as in the picture, so that one pixel still shows.
    let thickness = annotate::OUTLINE_WIDTHS[width] as f32 * 2.0;
    painter.rect_stroke(
        picture,
        0.0,
        Stroke::new(thickness, Color32::from_rgb(rgb[0], rgb[1], rgb[2])),
        StrokeKind::Inside,
    );
}

/// A square color button; `selected` rings it.
pub(super) fn swatch(
    ui: &mut egui::Ui,
    p: &Palette,
    rgb: [u8; 3],
    size: f32,
    selected: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), "color"));
    let painter = ui.painter();
    painter.rect_filled(
        rect,
        CornerRadius::same(4),
        Color32::from_rgb(rgb[0], rgb[1], rgb[2]),
    );
    if selected || response.hovered() {
        painter.rect_stroke(
            rect.expand(2.0),
            CornerRadius::same(5),
            Stroke::new(2.0, p.text),
            StrokeKind::Outside,
        );
    }
    response
}

/// A slider with `steps` stops, as the Snipping Tool draws its thickness: an accent track up to
/// a round knob. Returns the stop that was chosen by a click or drag.
pub(super) fn step_slider(
    ui: &mut egui::Ui,
    p: &Palette,
    steps: usize,
    current: usize,
    label: &str,
) -> Option<usize> {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(140.0, 32.0), Sense::click_and_drag());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Slider, ui.is_enabled(), label));
    let (left, right) = (rect.left() + 10.0, rect.right() - 10.0);
    let stop = |i: usize| egui::lerp(left..=right, i as f32 / (steps - 1) as f32);
    let mut chosen = None;
    if let Some(at) = response
        .interact_pointer_pos()
        .filter(|_| response.is_pointer_button_down_on() || response.clicked())
    {
        let t = ((at.x - left) / (right - left)).clamp(0.0, 1.0);
        let index = (t * (steps - 1) as f32).round() as usize;
        if index != current {
            chosen = Some(index);
        }
    }
    let shown = chosen.unwrap_or(current);
    let y = rect.center().y;
    let painter = ui.painter();
    painter.line_segment([pos2(left, y), pos2(right, y)], Stroke::new(4.0, p.stroke));
    painter.line_segment(
        [pos2(left, y), pos2(stop(shown), y)],
        Stroke::new(4.0, p.accent),
    );
    let knob = if response.hovered() { 10.0 } else { 9.0 };
    painter.circle_filled(pos2(stop(shown), y), knob, p.text);
    painter.circle_filled(pos2(stop(shown), y), knob - 4.0, p.accent);
    chosen
}
