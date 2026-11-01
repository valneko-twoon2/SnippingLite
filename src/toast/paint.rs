//! 通知の描画: アプリ名、閉じるボタン、題と本文、メニューのボタン、開くボタン。大きさはすべて
//! 通知の倍率（`Toast::scale`）で決め、文字は行の上下中央に置く。

use super::*;

pub(super) unsafe fn paint(toast: &Toast, dc: HDC) {
    let mut client: RECT = std::mem::zeroed();
    GetClientRect(toast.hwnd, &mut client);
    let Some(mut canvas) = Canvas::new(client.right, client.bottom) else {
        return;
    };
    let theme = ui::theme();
    let s = toast.scale;
    let title_top = 48.0;
    canvas.surface(theme.dark, s);
    let small = Font::new(ui::TEXT_FONT, 12.0 * s, FW_NORMAL as i32);
    canvas.app_icon(
        (PADDING * s).round() as i32,
        (18.0 * s).round() as i32,
        (16.0 * s).round() as u32,
    );
    canvas.text(
        "Snipping Lite",
        &small,
        ui::rect(
            (PADDING + 24.0) * s,
            PADDING * s,
            (WIDTH - PADDING - 40.0) * s,
            (PADDING + HEADER) * s,
        ),
        DT_LEFT | DT_VCENTER | DT_SINGLELINE,
        theme.secondary,
    );
    {
        let button = close_button(s, client.right);
        if toast.close_hover {
            canvas.fill_round_rect(
                [
                    button.left as f32,
                    button.top as f32,
                    button.right as f32,
                    button.bottom as f32,
                ],
                4.0 * s,
                theme.hover,
            );
        }
        let glyph = Font::new(ui::ICON_FONT, 10.0 * s, FW_NORMAL as i32);
        canvas.text(
            "\u{E711}",
            &glyph,
            button,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            theme.text,
        );
    }
    let title = Font::new(ui::TEXT_FONT, 14.0 * s, FW_NORMAL as i32);
    let body = Font::new(ui::TEXT_FONT, 14.0 * s, FW_NORMAL as i32);
    canvas.text(
        toast.title,
        &title,
        ui::rect(
            PADDING * s,
            title_top * s,
            (WIDTH - PADDING) * s,
            (title_top + 20.0) * s,
        ),
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
        theme.text,
    );
    canvas.text(
        &toast.body,
        &body,
        ui::rect(
            PADDING * s,
            (title_top + 22.0) * s,
            (WIDTH - PADDING) * s,
            (title_top + 40.0) * s,
        ),
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_PATH_ELLIPSIS,
        theme.secondary,
    );
    let menu = ui::rect((WIDTH - 84.0) * s, 8.0 * s, (WIDTH - 44.0) * s, 40.0 * s);
    let glyph = Font::new(ui::ICON_FONT, 12.0 * s, FW_NORMAL as i32);
    canvas.text(
        "\u{E712}",
        &glyph,
        menu,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        theme.secondary,
    );
    if toast.can_open {
        let top = if toast.body.is_empty() { 88.0 } else { 108.0 };
        let button = [
            PADDING * s,
            top * s,
            (WIDTH - PADDING) * s,
            (top + 32.0) * s,
        ];
        let border = if theme.dark { 0xff404040 } else { 0xffd0d0d0 };
        let fill = if theme.dark {
            if toast.action_hover {
                0xff353535
            } else {
                0xff2d2d2d
            }
        } else if toast.action_hover {
            0xffeeeeee
        } else {
            0xfffafafa
        };
        canvas.fill_round_rect(button, 4.0 * s, border);
        canvas.fill_round_rect(
            [button[0] + s, button[1] + s, button[2] - s, button[3] - s],
            3.0 * s,
            fill,
        );
        canvas.text(
            if toast.edit {
                tr("Markup and share")
            } else {
                tr("Open")
            },
            &body,
            ui::rect(button[0], button[1], button[2], button[3]),
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            theme.text,
        );
    }
    canvas.present(dc);
}
