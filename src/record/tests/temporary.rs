//! 保存しない録画を置く一時フォルダーの掃除。README は「コピーしたファイルを貼り付けられるよう、
//! 一時フォルダーに 1 日残す」と言うので、1 日より古いファイルだけを消す。場所と時刻は引数で渡し、
//! 本物の一時フォルダーと時計には触れない。
//!
//! | 項目 | 技法 | ケース → 期待値 |
//! |---|---|---|
//! | 古さ | 境界値（単位は FILETIME の 100 ns） | 1 日 - 1 目盛り、ちょうど 1 日 → 残る; 1 日 + 1 目盛り → 消える |
//! | 古さ | 同値分割 | 0、1 時間 → 残る; 2 日、30 日 → 消える; 未来の日時（時計が戻った）→ 残る |
//! | 場所 | 範囲 | 親のフォルダー、その隣のフォルダー、一時フォルダーの直下の古いファイル → 残る |
//! | 環境 | 失敗 | 開いたままで消せないファイル → 残り、ほかの古いファイルは消え、panic せず Ok |
//! | 環境 | 失敗 | 中のフォルダー → 掃除を止めない（フォルダーそのものの扱いは仕様になく、質問に回す） |

use super::*;
use crate::test_support::Sandbox;
use std::{os::windows::fs::OpenOptionsExt, time::SystemTime};

const DAY: Duration = Duration::from_secs(24 * 60 * 60);
/// FILETIME の 1 目盛り（100 ナノ秒）。NTFS が更新日時を持つ最小の単位。
const TICK: Duration = Duration::from_nanos(100);

/// テストの「今」は 10 日先。テストが作ったままのファイルやフォルダーは、どれも 1 日より古いことになる。
fn now() -> SystemTime {
    SystemTime::now() + 10 * DAY
}

/// 録画のフォルダーを作り、そのパスを返す。
fn recordings(base: &Path, now: SystemTime) -> PathBuf {
    let folder = temporary_folder_in(base, now).expect("the folder is created");
    assert!(folder.is_dir() && folder.starts_with(base));
    folder
}

/// 更新日時が `modified` のファイルを作る。閉じてから返すので、フォルダーの一覧にも日時が載る。
fn file_at(folder: &Path, name: &str, modified: SystemTime) -> PathBuf {
    std::fs::create_dir_all(folder).unwrap();
    let path = folder.join(name);
    let file = std::fs::File::create(&path).unwrap();
    file.set_modified(modified).unwrap();
    path
}

#[test]
fn a_recording_is_removed_only_when_it_is_older_than_a_day() {
    let sandbox = Sandbox::new();
    let now = now();
    let folder = recordings(sandbox.path(), now);
    // (古さ, 残るか)。境界はちょうど 1 日で、1 日は残す。
    let cases = [
        (Duration::ZERO, true),
        (Duration::from_secs(60 * 60), true),
        (DAY - TICK, true),
        (DAY, true),
        (DAY + TICK, false),
        (2 * DAY, false),
        (30 * DAY, false),
    ];
    let files: Vec<(PathBuf, bool)> = cases
        .iter()
        .enumerate()
        .map(|(i, &(age, kept))| {
            let name = format!("Snipping Lite {i}.mp4");
            (file_at(&folder, &name, now - age), kept)
        })
        .collect();
    assert_eq!(temporary_folder_in(sandbox.path(), now).unwrap(), folder);
    for ((path, kept), (age, _)) in files.iter().zip(cases) {
        assert_eq!(path.exists(), *kept, "{age:?}");
    }
}

#[test]
fn a_recording_dated_after_now_is_kept() {
    // 時計が戻されると、録画の日時が「今」より後になる。古くはないので残す。
    let sandbox = Sandbox::new();
    let now = now();
    let folder = recordings(sandbox.path(), now);
    let ahead = file_at(&folder, "Snipping Lite ahead.mp4", now + DAY);
    temporary_folder_in(sandbox.path(), now).unwrap();
    assert!(ahead.exists());
}

#[test]
fn only_the_recordings_folder_is_cleaned() {
    let sandbox = Sandbox::new();
    let now = now();
    let base = sandbox.path();
    let folder = recordings(base, now);
    let old = now - 30 * DAY;
    // 同じ親のフォルダーには「このアプリについて」のページなどが置かれる。
    let parent = folder.parent().unwrap();
    let outside = [
        file_at(parent, "about.html", old),
        file_at(&parent.join("Other"), "Snipping Lite old.mp4", old),
        file_at(base, "Snipping Lite old.mp4", old),
    ];
    let inside = file_at(&folder, "Snipping Lite old.mp4", old);
    temporary_folder_in(base, now).unwrap();
    assert!(!inside.exists(), "the old recording is removed");
    for path in outside {
        assert!(path.exists(), "{path:?} is not ours to remove");
    }
}

#[test]
fn a_file_that_cannot_be_removed_is_left_and_the_rest_are_still_cleaned() {
    let sandbox = Sandbox::new();
    let now = now();
    let folder = recordings(sandbox.path(), now);
    let old = now - 2 * DAY;
    // 名前の順で先に来るファイルを、共有を許さずに開いたままにする（貼り付け中のコピーなど）。
    let held = file_at(&folder, "Snipping Lite a.mp4", old);
    let other = file_at(&folder, "Snipping Lite b.mp4", old);
    let handle = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&held)
        .unwrap();
    assert!(std::fs::remove_file(&held).is_err(), "the file is held");
    assert_eq!(temporary_folder_in(sandbox.path(), now).unwrap(), folder);
    assert!(held.exists(), "a held file stays");
    assert!(!other.exists(), "the cleanup went on after it");
    drop(handle);
}

#[test]
fn a_folder_inside_does_not_stop_the_cleanup() {
    let sandbox = Sandbox::new();
    let now = now();
    let folder = recordings(sandbox.path(), now);
    // 中のフォルダーは作ったままなので、10 日先の「今」から見て古い。名前の順で先に来る。
    std::fs::create_dir_all(folder.join("a folder")).unwrap();
    let old = file_at(&folder, "Snipping Lite b.mp4", now - 2 * DAY);
    assert_eq!(temporary_folder_in(sandbox.path(), now).unwrap(), folder);
    assert!(!old.exists());
}
