//! The painted controls of the player: play button, seek bar and volume.

use crate::settings_ui::Palette;
use eframe::egui::{self, pos2, Color32, Sense, Stroke, Vec2};
use std::time::Duration;

/// `m:ss`, or `h:mm:ss` from one hour on.
pub(super) fn clock(at: Duration) -> String {
    let seconds = at.as_secs();
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// A round button with the play or pause mark; true when it was clicked.
pub(super) fn play_button(ui: &mut egui::Ui, p: &Palette, playing: bool, label: &str) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(40.0), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), label));
    let painter = ui.painter();
    if response.hovered() {
        painter.circle_filled(rect.center(), 20.0, p.field);
    }
    let c = rect.center();
    if playing {
        for dx in [-4.0, 4.0] {
            painter.rect_filled(
                egui::Rect::from_center_size(pos2(c.x + dx, c.y), Vec2::new(4.0, 14.0)),
                1.0,
                p.text,
            );
        }
    } else {
        painter.add(egui::Shape::convex_polygon(
            vec![
                pos2(c.x - 5.0, c.y - 8.0),
                pos2(c.x - 5.0, c.y + 8.0),
                pos2(c.x + 8.0, c.y),
            ],
            p.text,
            Stroke::NONE,
        ));
    }
    response.clicked()
}

/// A horizontal slider over `0.0..=1.0` with an accent track and a round knob. Returns the
/// new value while the pointer presses it.
pub(super) fn slider(
    ui: &mut egui::Ui,
    p: &Palette,
    width: f32,
    value: f32,
    label: &str,
) -> Option<f32> {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 32.0), Sense::click_and_drag());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Slider, ui.is_enabled(), label));
    let (left, right) = (rect.left() + 8.0, rect.right() - 8.0);
    let mut chosen = None;
    if let Some(at) = response
        .interact_pointer_pos()
        .filter(|_| response.is_pointer_button_down_on() || response.clicked())
    {
        chosen = Some(((at.x - left) / (right - left)).clamp(0.0, 1.0));
    }
    let shown = chosen.unwrap_or(value).clamp(0.0, 1.0);
    let (y, x) = (rect.center().y, egui::lerp(left..=right, shown));
    let painter = ui.painter();
    painter.line_segment([pos2(left, y), pos2(right, y)], Stroke::new(4.0, p.stroke));
    painter.line_segment([pos2(left, y), pos2(x, y)], Stroke::new(4.0, p.accent));
    let knob = if response.hovered() { 8.0 } else { 6.0 };
    painter.circle_filled(pos2(x, y), knob, p.accent);
    chosen
}

/// The loudspeaker mark; true when it was clicked (mute or unmute).
pub(super) fn speaker_button(ui: &mut egui::Ui, p: &Palette, muted: bool, label: &str) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(32.0), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), label));
    let painter = ui.painter();
    if response.hovered() {
        painter.circle_filled(rect.center(), 16.0, p.field);
    }
    let c = rect.center();
    let stroke = Stroke::new(1.4, p.text);
    painter.add(egui::Shape::convex_polygon(
        vec![
            pos2(c.x - 7.0, c.y - 3.0),
            pos2(c.x - 3.0, c.y - 3.0),
            pos2(c.x + 1.0, c.y - 7.0),
            pos2(c.x + 1.0, c.y + 7.0),
            pos2(c.x - 3.0, c.y + 3.0),
            pos2(c.x - 7.0, c.y + 3.0),
        ],
        p.text,
        Stroke::NONE,
    ));
    if muted {
        let slash = Stroke::new(1.4, Color32::from_rgb(0xe8, 0x11, 0x23));
        painter.line_segment(
            [pos2(c.x + 4.0, c.y - 4.0), pos2(c.x + 9.0, c.y + 4.0)],
            slash,
        );
        painter.line_segment(
            [pos2(c.x + 9.0, c.y - 4.0), pos2(c.x + 4.0, c.y + 4.0)],
            slash,
        );
    } else {
        painter.line_segment(
            [pos2(c.x + 4.5, c.y - 3.0), pos2(c.x + 4.5, c.y + 3.0)],
            stroke,
        );
        painter.line_segment(
            [pos2(c.x + 8.0, c.y - 5.0), pos2(c.x + 8.0, c.y + 5.0)],
            stroke,
        );
    }
    response.clicked()
}
