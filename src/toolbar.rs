//! The command bar shown at the top of the screen while snipping.

use super::*;

mod area_menu;
#[cfg(test)]
pub(crate) use area_menu::area_mode;

/// Fluent command bar: capture type, area picker, editing, history and settings.
const BAR_HEIGHT: f32 = 48.0;
const BAR_WIDTH: f32 = 376.0;
pub(crate) const CAMERA_BUTTON: i32 = 0;
pub(crate) const VIDEO_BUTTON: i32 = 1;
pub(crate) const AREA_BUTTON: i32 = 2;
pub(crate) const EDIT_BUTTON: i32 = 3;
pub(crate) const HISTORY_BUTTON: i32 = 4;
pub(crate) const SETTINGS_BUTTON: i32 = 5;
pub(crate) const CLOSE_BUTTON: i32 = 6;
pub(crate) const TEXT_BUTTON: i32 = 7;
const GLYPHS: [&str; 8] = [
    "\u{E722}", "\u{E714}", "\u{EF20}", "\u{E932}", "\u{E81C}", "\u{E713}", "\u{E711}", "\u{EE6F}",
];
pub(crate) fn toolbar_button(index: i32, scale: f32) -> [f32; 4] {
    let (left, width) = match index {
        CAMERA_BUTTON => (8.0, 36.0),
        VIDEO_BUTTON => (44.0, 36.0),
        AREA_BUTTON => (92.0, 48.0),
        EDIT_BUTTON => (148.0, 36.0),
        TEXT_BUTTON => (188.0, 36.0),
        HISTORY_BUTTON => (240.0, 36.0),
        SETTINGS_BUTTON => (280.0, 36.0),
        _ => (332.0, 36.0),
    };
    [
        left * scale,
        8.0 * scale,
        (left + width) * scale,
        40.0 * scale,
    ]
}

