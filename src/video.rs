//! Starting, counting down, framing and stopping screen recordings.

use super::*;

/// Recording indicator in DIPs: a red dot, the elapsed time and a Stop label.
const RECORD_WIDTH: f32 = 168.0;
const RECORD_HEIGHT: f32 = 48.0;
/// The Stop button of the recording indicator; only a click on it ends the recording.
fn stop_button(scale: f32) -> [f32; 4] {
    [100.0 * scale, 8.0 * scale, 160.0 * scale, 40.0 * scale]
}
/// Sends a selected shape to the active screenshot or recording destination.
pub(crate) unsafe fn complete_selection(rect: RECT, freeform: bool) {
    if state().video {
        start_recording(rect, freeform);
    } else {
        finish(rect, freeform);
    }
}

/// Starts recording a selected shape (overlay coordinates).
unsafe fn start_recording(rect: RECT, freeform: bool) {
    let points = if freeform {
        state()
            .points
            .iter()
            .map(|p| POINT {
                x: p.x - rect.left,
                y: p.y - rect.top,
            })
            .collect()
    } else {
        Vec::new()
    };
    let origin = state().origin;
    let mut area = RECT {
        left: rect.left + origin.x,
        top: rect.top + origin.y,
        right: rect.right + origin.x,
        bottom: rect.bottom + origin.y,
    };
    let target = (state().mode == MODE_WINDOW)
        .then(|| window_capture::Target::new(state().hover_hwnd, state().skip_title));
    if let Some(window) = target {
        match window.bounds() {
            Ok(bounds) => area = bounds,
            Err(_) => {
                cancel();
                message(state().owner, tr("Could not capture the selected window."));
                return;
            }
        }
    }
    if area.right - area.left < 16 || area.bottom - area.top < 16 {
        refresh_overlay();
        return;
    }
    // The overlay closes first; our own windows, including a kept toolbar, stay
    // hidden until the recording ends.
    let hidden = std::mem::take(&mut state().hidden);
    let open = std::mem::take(&mut state().toolbar_open);
    cancel();
    state().hidden = hidden;
    state().toolbar_open = open;
    if state().countdown {
        begin_countdown(area, points, target);
    } else {
        begin_recording(area, points, target);
    }
}
unsafe fn begin_recording(area: RECT, points: Vec<POINT>, target: Option<window_capture::Target>) {
    DwmFlush();
    let started = record::video_folder()
        .map_err(|_| tr("Could not create the recordings folder.").to_string())
        .and_then(|folder| {
            let name = imaging::timestamp().map_err(|e| e.to_string())?;
            record::start(
                area,
                folder.join(format!("Snipping Lite {name}.mp4")),
                points,
                record::Options {
                    fps: state().record_fps,
                    cursor: state().record_cursor,
                    window: target,
                    audio: record::Audio {
                        loopback: state().system_audio.then(|| {
                            match target.and_then(|window| window.process_id()) {
                                Some(id) if state().app_audio => record::Loopback::Process(id),
                                _ => record::Loopback::System,
                            }
                        }),
                        mic: state().mic,
                    },
                },
            )
        });
    match started {
        Ok(recording) => {
            let area = recording.display_area().unwrap_or(area);
            state().recording = Some(recording);
            let center = POINT {
                x: (area.left + area.right) / 2,
                y: (area.top + area.bottom) / 2,
            };
            show_indicator(center);
            show_frame(area);
        }
        Err(error) => {
            restore_windows();
            message(state().owner, &error);
        }
    }
}
/// A 3, 2, 1 countdown in the middle of the area; a click cancels it.
const COUNT_SIZE: f32 = 120.0;
unsafe fn begin_countdown(area: RECT, points: Vec<POINT>, target: Option<window_capture::Target>) {
    let center = POINT {
        x: (area.left + area.right) / 2,
        y: (area.top + area.bottom) / 2,
    };
    let theme = ui::theme();
    let scale = ui::scale_at(center);
    let size = (COUNT_SIZE * scale).round() as i32;
    let hwnd = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
        w(COUNT).as_ptr(),
        w(COUNT).as_ptr(),
        WS_POPUP,
        center.x - size / 2,
        center.y - size / 2,
        size,
        size,
        null_mut(),
        null_mut(),
        GetModuleHandleW(null()),
        null(),
    );
    if hwnd.is_null() {
        begin_recording(area, points, target);
        return;
    }
    SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);
    ui::acrylic(hwnd, theme.dark);
    ui::disable_transitions(hwnd);
    let s = state();
    s.theme = theme;
    s.scale = scale;
    s.count_window = hwnd;
    s.count_left = 3;
    s.count_area = area;
    s.count_target = target;
    s.count_points = points;
    ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    SetTimer(hwnd, COUNT_TIMER, 1000, None);
}
pub(crate) unsafe fn cancel_countdown() {
    state().count_points.clear();
    state().count_target = None;
    let window = std::mem::replace(&mut state().count_window, null_mut());
    if !window.is_null() {
        DestroyWindow(window);
    }
    restore_windows();
}
pub(crate) unsafe extern "system" fn count_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let dc = BeginPaint(hwnd, &mut ps);
            let mut client: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut client);
            if let Some(mut canvas) = ui::Canvas::new(client.right, client.bottom) {
                let s = state();
                let font = ui::Font::new(ui::TEXT_FONT, 56.0 * s.scale, FW_NORMAL as i32);
                canvas.text(
                    &s.count_left.to_string(),
                    &font,
                    client,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                    s.theme.text,
                );
                canvas.present(dc);
            }
            EndPaint(hwnd, &ps);
            0
        }
        WM_TIMER if wp == COUNT_TIMER => {
            state().count_left -= 1;
            if state().count_left <= 0 {
                let area = state().count_area;
                let points = std::mem::take(&mut state().count_points);
                let target = state().count_target.take();
                state().count_window = null_mut();
                DestroyWindow(hwnd);
                begin_recording(area, points, target);
            } else {
                InvalidateRect(hwnd, null(), 0);
            }
            0
        }
        WM_LBUTTONUP => {
            cancel_countdown();
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
/// Outlines the recorded area just outside of it; the frame is not captured.
unsafe fn show_frame(area: RECT) {
    let scale = ui::scale_at(POINT {
        x: (area.left + area.right) / 2,
        y: (area.top + area.bottom) / 2,
    });
    let thickness = (2.0 * scale).round().max(2.0) as i32;
    let (width, height) = (
        area.right - area.left + 2 * thickness,
        area.bottom - area.top + 2 * thickness,
    );
    let previous = state().record_frame;
    if !previous.is_null() {
        let mut bounds: RECT = std::mem::zeroed();
        GetWindowRect(previous, &mut bounds);
        if bounds.left == area.left - thickness
            && bounds.top == area.top - thickness
            && bounds.right == area.right + thickness
            && bounds.bottom == area.bottom + thickness
        {
            return;
        }
    }
    let hwnd = if previous.is_null() {
        CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
            w(FRAME).as_ptr(),
            w(FRAME).as_ptr(),
            WS_POPUP,
            area.left - thickness,
            area.top - thickness,
            width,
            height,
            null_mut(),
            null_mut(),
            GetModuleHandleW(null()),
            null(),
        )
    } else {
        previous
    };
    if hwnd.is_null() {
        return;
    }
    SetWindowPos(
        hwnd,
        null_mut(),
        area.left - thickness,
        area.top - thickness,
        width,
        height,
        SWP_NOACTIVATE | SWP_NOZORDER,
    );
    let outer = CreateRectRgn(0, 0, width, height);
    let inner = CreateRectRgn(thickness, thickness, width - thickness, height - thickness);
    CombineRgn(outer, outer, inner, RGN_DIFF);
    DeleteObject(inner);
    // The window takes ownership of the region.
    SetWindowRgn(hwnd, outer, 0);
    SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);
    ui::disable_transitions(hwnd);
    state().record_frame = hwnd;
    ShowWindow(hwnd, SW_SHOWNOACTIVATE);
}
unsafe fn show_indicator(center: POINT) {
    let theme = ui::theme();
    let scale = ui::scale_at(center);
    let mut info: MONITORINFO = std::mem::zeroed();
    info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    GetMonitorInfoW(
        MonitorFromPoint(center, MONITOR_DEFAULTTONEAREST),
        &mut info,
    );
    let width = (RECORD_WIDTH * scale).round() as i32;
    let height = (RECORD_HEIGHT * scale).round() as i32;
    let hwnd = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
        w(RECORD).as_ptr(),
        w(RECORD).as_ptr(),
        WS_POPUP,
        (info.rcMonitor.left + info.rcMonitor.right - width) / 2,
        info.rcMonitor.top + (12.0 * scale).round() as i32,
        width,
        height,
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
/// Asks where to keep a finished recording; cancelling leaves it in the recording folder.
unsafe fn choose_recording_path(path: PathBuf) -> PathBuf {
    let (Some(folder), Some(name)) = (path.parent(), path.file_name().and_then(|n| n.to_str()))
    else {
        return path;
    };
    let Some(target) = imaging::choose_video_path(null_mut(), folder, name) else {
        return path;
    };
    let moved = target == path
        || std::fs::rename(&path, &target).is_ok()
        || (std::fs::copy(&path, &target).is_ok() && std::fs::remove_file(&path).is_ok());
    if moved {
        target
    } else {
        path
    }
}
/// Ends the recording, restores our windows and announces the file.
pub(crate) unsafe fn stop_recording() {
    let Some(recording) = state().recording.take() else {
        return;
    };
    for window in [
        std::mem::replace(&mut state().record_window, null_mut()),
        std::mem::replace(&mut state().record_frame, null_mut()),
    ] {
        if !window.is_null() {
            DestroyWindow(window);
        }
    }
    let result = recording.stop();
    restore_windows();
    match result {
        Ok((path, frame)) => {
            let path = if state().ask_video {
                choose_recording_path(path)
            } else {
                path
            };
            if state().notify {
                toast::show_recording(&frame.pixels, frame.width, frame.height, path);
            }
            if let Some(warning) = frame.warning {
                let text = tr("Recording ended early; the video up to that point was saved.");
                message(state().owner, &format!("{text}\n{warning}"));
            }
        }
        Err(error) => message(state().owner, &error),
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
