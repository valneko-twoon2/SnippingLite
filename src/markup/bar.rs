//! 書き込み中に切り取った範囲の下に出すバー: キャプチャ、ペン、蛍光ペン、消しゴム、
//! 元に戻す、やり直す、すべて消去、閉じる。

use super::*;

pub(crate) const BAR: &str = "SnippingLiteMarkupBar";
const BAR_HEIGHT: f32 = 48.0;
const BAR_WIDTH: f32 = 432.0;
pub(super) const CAPTURE: i32 = 0;
pub(super) const PEN: i32 = 1;
pub(super) const HIGHLIGHTER: i32 = 2;
pub(super) const ERASER: i32 = 3;
pub(super) const UNDO: i32 = 4;
pub(super) const REDO: i32 = 5;
pub(super) const CLEAR: i32 = 6;
pub(super) const CLOSE: i32 = 7;
/// ボタンの左端と幅（DIP）。キャプチャだけは文字のボタン。
const BUTTONS: [(f32, f32); 8] = [
    (8.0, 96.0),
    (120.0, 36.0),
    (160.0, 36.0),
    (200.0, 36.0),
    (252.0, 36.0),
    (292.0, 36.0),
    (344.0, 36.0),
    (388.0, 36.0),
];
/// ペン、蛍光ペン、消しゴム、元に戻す、やり直す、すべて消去、閉じる（Segoe Fluent Icons）。
const GLYPHS: [&str; 8] = [
    "", "\u{E76D}", "\u{E7E6}", "\u{E75C}", "\u{E7A7}", "\u{E7A6}", "\u{E74D}", "\u{E711}",
];
/// グループの間の区切り線（DIP）。
const DIVIDERS: [f32; 3] = [112.0, 244.0, 336.0];

pub(super) fn button(index: i32, scale: f32) -> [f32; 4] {
    let (left, width) = BUTTONS[index as usize];
    [
        left * scale,
        8.0 * scale,
        (left + width) * scale,
        40.0 * scale,
    ]
}

/// `p`（クライアント座標）にあるボタン。ボタンの外なら -1。
pub(super) fn button_at(p: POINT, scale: f32) -> i32 {
    (0..BUTTONS.len() as i32)
        .find(|&i| ui::contains(button(i, scale), p))
        .unwrap_or(-1)
}

/// バーを置く左上（画面の座標）。範囲の下の中央に置き、入らなければ上、それも無理なら範囲の中の下端。
///
/// `monitor` は作業領域（rcWork）で、タスクバーの場所を除く。規則の優先順位は次のとおり。
/// 1. 作業領域の端から間隔の内側に収める（タスクバーにかけない）。範囲の下・上・中のどれよりも
///    先に立つ。範囲が作業領域の外（タスクバーの上だけ）にあれば、バーは範囲から離れてでも、
///    作業領域の中で範囲にいちばん近い端（間隔の内側）に寄せる。
/// 2. 範囲の下、上、範囲の中の下端の順に選ぶ。範囲がタスクバーにかかっていても、作業領域の中で
///    下や上に入らなければ範囲の中に重ねる（範囲の中の位置も作業領域の下端から測る）。
/// 3. 作業領域がバーと間隔より小さいときは、どの置き方でも収まらない。左端と上端を優先し、
///    バーは右と下へはみ出す（ボタンの先頭のキャプチャを画面の中に残す）。
///
/// タスクバーが上・左・右にあっても、作業領域がその分だけ狭いので同じ規則で避ける。自動で隠れる
/// タスクバーは作業領域を狭めないため、出てきたときにはバーにかかりうる。端に間隔を空けるので、
/// 隠れたタスクバーを呼び出す端の数画素はふさがない。
pub(super) fn place(area: RECT, monitor: RECT, scale: f32) -> POINT {
    let (width, height) = (
        (BAR_WIDTH * scale).round() as i32,
        (BAR_HEIGHT * scale).round() as i32,
    );
    let gap = (12.0 * scale).round() as i32;
    let x = ((area.left + area.right) / 2 - width / 2)
        .min(monitor.right - width - gap)
        .max(monitor.left + gap);
    let y = if area.bottom + gap + height <= monitor.bottom - gap {
        area.bottom + gap
    } else if area.top - gap - height >= monitor.top + gap {
        area.top - gap - height
    } else {
        // 範囲の下端から、間隔 2 つ分だけ上（範囲が画面いっぱいのときと同じ間隔）。範囲が作業領域の
        // 下へはみ出していれば作業領域の下端から測る。低すぎる範囲でも上端を越えないよう、後で収める。
        area.bottom.min(monitor.bottom) - gap * 2 - height
    };
    // 範囲が作業領域の外にあると、下や上に置いた位置がタスクバーにかかるので、作業領域へ戻す。
    // 上端を後に当てて、小さすぎる作業領域では上端を優先する（規則 3）。
    let y = y.min(monitor.bottom - gap - height).max(monitor.top + gap);
    POINT { x, y }
}