pub(crate) unsafe fn place_toolbar(cursor: POINT) {
    let scale = state().scale;
    let mut info: MONITORINFO = std::mem::zeroed();
    info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    GetMonitorInfoW(
        MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST),
        &mut info,
    );
    let width = (BAR_WIDTH * scale).round() as i32;
    let height = (BAR_HEIGHT * scale).round() as i32;
    let monitor = info.rcMonitor;
    // DWM attribute calls are slow, so only repeat them when the theme changes.
    if state().toolbar_dark != Some(state().theme.dark) {
        ui::acrylic(state().toolbar, state().theme.dark);
        state().toolbar_dark = Some(state().theme.dark);
    }
    // A kept toolbar stays where it was moved; a newly opened one starts at the top center.
    // ディスプレイの構成が変わって残したツールバーが画面からはみ出したときだけ、いちばん近い
    // モニターの中へ寄せる（`ui::monitors::keep_on_screen`）。
    let mut bounds: RECT = std::mem::zeroed();
    let (left, top) = if state().toolbar_open && GetWindowRect(state().toolbar, &mut bounds) != 0 {
        let resized = RECT {
            right: bounds.left + width,
            bottom: bounds.top + height,
            ..bounds
        };
        let kept = ui::monitors::keep_on_screen(&ui::monitors::current(), resized);
        (kept.left, kept.top)
    } else {
        (
            (monitor.left + monitor.right - width) / 2,
            monitor.top + (12.0 * scale).round() as i32,
        )
    };
    SetWindowPos(
        state().toolbar,
        null_mut(),
        left,
        top,
        width,
        height,
        SWP_NOACTIVATE | SWP_NOZORDER,
    );
}
/// 残したツールバーの倍率: いちばん広く重なるモニター（画面の外なら、戻す先のいちばん近い
/// モニター）の DPI。Windows も窓の DPI をこのモニターで決める。撮影を始めたときのマウスが
/// 別の DPI のモニターにあっても、ツールバーはそれのあるモニターの倍率で描く。
pub(crate) unsafe fn kept_scale() -> f32 {
    let mut bounds: RECT = std::mem::zeroed();
    GetWindowRect(state().toolbar, &mut bounds);
    ui::monitors::most_of(&ui::monitors::current(), bounds)
        .map_or(state().scale, |monitor| ui::monitors::scale(monitor.dpi))
}
/// Shows a kept toolbar between snips where it was left. A countdown or a
/// recording on another monitor changes the scale, so the toolbar is sized and
/// painted again for the monitor it is on.
pub(crate) unsafe fn show_kept_toolbar() {
    let toolbar = state().toolbar;
    let mut bounds: RECT = std::mem::zeroed();
    GetWindowRect(toolbar, &mut bounds);
    let corner = POINT {
        x: bounds.left,
        y: bounds.top,
    };
    state().scale = kept_scale();
    place_toolbar(corner);
    InvalidateRect(toolbar, null(), 0);
    ShowWindow(toolbar, SW_SHOWNOACTIVATE);
    ui::raise(toolbar);
    // A window that is always on top may rise over it a moment later; see `ui::raise`.
    SetTimer(toolbar, RAISE_TIMER, 200, None);
}
/// WM_DPICHANGED: 別の DPI のモニターへドラッグした、またはモニターの拡大率が変わった。新しい
/// 倍率で作り直し、提案された矩形の左上へ移す。大きさは提案ではなく倍率から決める。自分で別の
/// DPI のモニターへ置き直したとき（撮影の開始、構成の変化）にもこのメッセージが届き、そのときは
/// 倍率をもう新しい値にしてあるので、何もしない（提案の大きさを当てると二重に拡大しうる）。
///
/// 隠れているとき（書き込み中、カウントダウン中、録画中）も何もしない。倍率 `state().scale` は
/// 書き込みのバー、カウントダウン、録画中の表示と共有で、それらは「倍率が今と同じなら何もしない」と
/// 判定する。隠れたツールバーが先に書き換えると、見えている窓が作り直されず、古い大きさの窓に
/// 新しい倍率で描かれてボタンが窓の外に切れる。ツールバーを出すとき（`show_kept_toolbar`、撮影の
/// 開始）は倍率を求め直すので、隠れている間の変化を覚えておく必要はない。
unsafe fn rescale(hwnd: HWND, wp: WPARAM, lp: LPARAM) {
    let scale = ui::monitors::message_scale(wp);
    if IsWindowVisible(hwnd) == 0 || scale == state().scale || lp == 0 {
        return;
    }
    // 番地が揃っていなくても panic しないよう read_unaligned で読む（デバッグ版の `*` は揃いを
    // 確かめ、揃っていなければ panic する）。
    let suggested = std::ptr::read_unaligned(lp as *const RECT);
    state().scale = scale;
    SetWindowPos(
        hwnd,
        null_mut(),
        suggested.left,
        suggested.top,
        (BAR_WIDTH * scale).round() as i32,
        (BAR_HEIGHT * scale).round() as i32,
        SWP_NOACTIVATE | SWP_NOZORDER,
    );
    InvalidateRect(hwnd, null(), 0);
}
/// The button under `p` in client coordinates, or -1 for the space that moves the toolbar.
fn button_at(p: POINT, scale: f32) -> i32 {
    (0..GLYPHS.len() as i32)
        .find(|&i| ui::contains(toolbar_button(i, scale), p))
        .unwrap_or(-1)
}
unsafe fn toolbar_hit(p: POINT) -> i32 {
    button_at(p, state().scale)
}
unsafe fn paint_toolbar(hwnd: HWND, dc: HDC) {
    let mut client: RECT = std::mem::zeroed();
    GetClientRect(hwnd, &mut client);
    let Some(mut canvas) = ui::Canvas::new(client.right, client.bottom) else {
        return;
    };
    let s = state();
    let theme = &s.theme;
    let scale = s.scale;
    let icon = ui::Font::new(ui::ICON_FONT, 16.0 * scale, FW_NORMAL as i32);
    canvas.surface(theme.dark, scale);
    // The two capture types share one segmented outline.
    canvas.fill_round_rect(
        [8.0 * scale, 8.0 * scale, 80.0 * scale, 40.0 * scale],
        4.0 * scale,
        theme.divider,
    );
    canvas.fill_round_rect(
        [9.0 * scale, 9.0 * scale, 79.0 * scale, 39.0 * scale],
        3.0 * scale,
        if theme.dark { 0xff202020 } else { 0xfff3f3f3 },
    );
    for (i, glyph) in GLYPHS.iter().enumerate() {
        let i = i as i32;
        let r = toolbar_button(i, scale);
        let selected = (i == CAMERA_BUTTON && !s.video) || (i == VIDEO_BUTTON && s.video);
        // ペンと文字のボタンはオン・オフの切り替えで、オンのときはアクセントの色で塗る。
        let toggled = (i == EDIT_BUTTON && s.markup) || (i == TEXT_BUTTON && s.extract);
        let fill = if toggled {
            if i == s.pressed_button {
                theme.accent & 0xccffffff
            } else if i == s.hover_button {
                theme.accent & 0xe6ffffff
            } else {
                theme.accent
            }
        } else if i == s.pressed_button {
            theme.pressed
        } else if i == s.hover_button {
            theme.hover
        } else if selected {
            theme.selected
        } else {
            0
        };
        if fill != 0 {
            canvas.fill_round_rect(r, 4.0 * scale, fill);
        }
        if selected {
            let cx = (r[0] + r[2]) / 2.0;
            canvas.fill_round_rect(
                [cx - 7.0 * scale, r[3] - 3.0 * scale, cx + 7.0 * scale, r[3]],
                1.5 * scale,
                theme.accent,
            );
        }
        let glyph = if i == AREA_BUTTON {
            area_menu::glyph(s.mode)
        } else {
            glyph
        };
        let right = if i == AREA_BUTTON {
            r[2] - 14.0 * scale
        } else {
            r[2]
        };
        canvas.text(
            glyph,
            &icon,
            ui::rect(r[0], r[1], right, r[3]),
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            if toggled {
                // アクセントの上に置く文字の色。
                if theme.dark {
                    0xff000000
                } else {
                    0xffffffff
                }
            } else {
                theme.text
            },
        );
        if i == AREA_BUTTON {
            let chevron = ui::Font::new(ui::ICON_FONT, 8.0 * scale, FW_NORMAL as i32);
            canvas.text(
                "\u{E70D}",
                &chevron,
                ui::rect(right, r[1], r[2], r[3]),
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                theme.text,
            );
        }
    }
    for x in [232.0, 324.0] {
        canvas.fill_round_rect(
            [x * scale, 12.0 * scale, (x + 1.0) * scale, 36.0 * scale],
            0.0,
            theme.divider,
        );
    }
    canvas.present(dc);
}
pub(crate) unsafe extern "system" fn toolbar_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        // The space between the buttons acts as a title bar for moving the toolbar.
        WM_NCHITTEST => {
            let mut p = point(lp);
            ScreenToClient(hwnd, &mut p);
            if toolbar_hit(p) < 0 {
                HTCAPTION as LRESULT
            } else {
                HTCLIENT as LRESULT
            }
        }
        WM_NCLBUTTONDBLCLK => 0,
        WM_MEASUREITEM => {
            area_menu::measure(lp);
            1
        }
        WM_DRAWITEM => {
            area_menu::draw(lp);
            1
        }
        WM_DPICHANGED => {
            rescale(hwnd, wp, lp);
            0
        }
        WM_TIMER if wp == RAISE_TIMER => {
            KillTimer(hwnd, RAISE_TIMER);
            // Only a kept toolbar between snips; a closed one stays hidden.
            if !state().active && IsWindowVisible(hwnd) != 0 {
                ui::raise(hwnd);
            }
            0
        }
        WM_EXITSIZEMOVE => {
            // Moving may take the keyboard away from the overlay, which handles Escape.
            if state().active {
                SetForegroundWindow(state().overlay);
            }
            0
        }
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let dc = BeginPaint(hwnd, &mut ps);
            paint_toolbar(hwnd, dc);
            EndPaint(hwnd, &ps);
            0
        }
        WM_MOUSEMOVE => {
            let hit = toolbar_hit(point(lp));
            if hit != state().hover_button {
                if state().hover_button < 0 {
                    let mut track = TRACKMOUSEEVENT {
                        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    TrackMouseEvent(&mut track);
                }
                state().hover_button = hit;
                InvalidateRect(hwnd, null(), 0);
            }
            0
        }
        WM_MOUSELEAVE => {
            state().hover_button = -1;
            state().pressed_button = -1;
            InvalidateRect(hwnd, null(), 0);
            0
        }
        WM_LBUTTONDOWN => {
            state().pressed_button = toolbar_hit(point(lp));
            InvalidateRect(hwnd, null(), 0);
            0
        }
        WM_LBUTTONUP => {
            let hit = toolbar_hit(point(lp));
            let pressed = std::mem::replace(&mut state().pressed_button, -1);
            if hit >= 0 && hit == pressed {
                match hit {
                    CLOSE_BUTTON => {
                        state().toolbar_open = false;
                        ShowWindow(hwnd, SW_HIDE);
                        cancel();
                    }
                    SETTINGS_BUTTON => {
                        cancel();
                        launch_settings();
                    }
                    // Snipping Tool と同じく、押しても画面は変わらない。オンにしておくと、
                    // 切り取った範囲にその場で書き込んでから確定できる。
                    EDIT_BUTTON => {
                        state().markup = !state().markup;
                        if state().markup {
                            state().extract = false;
                        }
                        config::save_markup(state().markup);
                        if state().active {
                            SetForegroundWindow(state().overlay);
                        }
                    }
                    // 文字のボタンもペンと同じく、押しても画面は変わらない。オンにしておくと、
                    // 切り取った範囲の文字がクリップボードに入る。ペンとは同時に使えない。
                    TEXT_BUTTON => {
                        state().extract = !state().extract;
                        if state().extract && state().markup {
                            state().markup = false;
                            config::save_markup(false);
                        }
                        if state().active {
                            SetForegroundWindow(state().overlay);
                        }
                    }
                    HISTORY_BUTTON => {
                        cancel();
                        launch_history(false);
                    }
                    CAMERA_BUTTON | VIDEO_BUTTON => {
                        state().video = hit == VIDEO_BUTTON;
                        if !state().active {
                            // A kept toolbar between snips starts a new snip.
                            begin_capture();
                        } else if hovers_windows() {
                            // Refresh window bounds because recordings may exclude the title bar.
                            let mode = state().mode;
                            set_mode(mode);
                        }
                    }
                    AREA_BUTTON => area_menu::choose(hwnd),
                    _ => {}
                }
                InvalidateRect(hwnd, null(), 0);
            } else {
                InvalidateRect(hwnd, null(), 0);
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

#[cfg(test)]
mod tests;
