//! Rows and controls of the settings window, after the Windows Snipping Tool.

use super::*;

const ROW_TALL: f32 = 64.0;
/// スイッチの右端を、行の右端の余白（16）から更に空ける幅。折りたたみの矢印の分。
const SWITCH_RIGHT_INSET: f32 = 32.0;

/// A section heading above a card.
pub(super) fn section(ui: &mut egui::Ui, p: &Palette, title: &str, gap: bool) {
    ui.add_space(if gap { 24.0 } else { 4.0 });
    ui.label(RichText::new(title).size(14.0).color(p.text));
    ui.add_space(8.0);
}

/// A rounded card that holds the rows of one section.
pub(super) fn card(
    ui: &mut egui::Ui,
    _p: &Palette,
    width: f32,
    content: impl FnOnce(&mut egui::Ui),
) {
    egui::Frame::new()
        .fill(Color32::TRANSPARENT)
        .show(ui, |ui| {
            ui.set_width(width - 2.0);
            content(ui);
        });
}

/// A settings row: the title (and an optional note below it) on the left and
/// the control on the right.
pub(super) fn row(
    ui: &mut egui::Ui,
    p: &Palette,
    title: &str,
    note: Option<(&str, Color32)>,
    content: impl FnOnce(&mut egui::Ui),
) {
    let height = if note.is_some() { ROW_TALL } else { ROW };
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(4), p.card);
    let offset = if note.is_some() { -10.0 } else { 0.0 };
    ui.painter().text(
        rect.left_center() + Vec2::new(16.0, offset),
        Align2::LEFT_CENTER,
        title,
        FontId::new(14.0, FontFamily::Proportional),
        p.text,
    );
    if let Some((text, color)) = note {
        ui.painter().text(
            rect.left_center() + Vec2::new(16.0, 11.0),
            Align2::LEFT_CENTER,
            text,
            FontId::new(12.0, FontFamily::Proportional),
            color,
        );
    }
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(rect.shrink2(Vec2::new(16.0, 0.0)))
            .layout(Layout::right_to_left(Align::Center)),
    );
    content(&mut child);
}

pub(super) enum FolderAction {
    Change,
    Open,
}

/// A folder row: the title, the path with a Change link, and an Open folder button.
pub(super) fn folder_row(
    ui: &mut egui::Ui,
    p: &Palette,
    title: &str,
    path: &Path,
    filled: bool,
) -> Option<FolderAction> {
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_TALL), Sense::hover());
    if filled {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(4), p.card);
    }
    let left = if filled { 16.0 } else { 32.0 };
    ui.painter().text(
        rect.left_center() + Vec2::new(left, -10.0),
        Align2::LEFT_CENTER,
        title,
        FontId::new(14.0, FontFamily::Proportional),
        p.text,
    );
    let font = FontId::new(12.0, FontFamily::Proportional);
    // Long paths keep their end, where the folder name is.
    let room = rect.width() - left - 190.0;
    let mut shown = path.display().to_string();
    while shown.chars().count() > 8
        && ui
            .painter()
            .layout_no_wrap(shown.clone(), font.clone(), p.secondary)
            .size()
            .x
            > room
    {
        shown = format!("…{}", shown.chars().skip(2).collect::<String>());
    }
    let text_rect = ui.painter().text(
        rect.left_center() + Vec2::new(left, 11.0),
        Align2::LEFT_CENTER,
        shown,
        font.clone(),
        p.secondary,
    );
    let link = ui.painter().text(
        pos2(text_rect.right() + 8.0, text_rect.center().y),
        Align2::LEFT_CENTER,
        tr("Change"),
        font,
        p.accent,
    );
    let mut action = None;
    let response = ui.interact(
        link.expand(4.0),
        ui.make_persistent_id((title, "change")),
        Sense::click(),
    );
    if response.hovered() {
        ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::PointingHand);
    }
    if response.clicked() {
        action = Some(FolderAction::Change);
    }
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(rect.shrink2(Vec2::new(16.0, 0.0)))
            .layout(Layout::right_to_left(Align::Center)),
    );
    if outline_button(&mut child, tr("Open folder"), p).clicked() {
        action = Some(FolderAction::Open);
    }
    action
}

