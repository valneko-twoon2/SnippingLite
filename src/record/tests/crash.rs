//! クラッシュ時の整合性: 録画の書き込みと、一時フォルダーの掃除を途中で殺す。
//!
//! 一時フォルダーの掃除は古い録画を 1 つずつ消す。どの録画を消したところで殺しても、残りは
//! 次の掃除が消し、1 日より新しい録画は残る。子の一時フォルダーは親のサンドボックスの中にあり、
//! 本物の一時フォルダーには触れない（親は `temporary_folder` を呼ばず、場所を渡す方を使う）。

use super::*;
use crate::test_support::{crash, Sandbox};
use std::time::SystemTime;

const DAY: Duration = Duration::from_secs(24 * 60 * 60);
const OLD: [&str; 3] = [
    "Snipping Lite 1.mp4",
    "Snipping Lite 2.mp4",
    "Snipping Lite 3.mp4",
];
const FRESH: &str = "Snipping Lite 4.mp4";

/// 子の一時フォルダー（`TEMP`）の中の、録画を置くフォルダー。
fn recordings(sandbox: &Sandbox) -> PathBuf {
    sandbox
        .path()
        .join("temp")
        .join("Snipping Lite")
        .join("Recordings")
}

fn file_at(path: &Path, modified: SystemTime) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::File::create(path)
        .unwrap()
        .set_modified(modified)
        .unwrap();
}

#[test]
#[ignore = "Runs only as the child process of a crash test"]
fn child_cleans_the_temporary_recordings() {
    if !crash::is_child() {
        return;
    }
    // 子の一時フォルダーは、親のサンドボックスの中にある。
    temporary_folder().unwrap();
}

#[test]
fn a_cleanup_killed_part_way_is_finished_by_the_next_one() {
    let child = crash::test_name(module_path!(), "child_cleans_the_temporary_recordings");
    let prepare = |sandbox: &Sandbox| {
        let now = SystemTime::now();
        for name in OLD {
            file_at(&recordings(sandbox).join(name), now - 30 * DAY);
        }
        file_at(&recordings(sandbox).join(FRESH), now);
    };
    let check = |sandbox: &Sandbox, crashed: &crash::Crash| {
        let folder = recordings(sandbox);
        let left = OLD.iter().filter(|name| folder.join(name).exists()).count();
        // k 回目の段階は、k 個目の古い録画を消した直後。最後まで終えたら全部消えている。
        let removed = crashed.stopped.as_ref().map_or(OLD.len(), |_| crashed.at);
        assert_eq!(left, OLD.len() - removed, "{:?}", crashed.stopped);
        if let Some(label) = &crashed.stopped {
            let name = label.trim_start_matches("old recording removed: ");
            assert!(
                !folder.join(name).exists(),
                "{name} was removed before the stop"
            );
        }
        assert!(folder.join(FRESH).exists(), "a recording of today stays");
        // 次の掃除（次の録画の開始）が残りを消す。
        temporary_folder_in(&sandbox.path().join("temp"), SystemTime::now()).unwrap();
        assert!(OLD.iter().all(|name| !folder.join(name).exists()));
        assert!(folder.join(FRESH).exists());
    };
    let mut stages = crash::sweep(&child, 10, prepare, check);
    stages.sort();
    assert_eq!(
        stages,
        OLD.map(|name| format!("old recording removed: {name}")),
        "the child stopped after removing each old recording"
    );
}