/// バーを置く作業領域と倍率: 範囲の下端の中央（`place` が基準にする所）にいちばん近いモニターの
/// もの。範囲が、切り取りを始めたときのマウスと別の DPI のモニターにあっても、バーはそれが
/// 出るモニターの倍率で作る（バーは作業領域の中に収まるので、窓の DPI もこのモニターになる）。
pub(super) fn home(area: RECT, monitors: &[ui::monitors::Monitor]) -> Option<(RECT, f32)> {
    let anchor = POINT {
        x: (area.left + area.right) / 2,
        y: area.bottom,
    };
    ui::monitors::nearest(monitors, anchor)
        .map(|monitor| (monitor.work, ui::monitors::scale(monitor.dpi)))
}

/// 書き込み中の範囲（画面の座標）。
unsafe fn screen_area() -> Option<RECT> {
    let s = state();
    let a = s.markup_session.as_ref()?.area;
    Some(RECT {
        left: a.left + s.origin.x,
        top: a.top + s.origin.y,
        right: a.right + s.origin.x,
        bottom: a.bottom + s.origin.y,
    })
}

/// バーを範囲のそばに出す。ディスプレイの構成が変わったときも、これで置き直す。
pub(crate) unsafe fn show() {
    let s = state();
    if s.markup_bar.is_null() {
        s.markup_bar = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            w(BAR).as_ptr(),
            w(BAR).as_ptr(),
            WS_POPUP,
            0,
            0,
            1,
            1,
            s.overlay,
            null_mut(),
            GetModuleHandleW(null()),
            null(),
        );
        if s.markup_bar.is_null() {
            return;
        }
    }
    let Some(area) = screen_area() else {
        return;
    };
    let Some((work, scale)) = home(area, &ui::monitors::current()) else {
        return;
    };
    put(area, work, scale);
}

/// WM_DPICHANGED: バーのあるモニターの拡大率が変わった。新しい倍率で作り直し、同じ規則で
/// 置き直す（作業領域もタスクバーの高さとともに変わりうる）。自分で別の DPI のモニターへ
/// 置き直したときにも届き、そのときは倍率をもう新しい値にしてあるので、何もしない。
unsafe fn rescale(wp: WPARAM) {
    let scale = ui::monitors::message_scale(wp);
    if scale == state().scale {
        return;
    }
    let Some(area) = screen_area() else {
        return;
    };
    if let Some((work, _)) = home(area, &ui::monitors::current()) {
        put(area, work, scale);
    }
}

/// バーを倍率 `scale` で作り、範囲 `area` のそばの作業領域 `work` の中に出す。
unsafe fn put(area: RECT, work: RECT, scale: f32) {
    let s = state();
    s.scale = scale;
    let at = place(area, work, scale);
    ui::acrylic(s.markup_bar, s.theme.dark);
    SetWindowPos(
        s.markup_bar,
        HWND_TOPMOST,
        at.x,
        at.y,
        (BAR_WIDTH * scale).round() as i32,
        (BAR_HEIGHT * scale).round() as i32,
        SWP_NOACTIVATE,
    );
    InvalidateRect(s.markup_bar, null(), 0);
    ShowWindow(s.markup_bar, SW_SHOWNOACTIVATE);
    UpdateWindow(s.markup_bar);
}