/// A filled text button without a border, as the Snipping Tool uses for secondary actions.
pub(super) fn outline_button(ui: &mut egui::Ui, label: &str, p: &Palette) -> egui::Response {
    let font = FontId::new(14.0, FontFamily::Proportional);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font.clone(), p.text);
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(galley.size().x + 32.0, 32.0), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), label));
    let dark = p.text == Color32::WHITE;
    let fill = match (
        response.is_pointer_button_down_on(),
        response.hovered(),
        dark,
    ) {
        (true, _, true) => Color32::from_rgb(0x2f, 0x2f, 0x2f),
        (true, _, false) => Color32::from_rgb(0xf0, 0xf0, 0xf0),
        (false, true, true) => Color32::from_rgb(0x40, 0x40, 0x40),
        (false, true, false) => Color32::from_rgb(0xf6, 0xf6, 0xf6),
        (false, false, true) => Color32::from_rgb(0x38, 0x38, 0x38),
        (false, false, false) => Color32::WHITE,
    };
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(4), fill);
    if !dark {
        painter.rect_stroke(
            rect,
            CornerRadius::same(4),
            Stroke::new(1.0, p.stroke),
            StrokeKind::Inside,
        );
    }
    painter.text(rect.center(), Align2::CENTER_CENTER, label, font, p.text);
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

/// A toggle switch with its On or Off label to the left, as in the Windows settings.
pub(super) fn switch(ui: &mut egui::Ui, on: &mut bool, label: &str, p: &Palette) {
    let text = if *on { tr("On") } else { tr("Off") };
    // 折りたたみの行は右端に矢印があるぶんスイッチが左へ寄るので、どの行も同じ位置から始まるよう、
    // 矢印の幅（折りたたみの行の右端 48 から、行の余白 16 を引いた 32）を空ける。
    ui.add_space(SWITCH_RIGHT_INSET);
    // オンとオフで文字の幅が違っても、スイッチの位置が行ごとにずれないよう、長いほうの幅を確保する。
    let font = egui::FontId::proportional(13.0);
    let width = [tr("On"), tr("Off")]
        .into_iter()
        .map(|t| {
            ui.painter()
                .layout_no_wrap(t.to_owned(), font.clone(), p.text)
                .size()
                .x
        })
        .fold(0.0, f32::max)
        .ceil();
    ui.allocate_ui_with_layout(
        Vec2::new(width, 20.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| ui.label(RichText::new(text).size(13.0).color(p.text)),
    );
    ui.add_space(10.0);
    toggle(ui, on, label, p);
}

pub(super) fn divider(ui: &mut egui::Ui, _p: &Palette) {
    ui.add_space(6.0);
}

/// A Windows 11 toggle switch.
fn toggle(ui: &mut egui::Ui, on: &mut bool, label: &str, p: &Palette) -> egui::Response {
    let (rect, mut response) = ui.allocate_exact_size(Vec2::new(40.0, 20.0), Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::CheckBox, ui.is_enabled(), *on, label)
    });
    let t = ui.ctx().animate_bool_responsive(response.id, *on);
    let radius = CornerRadius::same(10);
    let painter = ui.painter();
    if *on || t > 0.0 {
        painter.rect_filled(rect, radius, p.accent.gamma_multiply(t));
    }
    if t < 1.0 {
        painter.rect_stroke(
            rect,
            radius,
            Stroke::new(1.0, p.secondary.gamma_multiply(1.0 - t)),
            StrokeKind::Inside,
        );
    }
    let knob = if response.hovered() || response.has_focus() {
        7.0
    } else {
        6.0
    };
    let x = egui::lerp((rect.left() + 10.0)..=(rect.right() - 10.0), t);
    let color = if *on { p.on_accent } else { p.secondary };
    painter.circle_filled(egui::pos2(x, rect.center().y), knob, color);
    if response.has_focus() {
        painter.rect_stroke(
            rect.expand(3.0),
            CornerRadius::same(13),
            Stroke::new(2.0, p.text),
            StrokeKind::Outside,
        );
    }
    response
}
