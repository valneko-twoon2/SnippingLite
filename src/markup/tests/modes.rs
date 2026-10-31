//! 切り取りのモードごとの書き込みの始まり。README: 選んだスクリーンショットはバーとともに画面に残り、
//! 確定するといつもどおり届く。枠線は四角形、ウィンドウ、全画面に付き、自由形式には付かない。
//! 録画には書き込まない。
//!
//! | モード | 書き込む範囲 | 境界線（`shaped`） | テスト |
//! |---|---|---|---|
//! | 四角形 | 選んだ範囲 | 付く | 既存 `a_marked_up_snip_is_drawn_on_undone_erased_and_captured` |
//! | ウィンドウ | 対象のウィンドウの範囲。上に重なった窓は入らない | 付く | `a_window_snip_is_marked_up_over_the_window_alone` |
//! | ウィンドウ（画面の左からはみ出す） | はみ出した分も含めて、窓のあった所 | - | `a_window_hanging_off_the_left_of_the_screen_is_marked_up_where_it_is` |
//! | 全画面 | 画面全体。バーは画面の内側 | 付く | `a_full_screen_snip_is_marked_up_over_the_whole_screen` |
//! | 自由形式 | 形を囲む範囲。形の外は透明 | 付かない | `a_freeform_snip_is_marked_up_as_a_shape_without_a_border` |
//! | 録画（どのモードでも） | 書き込まない | - | `a_recording_selection_is_not_marked_up` |

use super::*;
use crate::window_capture::{test_support::CoveredWindow, Target};

/// ペンのボタンをオンにして切り取りを始める。範囲はまだ選ばない。
fn begin(desk: &Desk) {
    s().markup = true;
    // SAFETY: the windows belong to this test.
    unsafe { begin_capture() };
    assert!(s().active, "the snip started");
    desk.pump(Duration::from_millis(30));
}

fn session() -> &'static Session {
    s().markup_session.as_deref().expect("marking up")
}

fn edges(r: RECT) -> (i32, i32, i32, i32) {
    (r.left, r.top, r.right, r.bottom)
}

/// 書き込みが始まり、バーが出て、ツールバーは隠れている。
fn assert_marking_up() {
    assert!(s().active && s().markup_session.is_some(), "marking up");
    assert!(
        shown(s().overlay) && shown(s().markup_bar),
        "the snip and its bar"
    );
    assert!(!shown(s().toolbar), "the toolbar makes way for the bar");
}

fn pixel(pixels: &[u8], width: i32, x: i32, y: i32) -> [u8; 4] {
    let at = ((y * width + x) * 4) as usize;
    [pixels[at], pixels[at + 1], pixels[at + 2], pixels[at + 3]]
}

/// 設定の枠線の色（BGRA）。
fn border() -> [u8; 4] {
    let [r, g, b] = annotate::COLORS[s().outline_color];
    [b, g, r, 255]
}

/// Enter で確定し、履歴に残った画像を返す。
fn capture_with_enter(desk: &Desk) -> (i32, i32, Vec<u8>) {
    desk.key(s().overlay, VK_RETURN);
    assert!(!s().active && s().markup_session.is_none(), "captured");
    let entry = crate::history::list().remove(0);
    imaging::decode(&entry.image).unwrap()
}

#[test]
#[ignore = "Covers the screen"]
fn a_window_snip_is_marked_up_over_the_window_alone() {
    let desk = Desk::start_with(|settings| {
        settings.copy = false;
        settings.history = true;
        settings.notify = false;
        settings.outline = true;
    });
    // 緑の窓の上に、赤い最前面の窓が重なっている。
    let fixture = CoveredWindow::new();
    begin(&desk);
    // SAFETY: the windows belong to this test.
    unsafe { set_mode(MODE_WINDOW) };
    // ポインターの下の窓は机の上の様子で変わるので、対象の窓を選んだことにする。
    let bounds = Target::new(fixture.target, false).bounds().unwrap();
    let origin = s().origin;
    let expected = rect(
        bounds.left - origin.x,
        bounds.top - origin.y,
        bounds.right - origin.x,
        bounds.bottom - origin.y,
    );
    s().hover_hwnd = fixture.target;
    s().hover_window = Some(expected);
    desk.mouse(
        s().overlay,
        WM_LBUTTONDOWN,
        expected.left + 10,
        expected.top + 10,
    );
    assert_marking_up();
    let session = session();
    assert_eq!(edges(session.area), edges(expected), "where the window is");
    assert!(!session.shaped, "a window has straight edges");
    // 重なった赤い窓ではなく、緑の窓そのもの。
    let [b, g, r, _] = pixel(
        &session.pixels,
        session.width,
        session.width / 2,
        session.height / 2,
    );
    assert!(
        u16::from(g) > u16::from(b) + 60 && u16::from(g) > u16::from(r) + 60,
        "{:?}",
        [b, g, r]
    );
    let (width, height, pixels) = capture_with_enter(&desk);
    assert_eq!(
        (width, height),
        (bounds.right - bounds.left, bounds.bottom - bounds.top)
    );
    assert_eq!(pixel(&pixels, width, 0, 0), border(), "the border is drawn");
    drop(fixture);
}

