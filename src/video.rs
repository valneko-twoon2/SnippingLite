//! Starting, counting down, framing and stopping screen recordings.

use super::*;

mod indicator;
pub(crate) use indicator::*;

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
    let folder = if state().save_video {
        record::video_folder()
    } else {
        record::temporary_folder()
    };
    let started = folder
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
/// WM_DPICHANGED: カウントダウンのあるモニターの拡大率が変わった。新しい倍率で作り直し、範囲の
/// 真ん中に置き直す。自分で置いたときにも届き、そのときは倍率が同じなので、何もしない。
unsafe fn rescale_count(hwnd: HWND, wp: WPARAM) {
    let scale = ui::monitors::message_scale(wp);
    if scale == state().scale {
        return;
    }
    state().scale = scale;
    let area = state().count_area;
    let size = (COUNT_SIZE * scale).round() as i32;
    SetWindowPos(
        hwnd,
        null_mut(),
        (area.left + area.right) / 2 - size / 2,
        (area.top + area.bottom) / 2 - size / 2,
        size,
        size,
        SWP_NOACTIVATE | SWP_NOZORDER,
    );
    InvalidateRect(hwnd, null(), 0);
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
        WM_DPICHANGED => {
            rescale_count(hwnd, wp);
            0
        }
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
            let copied = state().copy_video && imaging::copy_file(state().owner, &path);
            if frame.saved.is_some() {
                // 決めた名前に置けず別の名前で残した録画は、通知の設定にかかわらず場所を知らせる。
                toast::show_moved_recording(&frame.pixels, frame.width, frame.height, path.clone());
            }
            if state().copy_video && !copied {
                message(
                    state().owner,
                    tr("Could not copy the recording to the clipboard."),
                );
            } else if state().notify && frame.saved.is_none() {
                // 保存先に残さない録画は、クリップボードにコピーしたことを知らせる。
                let saved = state().save_video || !copied;
                toast::show_recording(&frame.pixels, frame.width, frame.height, path, saved);
            }
            if let Some(warning) = frame.warning {
                let text = tr("Recording ended early; the video up to that point was saved.");
                message(state().owner, &format!("{text}\n{warning}"));
            }
        }
        Err(error) => message(state().owner, &error),
    }
}
#[cfg(test)]
mod tests;

#[cfg(test)]
mod flows;
