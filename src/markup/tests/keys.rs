//! 書き込み中のキーボード。README: キャプチャか Enter で確定し（いつもどおりコピー、保存、履歴）、
//! Escape か × で捨てる。Ctrl+C は、自動コピーの設定にかかわらずクリップボードにコピーしてから
//! 確定する（保存と履歴は設定どおり）。コードの意図（`overlay_message`）: Ctrl+Z は元に戻す、
//! Ctrl+Y はやり直す。ほかのキーは何もしない。
//!
//! | キー | Ctrl | 期待値 | テスト |
//! |---|---|---|---|
//! | Enter | - | 書き込みを重ねて確定 | `ctrl_c_and_enter_deliver_the_marked_up_snip` |
//! | C | 押す | 同上 | 同上 |
//! | Enter、キャプチャのボタン（コピーがオフ） | - | 確定するが、コピーしない | `enter_and_the_capture_button_copy_nothing_with_copying_off` |
//! | C（コピーがオフ） | 押す | コピーして確定する。保存と履歴は設定どおり（オンとオフ） | `ctrl_c_copies_even_with_copying_off_and_saves_as_the_settings_say` |
//! | Z | 押す | 最後の線を取り消す。取り消すものがなければ何もしない | `ctrl_z_takes_back_the_last_stroke_and_ctrl_y_puts_it_back` |
//! | Y | 押す | 取り消した線を戻す。戻すものがなければ何もしない | 同上 |
//! | Z、Y（描いている・なぞっている途中） | 押す | 途中の線やなぞって消した分を先に確定してから、元に戻す・やり直す。その後は離すまで何もしない | `ctrl_z_and_ctrl_y_in_the_middle_of_a_drag_settle_it_first`、モデルベーステスト |
//! | Z、Y、C | 押さない | 何もしない | `letters_without_ctrl_and_other_ctrl_letters_leave_the_snip_alone` |
//! | X | 押す | 何もしない | 同上 |
//! | Escape | - | 捨てる。何も届かない | `escape_throws_away_the_strokes_and_delivers_nothing`、既存 `escape_and_the_close_button_discard_a_marked_up_snip` |

use super::*;
use crate::test_support::{clipboard_image, clipboard_text};

/// `key` を押す。`control` のときは Ctrl を押したままにする。オーバーレイが読む GetKeyState は
/// このスレッドのキーの状態なので、本物のキーボードには触れずに済む。
fn press(desk: &Desk, key: u16, control: bool) {
    let mut keys = [0u8; 256];
    // SAFETY: both calls read or write the 256 key states of this thread.
    unsafe {
        GetKeyboardState(keys.as_mut_ptr());
        let saved = keys;
        keys[VK_CONTROL as usize] = if control { 0x80 } else { 0 };
        SetKeyboardState(keys.as_ptr());
        desk.key(s().overlay, key);
        SetKeyboardState(saved.as_ptr());
    }
}

/// 範囲の中に横線を 1 本引く。線の中心は、範囲の左上から (60, 40)。
fn stroke(desk: &Desk) {
    desk.drag(s().overlay, (220, 240), &[(260, 240)], (300, 240));
}

/// (線の数, 元に戻せる数, やり直せる数)。
fn stacks() -> (usize, usize, usize) {
    let session = s().markup_session.as_ref().expect("marking up");
    (session.marks.len(), session.undo.len(), session.redo.len())
}

/// 画面に出している画像に線が描かれているか（切り取ったままの画像と違うか）。
fn drawn() -> bool {
    let session = s().markup_session.as_ref().expect("marking up");
    session.base != session.pixels
}

