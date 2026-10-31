//! 一時ファイルを経た書き込み（`write_atomic`）と、残った一時ファイルの後片付け（`config::temporary`）。
//!
//! | 項目 | 技法 | ケース → 期待値 |
//! |---|---|---|
//! | 書き込み | 状態 | 前のファイルがある → 置き換え、一時ファイルは残らない; 名前を変えられない → 失敗、一時ファイルは残らない; フォルダーがない → 失敗 |
//! | 持ち主 | 決定表（名前 × 持ち主） | 3 つのファイルの `<ファイル名>.<ID>.tmp` で終わったプロセス → 消す; このプロセス → 残す（動いているほかのプロセスは `crash.rs`） |
//! | 前の版の名前 | 境界値（秒） | `<名前>.tmp` が 120 秒前 → 消す; 30 秒前 → 残す（境目は 60 秒）; 未来の時刻 → 残す |
//! | 規則に合わない名前 | 同値分割 | ほかのファイルの一時ファイル、ID なし、数字でない、符号付き、桁あふれ、続きがある、形が違う → 残す |

use super::crash::{before, folder, names, prepare, FILES};
use super::*;
use crate::test_support::Sandbox;

#[test]
fn atomic_write_replaces_the_file_and_leaves_no_temporary() {
    let sandbox = Sandbox::new();
    let folder = sandbox.path().join("config");
    fs::create_dir_all(&folder).unwrap();
    let path = folder.join("settings.ini");
    write_atomic(&path, "a".to_owned()).unwrap();
    write_atomic(&path, "b".to_owned()).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "b");
    let names = || {
        let mut names: Vec<String> = fs::read_dir(&folder)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    };
    assert_eq!(names(), ["settings.ini"]);
    // 名前を変えられないとき（最後の名前がフォルダー）も、一時ファイルを残さない。
    let blocked = folder.join("pen.ini");
    fs::create_dir(&blocked).unwrap();
    assert!(write_atomic(&blocked, "c".to_owned()).is_err());
    assert_eq!(names(), ["pen.ini", "settings.ini"]);
    assert!(blocked.is_dir());
}

#[test]
fn atomic_write_fails_when_the_folder_is_missing() {
    let sandbox = Sandbox::new();
    let folder = sandbox
        .path()
        .join("snipping-lite-no-such-folder")
        .join("a");
    assert!(write_atomic(&folder.join("settings.ini"), "a".to_owned()).is_err());
}

#[test]
fn only_temporary_files_that_no_one_will_rename_are_removed() {
    let sandbox = Sandbox::new();
    prepare(&sandbox);
    // 終わったプロセス。ハンドルを持っている間、その ID はほかのプロセスに使い回されない。
    let mut finished = std::process::Command::new("cmd")
        .args(["/c", "exit 0"])
        .spawn()
        .expect("cmd starts");
    finished.wait().unwrap();
    let (dead, own) = (finished.id(), std::process::id());
    // `age` 秒前に最後に書かれた（負なら未来の時刻の）ファイルを作る。
    let file = |name: String, age: i64| {
        let path = folder().join(&name);
        fs::write(&path, b"x").unwrap();
        let now = std::time::SystemTime::now();
        let seconds = std::time::Duration::from_secs(age.unsigned_abs());
        let time = if age >= 0 {
            now - seconds
        } else {
            now + seconds
        };
        fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(time)
            .unwrap();
        name
    };
    let gone = [
        // 終わったプロセスが書いていた、3 つのファイルの一時ファイル。新しくても消す。
        file(format!("settings.ini.{dead}.tmp"), 0),
        file(format!("pen.ini.{dead}.tmp"), 0),
        file(format!("markup.ini.{dead}.tmp"), 0),
        // 前の版の一時ファイルで、2 分前から書かれていないもの（境目は 1 分）。
        file("settings.tmp".to_owned(), 120),
    ];
    let kept = [
        // このプロセスが書いている途中。古くても残す。
        file(format!("settings.ini.{own}.tmp"), 120),
        // 前の版の一時ファイルで、書いている途中かもしれないもの（30 秒前、時計が戻って未来）。
        file("pen.tmp".to_owned(), 30),
        file("markup.tmp".to_owned(), -3600),
        // 規則に合わない名前: ほかのファイルの一時ファイル、ID がない、数字でない、符号付き、
        // 桁あふれ、後ろに続きがある、形が違う。
        file(format!("notes.ini.{dead}.tmp"), 120),
        file("settings.ini.tmp".to_owned(), 120),
        file(format!("settings.ini.{dead}x.tmp"), 120),
        file(format!("settings.ini.+{dead}.tmp"), 120),
        file("settings.ini.4294967296.tmp".to_owned(), 120),
        file(format!("settings.ini.{dead}.tmp.bak"), 120),
        file(format!("settings.{dead}.tmp"), 120),
        file(format!("settings.ini.{dead}.partial"), 120),
    ];
    // 読むと片付く。残っていた一時ファイルの中身（"x"）は使わない。
    assert_eq!(load_settings(), before());
    assert_eq!(load_pen(), (1, 0));
    assert!(!load_markup());
    for name in &gone {
        assert!(!folder().join(name).exists(), "{name}");
    }
    let mut left: Vec<String> = kept.into_iter().collect();
    left.extend(FILES.map(str::to_owned));
    left.sort();
    assert_eq!(names(), left);
}
