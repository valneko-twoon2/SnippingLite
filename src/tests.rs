use super::*;
#[test]
fn selection_bounds_are_sorted_and_clipped() {
    let r = bounds(POINT { x: 140, y: 40 }, POINT { x: -10, y: 10 }, 100, 100);
    assert_eq!((r.left, r.top, r.right, r.bottom), (0, 10, 100, 40));
}

use crate::test_support::{answer_dialog, answer_menu, desk::Desk, stub_window};
use std::time::Duration;

fn s() -> &'static mut State {
    // SAFETY: the test owns the state and runs on the thread that created it.
    unsafe { state() }
}

fn shown(window: HWND) -> bool {
    // SAFETY: plain window query.
    unsafe { IsWindowVisible(window) != 0 }
}

#[test]
fn mouse_positions_are_signed_sixteen_bit_halves() {
    let lp = |x: i16, y: i16| ((y as u16 as isize) << 16) | (x as u16 as isize);
    for (x, y) in [
        (0, 0),
        (1, 2),
        (-1, -1),
        (-32768, 32767),
        (32767, -32768),
        (1919, 1079),
        (-1920, -300),
    ] {
        let p = point(lp(x, y));
        assert_eq!((p.x, p.y), (x as i32, y as i32));
    }
}

#[test]
fn rectangles_grow_by_the_same_amount_on_every_side() {
    let r = inflate(
        RECT {
            left: 10,
            top: 20,
            right: 30,
            bottom: 40,
        },
        5,
    );
    assert_eq!((r.left, r.top, r.right, r.bottom), (5, 15, 35, 45));
    let same = inflate(r, 0);
    assert_eq!((same.left, same.right), (5, 35));
    let shrunk = inflate(r, -5);
    assert_eq!(
        (shrunk.left, shrunk.top, shrunk.right, shrunk.bottom),
        (10, 20, 30, 40)
    );
}

#[test]
fn selections_are_sorted_and_stay_inside_the_screen() {
    // Equivalence: either corner first, and corners beyond the edges.
    for (a, b, expected) in [
        ((10, 20), (50, 60), (10, 20, 50, 60)),
        ((50, 60), (10, 20), (10, 20, 50, 60)),
        ((-5, -5), (200, 300), (0, 0, 100, 100)),
        ((40, 40), (40, 40), (40, 40, 40, 40)),
    ] {
        let r = bounds(POINT { x: a.0, y: a.1 }, POINT { x: b.0, y: b.1 }, 100, 100);
        assert_eq!((r.left, r.top, r.right, r.bottom), expected);
    }
}

#[test]
#[ignore = "Creates the tray window"]
fn starting_registers_the_tray_icon_the_shortcut_and_the_windows() {
    let desk = Desk::start();
    assert_eq!(s().owner, desk.main);
    assert!(!s().overlay.is_null() && !s().toolbar.is_null());
    // The overlay and the toolbar are made once and reused.
    let (overlay, toolbar) = (s().overlay, s().toolbar);
    // SAFETY: the windows belong to this test.
    assert!(unsafe { create_windows() });
    assert_eq!((s().overlay, s().toolbar), (overlay, toolbar));
    assert!(!s().tray_icon.is_null());
    assert_eq!(unsafe { find_window("SnippingLite.Nothing") }, null_mut());
}

