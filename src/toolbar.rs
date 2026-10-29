//! The command bar shown at the top of the screen while snipping.

use super::*;

/// Fluent command bar: capture type, area picker, editing, history and settings.
const BAR_HEIGHT: f32 = 48.0;
const BAR_WIDTH: f32 = 332.0;
const CAMERA_BUTTON: i32 = 0;
const VIDEO_BUTTON: i32 = 1;
const AREA_BUTTON: i32 = 2;
const EDIT_BUTTON: i32 = 3;
const HISTORY_BUTTON: i32 = 4;
const SETTINGS_BUTTON: i32 = 5;
const CLOSE_BUTTON: i32 = 6;
const GLYPHS: [&str; 7] = [
    "\u{E722}", "\u{E714}", "\u{EF20}", "\u{E70F}", "\u{E81C}", "\u{E713}", "\u{E711}",
];
fn toolbar_button(index: i32, scale: f32) -> [f32; 4] {
    let (left, width) = match index {
        CAMERA_BUTTON => (8.0, 36.0),
        VIDEO_BUTTON => (44.0, 36.0),
        AREA_BUTTON => (92.0, 48.0),
        EDIT_BUTTON => (148.0, 36.0),
        HISTORY_BUTTON => (196.0, 36.0),
        SETTINGS_BUTTON => (236.0, 36.0),
        _ => (288.0, 36.0),
    };
    [
        left * scale,
        8.0 * scale,
        (left + width) * scale,
        40.0 * scale,
    ]
}

unsafe fn choose_capture_area(hwnd: HWND) {
    let menu = CreatePopupMenu();
    if menu.is_null() {
        return;
    }
    for (mode, label) in [
        (MODE_RECT, tr("Rectangle")),
        (MODE_FREE, tr("Freeform")),
        (MODE_WINDOW, tr("Window")),
        (MODE_SCREEN, tr("Full screen")),
    ] {
        let checked = if state().mode == mode { MF_CHECKED } else { 0 };
        AppendMenuW(
            menu,
            MF_STRING | checked,
            mode as usize + 1,
            w(label).as_ptr(),
        );
    }
    let button = toolbar_button(AREA_BUTTON, state().scale);
    let mut anchor = POINT {
        x: button[0] as i32,
        y: button[3] as i32,
    };
    ClientToScreen(hwnd, &mut anchor);
    SetForegroundWindow(hwnd);
    let command = TrackPopupMenu(
        menu,
        TPM_RETURNCMD | TPM_NONOTIFY,
        anchor.x,
        anchor.y,
        0,
        hwnd,
        null(),
    );
    DestroyMenu(menu);
    if command > 0 && state().active {
        set_mode(command - 1);
    } else if command > 0 {
        // A kept toolbar between snips starts a new snip in the chosen mode.
        state().mode = command - 1;
        begin_capture();
        if state().active {
            set_mode(command - 1);
        }
    }
    if state().active {
        SetForegroundWindow(state().overlay);
    }
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
    let mut bounds: RECT = std::mem::zeroed();
    let (left, top) = if state().toolbar_open && GetWindowRect(state().toolbar, &mut bounds) != 0 {
        (bounds.left, bounds.top)
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
    state().scale = ui::scale_at(corner);
    place_toolbar(corner);
    InvalidateRect(toolbar, null(), 0);
    ShowWindow(toolbar, SW_SHOWNOACTIVATE);
    ui::raise(toolbar);
    // A window that is always on top may rise over it a moment later; see `ui::raise`.
    SetTimer(toolbar, RAISE_TIMER, 200, None);
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
        let fill = if i == s.pressed_button {
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
            match s.mode {
                MODE_FREE => "\u{EA49}",
                MODE_WINDOW => "\u{E7C4}",
                MODE_SCREEN => "\u{E7F4}",
                _ => glyph,
            }
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
            theme.text,
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
    for x in [188.0, 280.0] {
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
                    EDIT_BUTTON | HISTORY_BUTTON => {
                        cancel();
                        launch_history(hit == EDIT_BUTTON);
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
                    AREA_BUTTON => choose_capture_area(hwnd),
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
mod tests {
    use super::*;

    const SCALES: [f32; 5] = [1.0, 1.25, 1.5, 1.75, 2.0];

    fn at(x: i32, y: i32) -> i32 {
        button_at(POINT { x, y }, 1.0)
    }

    #[test]
    fn every_button_is_found_at_its_center_on_every_scale() {
        for scale in SCALES {
            for index in 0..GLYPHS.len() as i32 {
                let [l, t, r, b] = toolbar_button(index, scale);
                let center = POINT {
                    x: ((l + r) / 2.0) as i32,
                    y: ((t + b) / 2.0) as i32,
                };
                assert_eq!(button_at(center, scale), index, "scale {scale}");
            }
        }
    }

    #[test]
    fn button_edges_and_the_gaps_that_move_the_toolbar() {
        // The camera and video buttons touch: the shared edge belongs to video.
        assert_eq!(at(7, 24), -1);
        assert_eq!(at(8, 24), CAMERA_BUTTON);
        assert_eq!(at(43, 24), CAMERA_BUTTON);
        assert_eq!(at(44, 24), VIDEO_BUTTON);
        assert_eq!(at(79, 24), VIDEO_BUTTON);
        assert_eq!(at(80, 24), -1);
        assert_eq!(at(92, 24), AREA_BUTTON);
        assert_eq!(at(139, 24), AREA_BUTTON);
        assert_eq!(at(323, 24), CLOSE_BUTTON);
        assert_eq!(at(324, 24), -1);
        // Above and below the buttons.
        assert_eq!(at(26, 7), -1);
        assert_eq!(at(26, 8), CAMERA_BUTTON);
        assert_eq!(at(26, 39), CAMERA_BUTTON);
        assert_eq!(at(26, 40), -1);
        // Every gap between groups is free for dragging.
        for x in [86, 144, 190, 234, 280] {
            assert_eq!(at(x, 24), -1, "x {x}");
        }
        // Outside the bar.
        assert_eq!(at(-1, 24), -1);
        assert_eq!(at(BAR_WIDTH as i32, 24), -1);
        assert_eq!(at(26, BAR_HEIGHT as i32), -1);
    }

    #[test]
    fn buttons_never_overlap_and_stay_inside_the_bar() {
        for scale in SCALES {
            let (width, height) = ((BAR_WIDTH * scale) as i32, (BAR_HEIGHT * scale) as i32);
            for y in 0..height {
                for x in 0..width {
                    let p = POINT { x, y };
                    let hits = (0..GLYPHS.len() as i32)
                        .filter(|&i| ui::contains(toolbar_button(i, scale), p))
                        .count();
                    assert!(hits <= 1, "{hits} buttons at {x},{y} on scale {scale}");
                }
            }
            for index in 0..GLYPHS.len() as i32 {
                let [l, t, r, b] = toolbar_button(index, scale);
                assert!(l >= 0.0 && t >= 0.0 && r <= width as f32 && b <= height as f32);
            }
        }
    }
}
