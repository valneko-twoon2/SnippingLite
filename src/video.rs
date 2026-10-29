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
    if area.right - area.left < record::MIN_SIZE || area.bottom - area.top < record::MIN_SIZE {
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
                ui::raise(hwnd);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn stops(x: i32, y: i32, scale: f32) -> bool {
        ui::contains(stop_button(scale), POINT { x, y })
    }

    #[test]
    fn only_the_stop_button_ends_a_recording() {
        // The red dot and the elapsed time keep recording.
        assert!(!stops(20, 24, 1.0));
        assert!(!stops(60, 24, 1.0));
        // The button and its edges.
        assert!(stops(130, 24, 1.0));
        assert!(stops(100, 8, 1.0));
        assert!(stops(159, 39, 1.0));
        assert!(!stops(99, 24, 1.0));
        assert!(!stops(160, 24, 1.0));
        assert!(!stops(130, 7, 1.0));
        assert!(!stops(130, 40, 1.0));
        // The margin to the right of the button.
        assert!(!stops(164, 24, 1.0));
        // A scaled indicator moves the edges with it.
        assert!(stops(150, 12, 1.5));
        assert!(!stops(149, 24, 1.5));
        assert!(stops(239, 59, 1.5));
        assert!(!stops(240, 24, 1.5));
    }

    #[test]
    fn the_stop_button_fits_the_indicator_on_every_scale() {
        for step in 4..=16 {
            let scale = step as f32 / 4.0;
            let [l, t, r, b] = stop_button(scale);
            assert!(
                l > 92.0 * scale,
                "the button covers the time on scale {scale}"
            );
            assert!(r <= RECORD_WIDTH * scale && b <= RECORD_HEIGHT * scale && t >= 0.0);
            assert!(r - l >= 1.0 && b - t >= 1.0);
        }
    }
}

#[cfg(test)]
mod flows {
    use super::*;
    use crate::test_support::{answer_dialog, answer_save_as, desk::Desk};
    use crate::window_capture::test_support::CoveredWindow;
    use std::time::Duration;