#[test]
#[ignore = "Creates the tray window"]
fn new_settings_reach_the_running_tray_process() {
    let desk = Desk::start();
    let mut settings = config::parse_settings("");
    settings.copy = false;
    settings.save = true;
    settings.ask = true;
    settings.delay = 5;
    settings.history = false;
    settings.countdown = false;
    settings.skip_title = false;
    settings.record_cursor = false;
    settings.record_fps = 30;
    settings.freeze = true;
    settings.notify = false;
    settings.keep_toolbar = true;
    settings.outline = true;
    settings.outline_color = 1;
    settings.outline_width = 2;
    settings.mic = true;
    settings.system_audio = false;
    settings.app_audio = false;
    settings.copy_video = false;
    config::save_settings(&settings).unwrap();
    desk.send(desk.main, TRAY_RELOAD, 0, 0);
    let state = s();
    assert!(!state.copy && state.save && state.ask && !state.history);
    // 撮影の遅延とフレームレートは設定から外したので、ファイルにあっても使わない。
    assert_eq!((state.delay, state.record_fps), (0, 60));
    assert!(!state.countdown && !state.skip_title && !state.record_cursor);
    assert!(state.freeze && !state.notify && state.keep_toolbar && state.outline);
    assert_eq!((state.outline_color, state.outline_width), (1, 2));
    assert!(state.mic && !state.system_audio && !state.app_audio);
    assert!(!state.copy_video && state.save_video);
    // Switching "keep the toolbar" off closes an idle kept toolbar, but not one in use.
    state.toolbar_open = true;
    unsafe { ShowWindow(state.toolbar, SW_SHOWNOACTIVATE) };
    settings.keep_toolbar = false;
    config::save_settings(&settings).unwrap();
    desk.send(desk.main, TRAY_RELOAD, 0, 0);
    assert!(!s().toolbar_open && !shown(s().toolbar));
    s().toolbar_open = true;
    s().active = true;
    unsafe { ShowWindow(s().toolbar, SW_SHOWNOACTIVATE) };
    desk.send(desk.main, TRAY_RELOAD, 0, 0);
    assert!(!s().toolbar_open && shown(s().toolbar));
    s().active = false;
    unsafe { ShowWindow(s().toolbar, SW_HIDE) };
}

#[test]
#[ignore = "Creates the tray window"]
fn a_second_launch_opens_the_settings_of_the_running_tray() {
    let desk = Desk::start();
    let settings = stub_window(SETTINGS_WINDOW);
    // SAFETY: the windows belong to this test.
    unsafe { ShowWindow(settings, SW_HIDE) };
    // トレイが見つからなければ、自分がトレイとして起動する。
    assert!(!unsafe { hand_over_to(null_mut()) });
    desk.pump(Duration::from_millis(50));
    assert!(!shown(settings), "nothing was asked of anyone");
    // 見つかったトレイには設定ウィンドウを開かせ、自分は終わる。
    assert!(unsafe { hand_over_to(desk.main) });
    assert!(
        desk.pump_until(Duration::from_secs(2), || shown(settings)),
        "the running tray opened its settings"
    );
    assert!(!s().active, "it does not start a snip");
    unsafe { DestroyWindow(settings) };
}

#[test]
#[ignore = "Covers the screen"]
fn the_tray_commands_start_a_snip_and_open_the_windows() {
    let desk = Desk::start();
    let settings = stub_window(SETTINGS_WINDOW);
    let history = stub_window(HISTORY_WINDOW);
    for window in [settings, history] {
        // SAFETY: the windows belong to this test.
        unsafe { ShowWindow(window, SW_HIDE) };
    }
    desk.send(desk.main, WM_COMMAND, TRAY_SETTINGS, 0);
    assert!(shown(settings) && !shown(history));
    desk.send(desk.main, WM_COMMAND, TRAY_HISTORY, 0);
    assert!(shown(history));
    // Stopping with nothing recorded, and unknown commands, do nothing.
    desk.send(desk.main, WM_COMMAND, TRAY_STOP, 0);
    desk.send(desk.main, WM_COMMAND, 9999, 0);
    assert!(!s().active);
    desk.send(desk.main, WM_COMMAND, NEW, 0);
    assert!(s().active);
    desk.key(s().overlay, 0x1b);
    // The shortcut and a left click on the icon start a snip too.
    desk.send(desk.main, WM_HOTKEY, HOTKEY as usize, 0);
    assert!(s().active);
    desk.key(s().overlay, 0x1b);
    desk.send(desk.main, TRAY_MESSAGE, 1, WM_LBUTTONUP as isize);
    assert!(s().active);
    desk.key(s().overlay, 0x1b);
    // Another hotkey id is not ours.
    desk.send(desk.main, WM_HOTKEY, 99, 0);
    assert!(!s().active);
    unsafe {
        DestroyWindow(settings);
        DestroyWindow(history);
    }
}

