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
