//! 録画のときの範囲。Snipping Tool と同じく、録画は四角形とウィンドウだけを選べる。
//!
//! | 操作 | 前の範囲 | 後の範囲 |
//! |---|---|---|
//! | 動画のボタン | 四角形・ウィンドウ | そのまま |
//! | 動画のボタン | 自由形式・全画面 | 四角形 |
//! | カメラのボタン | どれでも | そのまま |
//! | 録画のドロップダウン | - | 四角形とウィンドウだけを並べる |

use super::*;

#[test]
fn a_recording_offers_only_the_rectangle_and_the_window() {
    // 下キーは末尾から先頭へ戻るので、3 つ目を選ぶと四角形に戻る。
    let video: Vec<i32> = (0..3).map(|position| area_mode(position, true)).collect();
    assert_eq!(video, [MODE_RECT, MODE_WINDOW, MODE_RECT]);
    let snip: Vec<i32> = (0..4).map(|position| area_mode(position, false)).collect();
    assert_eq!(snip, [MODE_RECT, MODE_WINDOW, MODE_SCREEN, MODE_FREE]);
    for mode in [MODE_RECT, MODE_WINDOW, MODE_SCREEN, MODE_FREE] {
        assert_eq!(area_menu::fitting(mode, false), mode, "a snip keeps {mode}");
    }
    assert_eq!(area_menu::fitting(MODE_WINDOW, true), MODE_WINDOW);
    assert_eq!(area_menu::fitting(MODE_FREE, true), MODE_RECT);
    assert_eq!(area_menu::fitting(MODE_SCREEN, true), MODE_RECT);
}

#[test]
#[ignore = "Covers the screen"]
fn the_video_button_turns_a_freeform_or_full_screen_snip_into_a_rectangle() {
    let desk = Desk::start();
    start(&desk);
    // SAFETY: the windows belong to this test.
    unsafe { set_mode(MODE_FREE) };
    press(&desk, VIDEO_BUTTON);
    assert!(s().video && s().active && s().mode == MODE_RECT);
    // カメラに戻しても四角形のまま。自由形式はもう一度選べる。
    press(&desk, CAMERA_BUTTON);
    assert!(!s().video && s().mode == MODE_RECT);
    desk.key(s().overlay, 0x1b);
    // 撮影の合間に残したツールバーでも、全画面のまま録画を始めない。
    s().mode = MODE_SCREEN;
    s().toolbar_open = true;
    press(&desk, VIDEO_BUTTON);
    assert!(s().video && s().active && s().mode == MODE_RECT);
    desk.key(s().overlay, 0x1b);
}

#[test]
#[ignore = "Shows a menu"]
fn the_area_menu_of_a_recording_lists_the_rectangle_and_the_window() {
    let desk = Desk::start();
    s().video = true;
    start(&desk);
    for (downs, mode) in [(1, MODE_WINDOW), (0, MODE_RECT), (3, MODE_WINDOW)] {
        let answer = answer_menu(downs);
        press(&desk, AREA_BUTTON);
        assert!(answer.join().unwrap());
        assert_eq!(s().mode, mode, "{downs} downs");
        assert!(s().active, "choosing a mode does not record yet");
    }
    desk.key(s().overlay, 0x1b);
}