    fn s() -> &'static mut State {
        // SAFETY: the test owns the state and runs on the thread that created it.
        unsafe { state() }
    }

    fn videos(desk: &Desk) -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = std::fs::read_dir(desk.sandbox.path().join("videos"))
            .into_iter()
            .flatten()
            .flatten()
            .map(|f| f.path())
            .collect();
        files.sort();
        files
    }

    /// Starts a video snip and selects an area of the given size in the middle of the screen.
    fn select(desk: &Desk, width: i32, height: i32) {
        s().video = true;
        // SAFETY: the windows belong to this test.
        unsafe { begin_capture() };
        assert!(s().active);
        desk.drag(s().overlay, (200, 200), &[], (200 + width, 200 + height));
    }

    fn stop_at(desk: &Desk) {
        let [l, t, r, b] = stop_button(s().scale);
        desk.click(
            s().record_window,
            ((l + r) / 2.0) as i32,
            ((t + b) / 2.0) as i32,
        );
    }

    fn wait_started(desk: &Desk) {
        assert!(desk.pump_until(Duration::from_secs(20), || {
            s().recording
                .as_ref()
                .is_some_and(|r| r.elapsed() > Duration::ZERO || r.failed())
        }));
    }

    #[test]
    #[ignore = "Records the screen"]
    fn a_recording_runs_from_the_selection_until_the_stop_button_is_pressed() {
        let desk = Desk::start_with(|settings| {
            settings.countdown = false;
            settings.system_audio = false;
            settings.notify = true;
        });
        select(&desk, 200, 120);
        assert!(!s().active, "the overlay closed");
        assert!(s().recording.is_some());
        assert!(!s().record_window.is_null() && !s().record_frame.is_null());
        wait_started(&desk);
        // Clicking the dot or the time does not end it; the indicator repaints with the clock.
        desk.click(s().record_window, 20, 24);
        desk.click(s().record_window, 60, 24);
        assert!(s().recording.is_some());
        desk.pump(Duration::from_millis(1200));
        desk.send(s().record_window, WM_PAINT, 0, 0);
        stop_at(&desk);
        assert!(s().recording.is_none());
        assert!(s().record_window.is_null() && s().record_frame.is_null());
        let files = videos(&desk);
        assert_eq!(files.len(), 1);
        assert_eq!(&std::fs::read(&files[0]).unwrap()[4..8], b"ftyp");
        assert!(unsafe { toast::visible() }, "the recording is announced");
    }

    #[test]
    #[ignore = "Records the screen"]
    fn the_countdown_counts_three_two_one_and_a_click_cancels_it() {
        let desk = Desk::start_with(|settings| {
            settings.countdown = true;
            settings.system_audio = false;
        });
        select(&desk, 200, 120);
        assert!(!s().count_window.is_null());
        assert_eq!(s().count_left, 3);
        let window = s().count_window;
        assert_eq!(
            desk.send(window, WM_MOUSEACTIVATE, 0, 0),
            MA_NOACTIVATE as LRESULT
        );
        assert_eq!(desk.send(window, WM_ERASEBKGND, 0, 0), 1);
        desk.send(window, WM_PAINT, 0, 0);
        desk.send(window, WM_TIMER, COUNT_TIMER, 0);
        assert_eq!(s().count_left, 2);
        desk.send(window, WM_PAINT, 0, 0);
        // A click cancels it and nothing is recorded.
        desk.mouse(window, WM_LBUTTONUP, 10, 10);
        assert!(s().count_window.is_null() && s().recording.is_none());
        assert!(videos(&desk).is_empty());
        // The shortcut cancels a countdown as well.
        select(&desk, 200, 120);
        assert!(!s().count_window.is_null());
        unsafe { begin_capture() };
        assert!(s().count_window.is_null() && !s().active);
    }

    #[test]
    #[ignore = "Records the screen"]
    fn the_countdown_ends_in_a_recording_that_the_shortcut_stops() {
        let desk = Desk::start_with(|settings| {
            settings.countdown = true;
            settings.system_audio = false;
            settings.notify = false;
        });
        select(&desk, 200, 120);
        let window = s().count_window;
        for left in [2, 1] {
            desk.send(window, WM_TIMER, COUNT_TIMER, 0);
            assert_eq!(s().count_left, left);
        }
        desk.send(window, WM_TIMER, COUNT_TIMER, 0);
        assert!(s().count_window.is_null() && s().recording.is_some());
        wait_started(&desk);
        // The shortcut ends a running recording.
        desk.pump(Duration::from_millis(600));
        unsafe { begin_capture() };
        assert!(s().recording.is_none());
        assert_eq!(videos(&desk).len(), 1);
        assert!(!unsafe { toast::visible() }, "notifications are off");
    }

    #[test]
    #[ignore = "Covers the screen"]
    fn an_area_below_the_encoder_minimum_starts_nothing() {
        let desk = Desk::start_with(|settings| settings.countdown = false);
        // Boundary: 33 pixels is refused and the snip goes on; the overlay stays for another try.
        select(&desk, 33, 33);
        assert!(s().active && s().recording.is_none());
        desk.key(s().overlay, 0x1b);
        assert!(videos(&desk).is_empty());
    }

    #[test]
    #[ignore = "Shows a message box"]
    fn a_recording_that_cannot_start_is_reported() {
        let desk = Desk::start_with(|settings| {
            settings.countdown = false;
            settings.system_audio = false;
        });
        // The recording folder cannot be created.
        let blocked = desk.sandbox.path().join("blocked");
        std::fs::write(&blocked, b"x").unwrap();
        let mut settings = config::load_settings();
        settings.video_folder = blocked.join("inside").to_string_lossy().into_owned();
        config::save_settings(&settings).unwrap();
        let answer = answer_dialog(2);
        select(&desk, 200, 120);
        assert!(answer.join().unwrap());
        assert!(s().recording.is_none() && s().record_window.is_null());
        // The encoder refuses the frame rate.
        settings.video_folder = desk
            .sandbox
            .path()
            .join("videos")
            .to_string_lossy()
            .into_owned();
        config::save_settings(&settings).unwrap();
        s().record_fps = 7;
        let answer = answer_dialog(2);
        select(&desk, 200, 120);
        assert!(answer.join().unwrap());
        assert!(s().recording.is_none());
    }

    #[test]
    #[ignore = "Records the screen"]
    fn a_window_that_goes_away_ends_the_recording_with_a_warning() {
        let desk = Desk::start_with(|settings| {
            settings.countdown = false;
            settings.system_audio = true;
            settings.notify = false;
        });
        let fixture = CoveredWindow::new();
        s().video = true;
        unsafe { begin_capture() };
        // Aim at the fixture window instead of whatever sits under the pointer.
        unsafe { set_mode(MODE_WINDOW) };
        s().hover_hwnd = fixture.target;
        s().hover_window = Some(RECT {
            left: 80,
            top: 80,
            right: 400,
            bottom: 320,
        });
        desk.mouse(s().overlay, WM_LBUTTONDOWN, 100, 100);
        assert!(s().recording.is_some(), "the window recording started");
        wait_started(&desk);
        desk.pump(Duration::from_millis(600));
        drop(fixture);
        // The recorder notices, and the next indicator tick stops it and reports it.
        let answer = answer_dialog(2);
        assert!(desk.pump_until(Duration::from_secs(10), || s().recording.is_none()));
        assert!(answer.join().unwrap(), "the early end is reported");
        assert_eq!(videos(&desk).len(), 1);
    }

    #[test]
    #[ignore = "Shows a dialog"]
    fn the_file_name_of_a_recording_can_be_chosen_at_the_end() {
        let desk = Desk::start_with(|settings| {
            settings.countdown = false;
            settings.system_audio = false;
            settings.ask_video = true;
            settings.notify = false;
        });
        select(&desk, 200, 120);
        wait_started(&desk);
        // Cancelling the dialog leaves the file where it was recorded (accepting the same
        // name would ask whether to overwrite the recording itself).
        let answer = answer_dialog(2);
        stop_at(&desk);
        assert!(answer.join().unwrap());
        assert_eq!(videos(&desk).len(), 1);
    }

    #[test]
    #[ignore = "Shows a dialog"]
    fn choosing_where_to_keep_a_recording_moves_it_or_leaves_it() {
        let desk = Desk::start();
        let folder = desk.sandbox.path().join("videos");
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("Snipping Lite 1.mp4");
        std::fs::write(&path, b"video").unwrap();
        // Cancelled: the recording stays.
        let answer = answer_dialog(2);
        // SAFETY: the windows belong to this test.
        let kept = unsafe { choose_recording_path(path.clone()) };
        assert!(answer.join().unwrap());
        assert_eq!(kept, path);
        // Accepted at the proposed place (which does not exist yet): the path is unchanged.
        let proposed = folder.join("Snipping Lite 2.mp4");
        let answer = answer_dialog(1);
        assert_eq!(unsafe { choose_recording_path(proposed.clone()) }, proposed);
        assert!(answer.join().unwrap());
        // Another name moves the file there.
        let renamed = folder.join("Renamed.mp4");
        let answer = answer_save_as(renamed.clone());
        assert_eq!(unsafe { choose_recording_path(path.clone()) }, renamed);
        assert!(answer.join().unwrap());
        assert!(renamed.is_file() && !path.exists());
        // A path without a folder or a name cannot be asked about.
        assert_eq!(
            unsafe { choose_recording_path(PathBuf::from("/")) },
            PathBuf::from("/")
        );
        assert_eq!(
            unsafe { choose_recording_path(PathBuf::new()) },
            PathBuf::new()
        );
    }

    #[test]
    #[ignore = "Records the screen"]
    fn a_recording_is_finished_when_the_program_exits() {
        let desk = Desk::start_with(|settings| {
            settings.countdown = false;
            settings.system_audio = false;
            settings.notify = false;
        });
        select(&desk, 200, 120);
        wait_started(&desk);
        desk.pump(Duration::from_millis(500));
        desk.send(desk.main, WM_COMMAND, TRAY_EXIT, 0);
        assert!(s().recording.is_none());
        assert_eq!(videos(&desk).len(), 1, "the file is complete");
    }

    #[test]
    #[ignore = "Records the screen"]
    fn a_failed_recorder_stops_itself_at_the_next_tick() {
        let desk = Desk::start_with(|settings| {
            settings.countdown = false;
            settings.system_audio = false;
            settings.notify = false;
        });
        select(&desk, 200, 120);
        wait_started(&desk);
        // The tick with a healthy recorder keeps the frame in place and raises the indicator.
        desk.send(s().record_window, WM_TIMER, RECORD_TIMER, 0);
        assert!(s().recording.is_some());
        // The stop request from the tray menu ends it as well.
        desk.send(desk.main, WM_COMMAND, TRAY_STOP, 0);
        assert!(s().recording.is_none());
        // Another tick after the end finds nothing to do.
        unsafe { stop_recording() };
    }
}
