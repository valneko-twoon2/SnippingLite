//! 録画中の表示: 赤い点、経過時間、停止のボタン。録画する範囲のあるモニターの上部中央に、
//! そのモニターの倍率で出す。動画には写らない。

use super::*;

/// Recording indicator in DIPs: a red dot, the elapsed time and a Stop label.
pub(super) const RECORD_WIDTH: f32 = 168.0;
pub(super) const RECORD_HEIGHT: f32 = 48.0;
/// The Stop button of the recording indicator; only a click on it ends the recording.
pub(super) fn stop_button(scale: f32) -> [f32; 4] {
    [100.0 * scale, 8.0 * scale, 160.0 * scale, 40.0 * scale]
}

/// 表示を置く矩形: 範囲の真ん中 `center` にいちばん近いモニターの上部中央に、倍率 `scale` の
/// 大きさで（ツールバーと同じく上端から 12 DIP）。
pub(super) fn indicator_rect(
    monitors: &[ui::monitors::Monitor],
    center: POINT,
    scale: f32,
) -> Option<RECT> {
    let monitor = ui::monitors::nearest(monitors, center)?;
    Some(ui::monitors::top_center(
        monitor.rect,
        (RECORD_WIDTH * scale).round() as i32,
        (RECORD_HEIGHT * scale).round() as i32,
        scale,
    ))
}

pub(super) unsafe fn show_indicator(center: POINT) {
    let theme = ui::theme();
    let scale = ui::scale_at(center);
    let Some(r) = indicator_rect(&ui::monitors::current(), center, scale) else {
        return;
    };
    let hwnd = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
        w(RECORD).as_ptr(),
        w(RECORD).as_ptr(),
        WS_POPUP,
        r.left,
        r.top,
        r.right - r.left,
        r.bottom - r.top,
        null_mut(),
        null_mut(),
        GetModuleHandleW(null()),
        null(),
    );
    if hwnd.is_null() {
        return;
    }
    // The indicator must not appear in the video it controls.
    SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);
    ui::acrylic(hwnd, theme.dark);
    ui::disable_transitions(hwnd);
    state().theme = theme;
    state().scale = scale;
    state().record_window = hwnd;
    ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    SetTimer(hwnd, RECORD_TIMER, 500, None);
}

/// 録画している範囲の真ん中（画面の座標）。
unsafe fn recorded_center() -> Option<POINT> {
    let area = state().recording.as_ref()?.display_area()?;
    Some(POINT {
        x: (area.left + area.right) / 2,
        y: (area.top + area.bottom) / 2,
    })
}

/// 表示を倍率 `scale` で作り直し、範囲の真ん中に近いモニターの上部中央へ置き直す。
unsafe fn place_indicator(center: POINT, scale: f32) {
    let s = state();
    let Some(r) = indicator_rect(&ui::monitors::current(), center, scale) else {
        return;
    };
    s.scale = scale;
    SetWindowPos(
        s.record_window,
        null_mut(),
        r.left,
        r.top,
        r.right - r.left,
        r.bottom - r.top,
        SWP_NOACTIVATE | SWP_NOZORDER,
    );
    InvalidateRect(s.record_window, null(), 0);
}

/// ディスプレイの構成が変わった: 表示のあったモニターを外しても停止のボタンに届くよう、範囲の
/// 真ん中に近いモニターの上部中央へ、そのモニターの倍率で置き直す。
pub(crate) unsafe fn replace_indicator() {
    if state().record_window.is_null() {
        return;
    }
    if let Some(center) = recorded_center() {
        place_indicator(center, ui::scale_at(center));
    }
}

unsafe fn paint_record(hwnd: HWND, dc: HDC) {
    let mut client: RECT = std::mem::zeroed();
    GetClientRect(hwnd, &mut client);
    let Some(mut canvas) = ui::Canvas::new(client.right, client.bottom) else {
        return;
    };
    let s = state();
    let (theme, scale) = (&s.theme, s.scale);
    let elapsed = s.recording.as_ref().map_or(0, |r| r.elapsed().as_secs());
    let text = ui::Font::new(ui::TEXT_FONT, 14.0 * scale, FW_NORMAL as i32);
    canvas.fill_round_rect(
        [14.0 * scale, 18.0 * scale, 26.0 * scale, 30.0 * scale],
        6.0 * scale,
        0xffe81123,
    );
    canvas.text(
        &format!("{:02}:{:02}", elapsed / 60, elapsed % 60),
        &text,
        ui::rect(36.0 * scale, 0.0, 92.0 * scale, client.bottom as f32),
        DT_LEFT | DT_VCENTER | DT_SINGLELINE,
        theme.text,
    );
    canvas.fill_round_rect(stop_button(scale), 4.0 * scale, theme.selected);
    canvas.fill_round_rect(
        [110.0 * scale, 18.0 * scale, 122.0 * scale, 30.0 * scale],
        2.0 * scale,
        theme.text,
    );
    canvas.text(
        tr("Stop"),
        &text,
        ui::rect(128.0 * scale, 0.0, 160.0 * scale, client.bottom as f32),
        DT_LEFT | DT_VCENTER | DT_SINGLELINE,
        theme.text,
    );
    canvas.present(dc);
}
pub(crate) unsafe extern "system" fn record_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        WM_ERASEBKGND => 1,
        // 表示のあるモニターの拡大率が変わった。新しい倍率で作り直し、同じ規則で置き直す。
        // 自分で置き直したときにも届き、そのときは倍率をもう新しい値にしてあるので、何もしない。
        WM_DPICHANGED => {
            let scale = ui::monitors::message_scale(wp);
            if scale != state().scale {
                if let Some(center) = recorded_center() {
                    place_indicator(center, scale);
                }
            }
            0
        }
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let dc = BeginPaint(hwnd, &mut ps);
            paint_record(hwnd, dc);
            EndPaint(hwnd, &ps);
            0
        }
        WM_TIMER if wp == RECORD_TIMER => {
            // A recorder that ended on its own failed; stopping reports the error.
            if state().recording.as_ref().is_some_and(|r| r.failed()) {
                stop_recording();
            } else {
                if let Some(area) = state()
                    .recording
                    .as_ref()
                    .and_then(|recording| recording.display_area())
                {
                    show_frame(area);
                }
                // Keep the Stop button reachable over windows that are always on top;
                // see `ui::raise`. The frame goes first so the indicator stays above it.
                ui::raise(state().record_frame);
                ui::raise(hwnd);
                InvalidateRect(hwnd, null(), 0);
            }
            0
        }
        WM_LBUTTONUP => {
            if ui::contains(stop_button(state().scale), point(lp)) {
                stop_recording();
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
