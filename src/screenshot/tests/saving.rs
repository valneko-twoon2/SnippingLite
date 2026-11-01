//! README: 置けなかったスクリーンショットは残さない。エラーを表示し、クリップボードと履歴がオンなら、
//! 画像はそこにある。保存に失敗する所で分ける。
//!
//! | 保存先 | 期待値 |
//! |---|---|
//! | フォルダーを作れない（同じ名前のファイルがある） | エラーを表示、保存したファイルなし、コピーと履歴に画像、通知なし |
//! | フォルダーはあるが、ファイルを作れない（書き込みを拒むアクセス許可） | 同上 |

use super::*;
use std::{path::Path, process::Command, process::Stdio};

const NO_FOLDER: &str = "the folder cannot be made";
const NO_FILES: &str = "the folder takes no files";

/// だれも `folder` にファイルやフォルダーを作れないようにする（`deny` が false なら元に戻す）。
fn refuse_writes(folder: &Path, deny: bool) {
    // S-1-1-0 は Everyone。WD はファイルの追加、AD はフォルダーの追加。
    let args: &[&str] = if deny {
        &["/deny", "*S-1-1-0:(WD,AD)"]
    } else {
        &["/remove:d", "*S-1-1-0"]
    };
    let status = Command::new("icacls")
        .arg(folder)
        .args(args)
        .stdout(Stdio::null())
        .status()
        .expect("icacls runs");
    assert!(status.success(), "icacls {args:?}");
}

#[test]
#[ignore = "Shows a message box"]
fn a_screenshot_that_cannot_be_saved_is_reported_and_still_copied_and_kept() {
    for case in [NO_FOLDER, NO_FILES] {
        let desk = Desk::start_with(|settings| {
            settings.copy = true;
            settings.save = true;
            settings.history = true;
            settings.notify = true;
        });
        let folder = desk.sandbox.path().join("unwritable");
        if case == NO_FOLDER {
            std::fs::write(&folder, b"x").unwrap();
        } else {
            std::fs::create_dir_all(&folder).unwrap();
            refuse_writes(&folder, true);
        }
        let mut settings = config::load_settings();
        settings.screenshot_folder = folder.to_string_lossy().into_owned();
        config::save_settings(&settings).unwrap();
        // 前の画像と取り違えないよう、クリップボードを文字にしておく。
        assert!(imaging::copy_text(null_mut(), "-"));
        begin(&desk);
        let answer = answer_dialog(2);
        desk.drag(s().overlay, (100, 100), &[], (180, 160));
        let shown = answer.join().unwrap();
        if case == NO_FILES {
            refuse_writes(&folder, false);
        }
        assert!(shown, "{case}: the error is shown");
        assert!(!s().active, "{case}");
        if case == NO_FILES {
            let left: Vec<_> = std::fs::read_dir(&folder).unwrap().flatten().collect();
            assert!(left.is_empty(), "{case}: nothing is saved: {left:?}");
        }
        let (width, height, _) = clipboard_image().expect(case);
        assert_eq!((width, height), (80, 60), "{case}: the picture is copied");
        let entries = history::list();
        assert_eq!(entries.len(), 1, "{case}: the picture is in the history");
        let kept = imaging::decode(&entries[0].image).map(|(w, h, _)| (w, h));
        assert_eq!(kept, Some((80, 60)), "{case}");
        // SAFETY: plain state query.
        assert!(
            !unsafe { toast::visible() },
            "{case}: the error replaces the notification"
        );
        drop(desk);
    }
}
