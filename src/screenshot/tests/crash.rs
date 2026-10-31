//! クラッシュ時の整合性: スクリーンショットの保存を、画像を書いたところで子プロセスごと殺す。
//!
//! 画像は一時名で書いてから最後の名前（`Snipping Lite <時刻>.png`）に変える。名前を変える前に
//! 殺すと、保存先に最後の名前のファイルはなく、書きかけは一時名でだけ残る。次の保存が保存先を
//! 用意するとき、終わったプロセスの書きかけを消す。最後まで終えれば、読める画像が 1 つだけある。

use super::*;
use crate::test_support::{crash, Sandbox};
use std::path::Path;

/// 4x3 の画像。画素はどれも違う。
fn picture() -> Vec<u8> {
    (0..12u8).flat_map(|i| [i, 20 * i, 255 - i, 255]).collect()
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
#[ignore = "Runs only as the child process of a crash test"]
fn child_saves_a_screenshot() {
    if !crash::is_child() {
        return;
    }
    // SAFETY: saving reads only the settings, not the tray state.
    unsafe { save_image(&picture(), 4, 3, false) }.unwrap();
}

#[test]
fn a_screenshot_killed_while_it_is_written_leaves_no_broken_file() {
    let child = crash::test_name(module_path!(), "child_saves_a_screenshot");
    let shots = |sandbox: &Sandbox| sandbox.path().join("shots");
    let prepare = |sandbox: &Sandbox| {
        config::save_settings(&config::Settings {
            screenshot_folder: shots(sandbox).to_string_lossy().into_owned(),
            ..config::parse_settings("")
        })
        .unwrap();
    };
    let check = |sandbox: &Sandbox, crashed: &crash::Crash| {
        let folder = shots(sandbox);
        let files = names(&folder);
        let pictures: Vec<&String> = files.iter().filter(|n| n.ends_with(".png")).collect();
        let partial: Vec<&String> = files.iter().filter(|n| n.ends_with(".partial")).collect();
        if crashed.stopped.is_some() {
            assert!(
                pictures.is_empty(),
                "no picture under its final name: {files:?}"
            );
            assert_eq!(partial.len(), 1, "the picture being written: {files:?}");
            // 次の保存が保存先を用意するとき、終わったプロセスの書きかけを消す。
            assert_eq!(imaging::screenshot_folder().unwrap(), folder);
            assert_eq!(names(&folder), Vec::<String>::new());
        } else {
            assert_eq!((pictures.len(), partial.len()), (1, 0), "{files:?}");
            let saved = imaging::decode(&folder.join(pictures[0])).expect("a whole picture");
            assert_eq!(saved, (4, 3, picture()));
        }
    };
    let stages = crash::sweep(&child, 5, prepare, check);
    assert_eq!(stages.len(), 1, "{stages:?}");
    assert!(
        stages[0].starts_with("picture written: Snipping Lite ") && stages[0].ends_with(".png"),
        "{stages:?}"
    );
}