#[test]
#[ignore = "Shows a menu"]
fn the_tray_menu_offers_the_app_about_and_exit() {
    let desk = Desk::start();
    let settings = stub_window(SETTINGS_WINDOW);
    unsafe { ShowWindow(settings, SW_HIDE) };
    crate::test_support::take_shell_opened();
    // Menu entries without a recording: Snipping Lite, About, Exit.
    let answer = answer_menu(0);
    desk.send(desk.main, TRAY_MESSAGE, 1, WM_RBUTTONUP as isize);
    assert!(answer.join().unwrap());
    // The menu posts its command, so it arrives with the next messages.
    assert!(
        desk.pump_until(Duration::from_secs(2), || shown(settings)),
        "Snipping Lite opens the window"
    );
    let answer = answer_menu(1);
    desk.send(desk.main, TRAY_MESSAGE, 1, WM_RBUTTONUP as isize);
    assert!(answer.join().unwrap());
    assert!(
        desk.pump_until(Duration::from_secs(2), || {
            !crate::test_support::take_shell_opened().is_empty()
        }),
        "About opens the page"
    );
    // Leaving the menu does nothing.
    let answer = answer_menu(usize::MAX);
    desk.send(desk.main, TRAY_MESSAGE, 1, WM_RBUTTONUP as isize);
    assert!(answer.join().unwrap());
    assert!(!s().active);
    // Other mouse messages from the icon are ignored.
    desk.send(desk.main, TRAY_MESSAGE, 1, WM_MOUSEMOVE as isize);
    unsafe { DestroyWindow(settings) };
}

#[test]
#[ignore = "Creates the tray window"]
fn the_about_command_opens_the_page_in_the_browser() {
    let desk = Desk::start();
    crate::test_support::take_shell_opened();
    desk.send(desk.main, WM_COMMAND, TRAY_ABOUT, 0);
    let opened = crate::test_support::take_shell_opened();
    assert_eq!(opened.len(), 1);
    assert!(opened[0].0.ends_with("about.html"), "{:?}", opened[0]);
}
#[test]
#[ignore = "Covers the screen"]
fn the_delay_timer_takes_the_picture_and_closing_only_hides() {
    let desk = Desk::start();
    s().pending = true;
    desk.send(desk.main, WM_TIMER, DELAY_TIMER, 0);
    assert!(!s().pending && s().active);
    desk.key(s().overlay, 0x1b);
    // Other timers are left to Windows.
    desk.send(desk.main, WM_TIMER, 77, 0);
    assert!(!s().active);
    assert_eq!(desk.send(desk.main, WM_CLOSE, 0, 0), 0);
    assert!(!shown(desk.main));
}

#[test]
#[ignore = "Creates the tray window"]
fn exiting_removes_the_tray_icon_and_ends_the_message_loop() {
    let desk = Desk::start();
    desk.send(desk.main, WM_COMMAND, TRAY_EXIT, 0);
    assert!(unsafe { IsWindow(desk.main) } == 0);
    assert!(s().tray_icon.is_null(), "the icon is released");
    assert!(!unsafe { toast::visible() });
}

#[test]
#[ignore = "Shows a message box"]
fn a_message_box_has_the_app_name_and_waits_for_a_click() {
    let desk = Desk::start();
    let answer = answer_dialog(2);
    // SAFETY: the window belongs to this test.
    unsafe { message(desk.main, "Something happened") };
    assert!(answer.join().unwrap());
}

#[test]
#[ignore = "Creates a process"]
fn opening_a_window_that_is_not_running_starts_another_copy_of_the_program() {
    let mut desk = Desk::start();
    // The test program stands in for the program: it exits on the unknown option.
    // Its coverage data must not replace that of the tests, so it gets a file of its own.
    let profile = desk.sandbox.path().join("child.profraw");
    desk.sandbox.set("LLVM_PROFILE_FILE", profile);
    // SAFETY: the state belongs to this test.
    unsafe {
        launch_settings();
        launch_history(true);
        s().hotkey_ok = true;
        launch_history(false);
    }
    desk.pump(Duration::from_millis(100));
}

#[test]
fn only_a_window_flag_makes_a_window_process() {
    let args = |list: &[&str]| list.iter().map(|a| (*a).to_owned()).collect::<Vec<_>>();
    assert!(!run_window_process(&args(&["snippinglite.exe"])));
    assert!(!run_window_process(&args(&[
        "snippinglite.exe",
        "--latest"
    ])));
    // A player without a file has nothing to show and ends at once.
    assert!(run_window_process(&args(&["snippinglite.exe", "--player"])));
}
