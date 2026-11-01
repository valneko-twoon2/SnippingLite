//! クラッシュ時の整合性: 録画の書き込みと、一時フォルダーの掃除を途中で殺す。
//!
//! 一時フォルダーの掃除は古い録画を 1 つずつ消す。どの録画を消したところで殺しても、残りは
//! 次の掃除が消し、1 日より新しい録画は残る。子の一時フォルダーは親のサンドボックスの中にあり、
//! 本物の一時フォルダーには触れない（親は `temporary_folder` を呼ばず、場所を渡す方を使う）。
//!
//! 録画は一時名（`<最後の名前>.<プロセス ID>.partial`）で書き、仕上げてから最後の名前に変える。
//! 録画の途中で殺すと、保存先に最後の名前の MP4（moov のない再生できないファイル）はなく、
//! 書きかけは次の録画を始めるときに消える。止めずに終えれば、再生できる MP4 が 1 つだけある。

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

/// 終わったプロセス。ハンドルを持っている間、その ID はほかのプロセスに使い回されない。
pub(super) struct Finished(pub(super) std::process::Child);

pub(super) fn finished_process() -> Finished {
    let mut child = std::process::Command::new("cmd")
        .args(["/c", "exit 0"])
        .spawn()
        .expect("cmd starts");
    child.wait().unwrap();
    Finished(child)
}

fn names(folder: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(folder)
        .into_iter()
        .flatten()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn a_recording_left_by_a_stopped_process_is_removed_when_the_next_one_starts() {
    let sandbox = Sandbox::new();
    let videos = sandbox.path().join("videos");
    crate::config::save_settings(&crate::config::Settings {
        video_folder: videos.to_string_lossy().into_owned(),
        ..crate::config::parse_settings("")
    })
    .unwrap();
    let stopped = finished_process();
    let mine = std::process::id();
    let now = SystemTime::now();
    for folder in [videos.clone(), recordings(&sandbox)] {
        file_at(
            &folder.join(format!("Snipping Lite 1.mp4.{}.partial", stopped.0.id())),
            now,
        );
        file_at(&folder.join("Snipping Lite 2.mp4"), now);
        file_at(
            &folder.join(format!("Snipping Lite 3.mp4.{mine}.partial")),
            now,
        );
    }
    // 保存する録画の場所と、保存しない録画の一時フォルダー。どちらも次の録画を始めるときに片付く。
    assert_eq!(video_folder().unwrap(), videos);
    let temporary = temporary_folder_in(&sandbox.path().join("temp"), now).unwrap();
    assert_eq!(temporary, recordings(&sandbox));
    for folder in [videos, temporary] {
        assert_eq!(
            names(&folder),
            [
                "Snipping Lite 2.mp4".to_owned(),
                format!("Snipping Lite 3.mp4.{mine}.partial")
            ],
            "a finished recording and one this process is writing stay"
        );
    }
}

#[test]
#[ignore = "Runs only as the child process of a crash test"]
fn child_records_the_screen() {
    if !crash::is_child() {
        return;
    }
    // 録画を始める流れ（video::begin_recording）と同じ場所と名前。
    let path = video_folder().unwrap().join(format!(
        "Snipping Lite {}.mp4",
        crate::imaging::timestamp().unwrap()
    ));
    let recording = start(area(64, 64), path.clone(), Vec::new(), options(30)).unwrap();
    for _ in 0..1500 {
        if recording.started.get().is_some() || recording.failed() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        recording.started.get().is_some(),
        "the first frame was written"
    );
    // 録画している途中。1 回目はここで殺される。
    crash::step("recording", &path);
    let (saved, frame) = recording.stop().expect("the recording finishes");
    assert!(saved == path && frame.warning.is_none());
}

#[test]
#[ignore = "Records the screen"]
fn a_recording_killed_while_it_runs_leaves_no_broken_video() {
    let child = crash::test_name(module_path!(), "child_records_the_screen");
    let videos = |sandbox: &Sandbox| sandbox.path().join("videos");
    let prepare = |sandbox: &Sandbox| {
        crate::config::save_settings(&crate::config::Settings {
            video_folder: videos(sandbox).to_string_lossy().into_owned(),
            save_video: true,
            ..crate::config::parse_settings("")
        })
        .unwrap();
    };
    let check = |sandbox: &Sandbox, crashed: &crash::Crash| {
        let folder = videos(sandbox);
        let files = names(&folder);
        let videos: Vec<&String> = files.iter().filter(|n| n.ends_with(".mp4")).collect();
        let partial = files.iter().filter(|n| n.ends_with(".partial")).count();
        if crashed.stopped.is_some() {
            assert!(
                videos.is_empty(),
                "no video under its final name: {files:?}"
            );
            assert_eq!(partial, 1, "the recording being written: {files:?}");
            // 次の録画を始めるとき、終わったプロセスの書きかけを消す。
            assert_eq!(video_folder().unwrap(), folder);
            assert_eq!(names(&folder), Vec::<String>::new());
        } else {
            assert_eq!((videos.len(), partial), (1, 0), "{files:?}");
            let bytes = std::fs::read(folder.join(videos[0])).unwrap();
            assert_eq!(&bytes[4..8], b"ftyp");
            assert!(
                bytes.windows(4).any(|atom| atom == b"moov"),
                "a playable MP4"
            );
        }
    };
    let stages = crash::sweep(&child, 5, prepare, check);
    assert_eq!(stages.len(), 1, "{stages:?}");
    assert!(
        stages[0].starts_with("recording: Snipping Lite "),
        "{stages:?}"
    );
}
