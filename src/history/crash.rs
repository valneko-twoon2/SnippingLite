//! クラッシュ時の整合性: 履歴への追加（縮小画像、画像、件数の上限による削除）を、段階ごとに
//! 子プロセスごと殺す。
//!
//! 子は、上限（50 件）まで埋まった履歴に 2 回追加する。1 回目は保存したばかりの PNG を写し、
//! 2 回目は自分で書く。どちらも、縮小画像と画像を一時名で書き、縮小画像、画像の順に置き、
//! いちばん古い項目を画像、縮小画像の順に消す。どの段階で殺しても、
//!
//! - 最後の名前（`<id>.png`、`<id>.thumb.png`）のファイルは、どれも書いたとおりの画像。
//! - 一覧の項目はどれも開け、縮小画像もある。項目は、画像を置いた時点で増え、画像を消した時点で減る。
//! - 次の追加（次の起動の最初の切り取り）が、書きかけと画像のない縮小画像を消す。その後は上限
//!   ちょうどの件数で、子が置いた項目も残っている。
//!
//! 縮小画像のない項目（別のプロセスの消去と追加が重なったとき）に、次の追加が縮小画像を作り直す
//! ことも確かめる。

use super::*;
use crate::test_support::{crash, Sandbox};

const LIMIT: usize = 50;
/// 1 回の追加の段階。
const ADD: [&str; 6] = [
    "thumbnail written",
    "picture written",
    "thumbnail in place",
    "picture in place",
    "picture removed",
    "thumbnail removed",
];

/// 子が追加する 6x4 の画像。画素はどれも違う。
fn picture() -> Vec<u8> {
    (0..24u8).flat_map(|i| [10 * i, 255 - i, i, 255]).collect()
}

/// 履歴のフォルダーのファイルの名前。
fn names() -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(folder().unwrap())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// 画像のない縮小画像の数。
fn orphans() -> usize {
    let folder = folder().unwrap();
    names()
        .iter()
        .filter_map(|name| name.strip_suffix(".thumb.png")?.parse::<u128>().ok())
        .filter(|&id| !entry(&folder, id).image.exists())
        .count()
}