#[test]
#[ignore = "Covers the screen"]
fn ctrl_z_takes_back_the_last_stroke_and_ctrl_y_puts_it_back() {
    let desk = Desk::start_with(|settings| {
        settings.copy = false;
        settings.notify = false;
    });
    mark_up(&desk);
    // 取り消すものも戻すものもないうちは、何もしない。
    press(&desk, b'Z' as u16, true);
    press(&desk, b'Y' as u16, true);
    assert_eq!(stacks(), (0, 0, 0));
    stroke(&desk);
    assert_eq!(stacks(), (1, 1, 0));
    press(&desk, b'Z' as u16, true);
    assert_eq!(stacks(), (0, 0, 1));
    assert!(!drawn(), "the stroke left the picture");
    press(&desk, b'Z' as u16, true);
    assert_eq!(stacks(), (0, 0, 1), "nothing more to take back");
    press(&desk, b'Y' as u16, true);
    assert_eq!(stacks(), (1, 1, 0));
    assert!(drawn(), "the stroke is back on the picture");
    press(&desk, b'Y' as u16, true);
    assert_eq!(stacks(), (1, 1, 0), "nothing more to put back");
    assert!(s().active && shown(s().markup_bar), "marking up goes on");
}

#[test]
#[ignore = "Covers the screen"]
fn ctrl_z_and_ctrl_y_in_the_middle_of_a_drag_settle_it_first() {
    let desk = Desk::start_with(|settings| {
        settings.copy = false;
        settings.notify = false;
    });
    mark_up(&desk);
    stroke(&desk);
    let overlay = s().overlay;
    // 2 本目を描いている途中の Ctrl+Z: 描きかけの線を 1 回分として確定してから元に戻す。
    desk.mouse(overlay, WM_MOUSEMOVE, 220, 260);
    desk.mouse(overlay, WM_LBUTTONDOWN, 220, 260);
    desk.mouse(overlay, WM_MOUSEMOVE, 300, 260);
    press(&desk, b'Z' as u16, true);
    assert_eq!(stacks(), (1, 1, 1), "the line being drawn is taken back");
    // 押したまま動かしても、離しても、もう描かない。
    desk.mouse(overlay, WM_MOUSEMOVE, 320, 270);
    desk.mouse(overlay, WM_LBUTTONUP, 320, 270);
    assert_eq!(stacks(), (1, 1, 1), "nothing more is drawn");
    press(&desk, b'Y' as u16, true);
    assert_eq!(stacks(), (2, 2, 0), "Ctrl+Y puts it back");
    // 消しゴムで 2 本ともなぞって消した途中の Ctrl+Z: 消した分を 1 回分として確定してから戻す。
    // SAFETY: the windows belong to this test.
    unsafe { set_tool(Tool::Eraser) };
    desk.mouse(overlay, WM_MOUSEMOVE, 260, 230);
    desk.mouse(overlay, WM_LBUTTONDOWN, 260, 230);
    desk.mouse(overlay, WM_MOUSEMOVE, 260, 270);
    assert_eq!(stacks().0, 0, "both lines are erased");
    press(&desk, b'Z' as u16, true);
    assert_eq!(stacks(), (2, 2, 1), "the erasing is taken back");
    desk.mouse(overlay, WM_LBUTTONUP, 260, 270);
    assert_eq!(stacks(), (2, 2, 1), "letting go adds nothing to undo");
    press(&desk, b'Y' as u16, true);
    assert_eq!(stacks(), (0, 3, 0), "Ctrl+Y erases them again");
    assert!(s().active && shown(s().markup_bar), "marking up goes on");
}

#[test]
#[ignore = "Covers the screen"]
fn ctrl_c_and_enter_deliver_the_marked_up_snip() {
    for (name, key, control) in [("Enter", VK_RETURN, false), ("Ctrl+C", b'C' as u16, true)] {
        let desk = Desk::start_with(|settings| {
            settings.copy = true;
            settings.save = false;
            settings.history = false;
            settings.notify = false;
        });
        // 前の画像と取り違えないよう、クリップボードを文字にしておく。
        assert!(imaging::copy_text(null_mut(), "-"));
        mark_up(&desk);
        stroke(&desk);
        press(&desk, key, control);
        assert!(!s().active && s().markup_session.is_none(), "{name}");
        assert!(!shown(s().overlay) && !shown(s().markup_bar), "{name}");
        let (width, height, pixels) = clipboard_image().expect(name);
        assert_eq!((width, height), (160, 80), "{name}");
        let at = ((40 * width + 60) * 4) as usize;
        let [r, g, b] = annotate::COLORS[config::load_pen().0];
        assert_eq!(
            pixels[at..at + 4],
            [b, g, r, 255],
            "{name}: the stroke is in it"
        );
        drop(desk);
    }
}