#[test]
#[ignore = "Covers the screen"]
fn a_window_hanging_off_the_left_of_the_screen_is_marked_up_where_it_is() {
    let desk = Desk::start_with(|settings| {
        settings.copy = false;
        settings.notify = false;
    });
    let fixture = CoveredWindow::new();
    // SAFETY: the windows belong to this test; plain monitor queries.
    unsafe {
        // いちばん左の画面の左端から 100 ピクセルはみ出させる。
        let left = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        let edge = POINT {
            x: left,
            y: GetSystemMetrics(SM_YVIRTUALSCREEN),
        };
        GetMonitorInfoW(MonitorFromPoint(edge, MONITOR_DEFAULTTONEAREST), &mut info);
        let top = info.rcMonitor.top + 100;
        SetWindowPos(
            fixture.target,
            HWND_TOP,
            left - 100,
            top,
            320,
            240,
            SWP_NOACTIVATE,
        );
        DwmFlush();
    }
    begin(&desk);
    unsafe { set_mode(MODE_WINDOW) };
    let bounds = Target::new(fixture.target, false).bounds().unwrap();
    let origin = s().origin;
    assert!(bounds.left < origin.x, "the window hangs off the screen");
    // ポインターの下の窓は机の上の様子で変わるので、窓を選んだことにする。強調の枠は、
    // ポインターで選んだときと同じく、画面の端で切り詰めたもの。
    s().hover_hwnd = fixture.target;
    s().hover_window = Some(rect(
        0,
        bounds.top - origin.y,
        bounds.right - origin.x,
        bounds.bottom - origin.y,
    ));
    desk.mouse(s().overlay, WM_LBUTTONDOWN, 10, bounds.top - origin.y + 10);
    assert_marking_up();
    // 切り取った窓は、はみ出した分も含めて、窓のあった所に残る（左上を画面の端に詰めない）。
    let session = session();
    assert_eq!(
        edges(session.area),
        (
            bounds.left - origin.x,
            bounds.top - origin.y,
            bounds.right - origin.x,
            bounds.bottom - origin.y
        ),
        "the snip stays where the window is"
    );
    drop(fixture);
}

#[test]
#[ignore = "Covers the screen"]
fn a_full_screen_snip_is_marked_up_over_the_whole_screen() {
    let desk = Desk::start_with(|settings| {
        settings.copy = false;
        settings.history = true;
        settings.notify = false;
        settings.outline = true;
    });
    begin(&desk);
    let (width, height) = (s().width, s().height);
    // SAFETY: the windows belong to this test.
    unsafe { set_mode(MODE_SCREEN) };
    assert_marking_up();
    let session = session();
    assert_eq!(edges(session.area), (0, 0, width, height));
    assert!(!session.shaped);
    // 範囲の下にも上にも入らないので、バーは画面の作業領域（タスクバーを除く）の内側に出る。
    // SAFETY: plain window and monitor queries.
    let (bar, work) = unsafe {
        let mut bar: RECT = std::mem::zeroed();
        GetWindowRect(s().markup_bar, &mut bar);
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        GetMonitorInfoW(
            MonitorFromWindow(s().markup_bar, MONITOR_DEFAULTTONEAREST),
            &mut info,
        );
        (edges(bar), edges(info.rcWork))
    };
    assert!(
        bar.0 >= work.0 && bar.1 >= work.1 && bar.2 <= work.2 && bar.3 <= work.3,
        "the bar {bar:?} is inside {work:?}"
    );
    let (w, h, pixels) = capture_with_enter(&desk);
    assert_eq!((w, h), (width, height));
    assert_eq!(pixel(&pixels, w, 0, 0), border(), "the border is drawn");
}

#[test]
#[ignore = "Covers the screen"]
fn a_freeform_snip_is_marked_up_as_a_shape_without_a_border() {
    let desk = Desk::start_with(|settings| {
        settings.copy = false;
        settings.history = true;
        settings.notify = false;
        settings.outline = true;
    });
    begin(&desk);
    // SAFETY: the windows belong to this test.
    unsafe { set_mode(MODE_FREE) };
    desk.drag(
        s().overlay,
        (200, 200),
        &[(250, 200), (300, 200), (280, 240)],
        (250, 280),
    );
    assert_marking_up();
    let session = session();
    assert!(session.shaped, "a freeform shape has no straight edge");
    // 形を囲む範囲。左下の隅は形の外で透明。
    assert_eq!(edges(session.area), (200, 200, 300, 280));
    assert_eq!(pixel(&session.pixels, 100, 0, 79), [255, 255, 255, 0]);
    let original = session.pixels.clone();
    let (width, height, pixels) = capture_with_enter(&desk);
    assert_eq!((width, height), (100, 80));
    // 書き込まなければ、届くのは切り取ったままの画像。枠線は描かない。
    assert!(pixels == original, "the picture was changed");
}

#[test]
#[ignore = "Covers the screen"]
fn a_recording_selection_is_not_marked_up() {
    let desk = Desk::start_with(|settings| {
        settings.countdown = true;
        settings.system_audio = false;
    });
    s().video = true;
    begin(&desk);
    desk.drag(s().overlay, (200, 200), &[], (400, 320));
    assert!(s().markup_session.is_none(), "recordings are not marked up");
    assert!(
        !s().count_window.is_null(),
        "the recording counts down instead"
    );
    assert!(!shown(s().markup_bar));
    drop(desk);
}