#[test]
#[ignore = "Runs only as the child process of a crash test"]
fn child_adds_two_captures() {
    if !crash::is_child() {
        return;
    }
    let saved = std::env::temp_dir().join("saved.png");
    let first = imaging::timestamp().unwrap();
    add(&picture(), 6, 4, Some(&saved)).unwrap();
    // 項目の名前はミリ秒の時刻なので、2 回目は次のミリ秒まで待つ。
    for _ in 0..1000 {
        if imaging::timestamp().unwrap() > first {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    add(&picture(), 6, 4, None).unwrap();
}

fn prepare(sandbox: &Sandbox) {
    config::save_settings(&config::Settings {
        history_limit: LIMIT,
        ..config::parse_settings("")
    })
    .unwrap();
    let folder = folder().unwrap();
    fs::create_dir_all(&folder).unwrap();
    let old = sandbox.path().join("old.png");
    imaging::encode(&old, &[90, 60, 30, 255].repeat(4), 2, 2).unwrap();
    for id in 1..=LIMIT as u128 {
        fs::copy(&old, entry(&folder, id).image).unwrap();
        fs::copy(&old, entry(&folder, id).thumbnail).unwrap();
    }
    // 子が写す、保存したばかりの PNG。子の一時フォルダーに置く。
    let temp = sandbox.path().join("temp");
    fs::create_dir_all(&temp).unwrap();
    imaging::encode(&temp.join("saved.png"), &picture(), 6, 4).unwrap();
}

fn check(_: &Sandbox, crashed: &crash::Crash) {
    let order: Vec<&str> = ADD.iter().chain(&ADD).copied().collect();
    // 終えた段階（止まった段階を含む）。最後まで終えたらすべて。
    let done = match &crashed.stopped {
        Some(label) => {
            assert!(label.starts_with(order[crashed.at - 1]), "{label}");
            &order[..crashed.at]
        }
        None => &order[..],
    };
    let last = crashed.stopped.as_ref().map(|_| *done.last().unwrap());
    let count = |stage: &str| done.iter().filter(|&&s| s == stage).count();
    let placed = count("picture in place");
    let what = format!("stopped after {:?}", crashed.stopped);
    // 最後の名前のファイルは、どれも書いたとおりの画像。書きかけは一時名にだけある。GDI+ は
    // 途中で切れた PNG も読めることがあるので、読めるだけでなく中身を比べる。
    let files = names();
    for name in files.iter().filter(|n| n.ends_with(".png")) {
        let decoded = imaging::decode(&folder().unwrap().join(name));
        let (id, thumbnail) = match name.strip_suffix(".thumb.png") {
            Some(id) => (id, true),
            None => (name.strip_suffix(".png").unwrap(), false),
        };
        let expected = if id.parse::<u128>().unwrap() <= LIMIT as u128 {
            Some((2, 2, [90, 60, 30, 255].repeat(4)))
        } else if thumbnail {
            decoded
                .as_ref()
                .filter(|(w, h, _)| (*w, *h) == (6, 4))
                .cloned()
        } else {
            Some((6, 4, picture()))
        };
        assert!(
            decoded.is_some() && decoded == expected,
            "{name} is whole; {what}"
        );
    }
    let partial = files.iter().filter(|n| n.ends_with(".partial")).count();
    let expected_partial = match last {
        Some("thumbnail written") | Some("thumbnail in place") => 1,
        Some("picture written") => 2,
        _ => 0,
    };
    assert_eq!(partial, expected_partial, "{what}: {files:?}");
    // 一覧の項目はどれも開ける。
    let entries = list();
    assert_eq!(
        entries.len(),
        LIMIT + placed - count("picture removed"),
        "{what}"
    );
    for entry in &entries {
        let (width, height, pixels) = imaging::decode(&entry.image).expect("the entry opens");
        if entry.id > LIMIT as u128 {
            assert_eq!((width, height, pixels), (6, 4, picture()), "{what}");
        }
    }
    let without = entries.iter().filter(|e| !e.thumbnail.exists()).count();
    assert_eq!(without, 0, "every listed entry has its thumbnail; {what}");
    let stranded = matches!(last, Some("thumbnail in place") | Some("picture removed"));
    assert_eq!(orphans(), stranded as usize, "{what}");
    // 次の起動の最初の切り取りが片付ける。
    add(&[1, 2, 3, 255].repeat(9), 3, 3, None).unwrap();
    assert!(!names().iter().any(|n| n.ends_with(".partial")), "{what}");
    assert_eq!(orphans(), 0, "{what}");
    let entries = list();
    assert_eq!(entries.len(), LIMIT, "{what}");
    assert_eq!(
        imaging::decode(&entries[0].image).unwrap().0,
        3,
        "the newest"
    );
    let kept = entries[1..].iter().filter(|e| e.id > LIMIT as u128).count();
    assert_eq!(kept, placed, "the child's captures stay; {what}");
    for entry in &entries {
        assert!(imaging::decode(&entry.thumbnail).is_some(), "{what}");
    }
}

#[test]
fn clearing_the_history_also_removes_what_a_stopped_process_left() {
    // 履歴の消去は、残しておきたくない画像を消すためにある（README）。削除の途中で止まって残った
    // 縮小画像や、終わったプロセスの書きかけも消す。
    let sandbox = Sandbox::new();
    prepare(&sandbox);
    let mut finished = std::process::Command::new("cmd")
        .args(["/c", "exit 0"])
        .spawn()
        .unwrap();
    finished.wait().unwrap();
    let folder = folder().unwrap();
    fs::remove_file(entry(&folder, 7).image).unwrap();
    let leftover = folder.join(format!("99.png.{}.partial", finished.id()));
    fs::write(&leftover, b"half").unwrap();
    assert_eq!(orphans(), 1);
    clear();
    assert_eq!(names(), Vec::<String>::new());
}

#[test]
fn a_missing_thumbnail_is_made_again_by_the_next_capture() {
    // 履歴のウィンドウの消去が、ちょうど追加している項目の縮小画像を消したとき（画像はまだ置かれて
    // いなかった）。次の追加が、その項目の画像から縮小画像を作り直す。
    let sandbox = Sandbox::new();
    prepare(&sandbox);
    let folder = folder().unwrap();
    let lone = entry(&folder, 60);
    imaging::encode(&lone.image, &picture(), 6, 4).unwrap();
    assert!(!lone.thumbnail.exists());
    add(&[1, 2, 3, 255].repeat(9), 3, 3, None).unwrap();
    let (width, height, _) = imaging::decode(&lone.thumbnail).expect("a new thumbnail");
    assert_eq!((width, height), (6, 4));
    assert!(list().iter().all(|e| e.thumbnail.exists()));
}

#[test]
fn an_addition_killed_at_any_step_leaves_a_history_that_opens_and_is_tidied_next_time() {
    let child = crash::test_name(module_path!(), "child_adds_two_captures");
    let stages = crash::sweep(&child, 20, prepare, check);
    let names: Vec<&str> = stages
        .iter()
        .map(|label| {
            label
                .split_once(':')
                .map_or(label.as_str(), |(stage, _)| stage)
        })
        .collect();
    let expected: Vec<&str> = ADD.iter().chain(&ADD).copied().collect();
    assert_eq!(
        names, expected,
        "the child stopped at every step of both additions"
    );
}