#[test]
#[ignore = "Covers the screen"]
fn letters_without_ctrl_and_other_ctrl_letters_leave_the_snip_alone() {
    let desk = Desk::start_with(|settings| {
        settings.copy = true;
        settings.history = true;
        settings.notify = false;
    });
    assert!(imaging::copy_text(null_mut(), "-"));
    mark_up(&desk);
    stroke(&desk);
    for (key, control) in [(b'Z', false), (b'Y', false), (b'C', false), (b'X', true)] {
        let name = format!("{}{}", if control { "Ctrl+" } else { "" }, key as char);
        press(&desk, key as u16, control);
        assert_eq!(stacks(), (1, 1, 0), "{name}");
        assert!(s().active && shown(s().markup_bar), "{name}");
    }
    assert!(crate::history::list().is_empty(), "nothing was captured");
    assert_eq!(clipboard_text().as_deref(), Some("-"), "nothing was copied");
}

#[test]
#[ignore = "Covers the screen"]
fn escape_throws_away_the_strokes_and_delivers_nothing() {
    let desk = Desk::start_with(|settings| {
        settings.copy = true;
        settings.history = true;
        settings.notify = false;
    });
    assert!(imaging::copy_text(null_mut(), "-"));
    mark_up(&desk);
    stroke(&desk);
    desk.key(s().overlay, VK_ESCAPE);
    assert!(!s().active && s().markup_session.is_none());
    assert!(!shown(s().overlay) && !shown(s().markup_bar));
    assert!(crate::history::list().is_empty());
    assert_eq!(clipboard_text().as_deref(), Some("-"));
}

#[test]
#[ignore = "Covers the screen"]
fn ctrl_c_copies_even_with_copying_off_and_saves_as_the_settings_say() {
    for (save, history) in [(true, true), (false, false)] {
        let what = format!("save={save} history={history}");
        let desk = Desk::start_with(|settings| {
            settings.copy = false;
            settings.save = save;
            settings.history = history;
            settings.notify = false;
        });
        assert!(imaging::copy_text(null_mut(), "-"));
        mark_up(&desk);
        stroke(&desk);
        press(&desk, b'C' as u16, true);
        assert!(!s().active && s().markup_session.is_none(), "{what}");
        let (width, height, pixels) = clipboard_image().expect(&what);
        assert_eq!((width, height), (160, 80), "{what}");
        let at = ((40 * width + 60) * 4) as usize;
        let [r, g, b] = annotate::COLORS[config::load_pen().0];
        assert_eq!(
            pixels[at..at + 4],
            [b, g, r, 255],
            "{what}: the stroke is in it"
        );
        assert_eq!(shots(&desk).len(), save as usize, "{what}");
        assert_eq!(crate::history::list().len(), history as usize, "{what}");
        drop(desk);
    }
}

#[test]
#[ignore = "Covers the screen"]
fn enter_and_the_capture_button_copy_nothing_with_copying_off() {
    for name in ["Enter", "the capture button"] {
        let desk = Desk::start_with(|settings| {
            settings.copy = false;
            settings.history = true;
            settings.notify = false;
        });
        assert!(imaging::copy_text(null_mut(), "-"));
        mark_up(&desk);
        stroke(&desk);
        if name == "Enter" {
            press(&desk, VK_RETURN, false);
        } else {
            // SAFETY: the windows belong to this test.
            unsafe { crate::markup::bar::press(CAPTURE) };
        }
        assert!(!s().active, "{name}");
        assert_eq!(crate::history::list().len(), 1, "{name} captures");
        assert_eq!(
            clipboard_text().as_deref(),
            Some("-"),
            "{name} copies nothing"
        );
        drop(desk);
    }
}