unsafe fn paint_bar(hwnd: HWND, dc: HDC) {
    let mut client: RECT = std::mem::zeroed();
    GetClientRect(hwnd, &mut client);
    let Some(mut canvas) = ui::Canvas::new(client.right, client.bottom) else {
        return;
    };
    let s = state();
    let Some(session) = s.markup_session.as_ref() else {
        return;
    };
    let theme = &s.theme;
    let scale = s.scale;
    canvas.surface(theme.dark, scale);
    let icon = ui::Font::new(ui::ICON_FONT, 16.0 * scale, FW_NORMAL as i32);
    let text = ui::Font::new(ui::TEXT_FONT, 14.0 * scale, FW_NORMAL as i32);
    // アクセントの上に置く文字の色。
    let on_accent = if theme.dark { 0xff000000 } else { 0xffffffff };
    for i in 0..BUTTONS.len() as i32 {
        let r = button(i, scale);
        let tool = match i {
            PEN => Some(Tool::Pen),
            HIGHLIGHTER => Some(Tool::Highlighter),
            ERASER => Some(Tool::Eraser),
            _ => None,
        };
        let selected = tool == Some(session.tool);
        let enabled = match i {
            UNDO => !session.undo.is_empty(),
            REDO => !session.redo.is_empty(),
            CLEAR => !session.marks.is_empty(),
            _ => true,
        };
        if i == CAPTURE {
            let fill = if i == session.pressed {
                theme.accent & 0xccffffff
            } else if i == session.hover {
                theme.accent & 0xe6ffffff
            } else {
                theme.accent
            };
            canvas.fill_round_rect(r, 4.0 * scale, fill);
            canvas.text(
                tr("Capture"),
                &text,
                ui::rect(r[0], r[1], r[2], r[3]),
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                on_accent,
            );
            continue;
        }
        let fill = if !enabled {
            0
        } else if i == session.pressed {
            theme.pressed
        } else if i == session.hover {
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
        // ペンと蛍光ペンは、描く色で示す。
        let color = match i {
            _ if !enabled => theme.secondary & 0x60ffffff,
            PEN => {
                let [r, g, b] = annotate::COLORS[config::load_pen().0];
                0xff000000 | (r as u32) << 16 | (g as u32) << 8 | b as u32
            }
            HIGHLIGHTER => {
                let [r, g, b] = annotate::COLORS[1];
                0xff000000 | (r as u32) << 16 | (g as u32) << 8 | b as u32
            }
            _ => theme.text,
        };
        canvas.text(
            GLYPHS[i as usize],
            &icon,
            ui::rect(r[0], r[1], r[2], r[3]),
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            color,
        );
    }
    for x in DIVIDERS {
        canvas.fill_round_rect(
            [x * scale, 12.0 * scale, (x + 1.0) * scale, 36.0 * scale],
            0.0,
            theme.divider,
        );
    }
    canvas.present(dc);
}

/// ボタンを押したときの動作。
pub(super) unsafe fn press(index: i32) {
    match index {
        CAPTURE => capture(false),
        PEN => set_tool(Tool::Pen),
        HIGHLIGHTER => set_tool(Tool::Highlighter),
        ERASER => set_tool(Tool::Eraser),
        UNDO => undo(),
        REDO => redo(),
        CLEAR => clear(),
        CLOSE => cancel(),
        _ => {}
    }
    // キーボード（Esc や Ctrl+Z）はオーバーレイが受け取る。
    if state().active {
        SetForegroundWindow(state().overlay);
    }
}

pub(crate) unsafe extern "system" fn bar_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
) -> LRESULT {
    let scale = state().scale;
    match msg {
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        WM_ERASEBKGND => 1,
        WM_DPICHANGED => {
            rescale(wp);
            0
        }
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let dc = BeginPaint(hwnd, &mut ps);
            paint_bar(hwnd, dc);
            EndPaint(hwnd, &ps);
            0
        }
        WM_MOUSEMOVE => {
            let hit = button_at(point(lp), scale);
            if let Some(session) = state().markup_session.as_mut() {
                if hit != session.hover {
                    if session.hover < 0 {
                        let mut track = TRACKMOUSEEVENT {
                            cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                            dwFlags: TME_LEAVE,
                            hwndTrack: hwnd,
                            dwHoverTime: 0,
                        };
                        TrackMouseEvent(&mut track);
                    }
                    session.hover = hit;
                    InvalidateRect(hwnd, null(), 0);
                }
            }
            0
        }
        WM_MOUSELEAVE => {
            if let Some(session) = state().markup_session.as_mut() {
                session.hover = -1;
                session.pressed = -1;
                InvalidateRect(hwnd, null(), 0);
            }
            0
        }
        WM_LBUTTONDOWN => {
            if let Some(session) = state().markup_session.as_mut() {
                session.pressed = button_at(point(lp), scale);
                InvalidateRect(hwnd, null(), 0);
            }
            0
        }
        WM_LBUTTONUP => {
            let hit = button_at(point(lp), scale);
            let pressed = state()
                .markup_session
                .as_mut()
                .map_or(-1, |s| std::mem::replace(&mut s.pressed, -1));
            if hit >= 0 && hit == pressed {
                press(hit);
            }
            InvalidateRect(hwnd, null(), 0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
