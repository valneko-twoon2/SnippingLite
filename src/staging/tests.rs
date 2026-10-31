//! 一時名、名前の付け替え、残った一時名の掃除。
//!
//! | 項目 | 技法 | ケース → 期待値 |
//! |---|---|---|
//! | 一時名 | 例 | `a.png` → `a.png.<このプロセスの ID>.partial` |
//! | 掃除の対象 | 同値分割 | png、bmp、mp4（大文字も）の書きかけ → 書いたプロセスの ID; ほかの種類、ID のない名前、数字でない ID、最後の名前 → 対象外 |
//! | 持ち主 | 状態 | このプロセス → 動いている; 終わったプロセス（ハンドルを持ち、ID が使い回されない）→ 動いていない |
//! | 掃除 | 決定表（持ち主 × 種類） | 終わったプロセスの書きかけ → 消す; 動いているプロセスの書きかけ、利用者のファイル → 残す |
//! | 付け替え | 状態 | 前のファイルがある → 置き換える; 名前を変えられない → 一時名を消して失敗を返す |

use super::*;
use crate::test_support::Sandbox;

/// 終わったプロセス。ハンドルを持っている間、その ID はほかのプロセスに使い回されない。
struct Finished(std::process::Child);

impl Finished {
    fn id(&self) -> u32 {
        self.0.id()
    }
}

fn finished_process() -> Finished {
    let mut child = std::process::Command::new("cmd")
        .args(["/c", "exit 0"])
        .spawn()
        .expect("cmd starts");
    child.wait().unwrap();
    Finished(child)
}

#[test]
fn a_file_is_written_under_a_name_that_carries_the_writer() {
    let path = Path::new("D:\\Shots\\Snipping Lite 1.png");
    assert_eq!(
        partial(path),
        PathBuf::from(format!(
            "D:\\Shots\\Snipping Lite 1.png.{}.partial",
            std::process::id()
        ))
    );
    assert_eq!(
        partial(path)
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(writer_of),
        Some(std::process::id())
    );
}

#[test]
fn only_our_pictures_and_recordings_count_as_leftovers() {
    let cases = [
        ("Snipping Lite 1.png.42.partial", Some(42)),
        ("1793151938315.thumb.png.7.partial", Some(7)),
        ("Snipping Lite 2.bmp.4294967295.partial", Some(u32::MAX)),
        ("Snipping Lite 3.mp4.9.partial", Some(9)),
        ("SHOUTED.PNG.5.partial", Some(5)),
        // 対象外: ほかの種類、書いたプロセスのない名前、数字でない ID、桁あふれ、最後の名前。
        ("notes.txt.5.partial", None),
        ("archive.5.partial", None),
        ("a.png.partial", None),
        ("a.png..partial", None),
        ("a.png.12x.partial", None),
        ("a.png.-1.partial", None),
        ("a.png.4294967296.partial", None),
        ("a.png", None),
        ("a.png.5.partial.bak", None),
    ];
    for (name, writer) in cases {
        assert_eq!(writer_of(name), writer, "{name}");
    }
}

#[test]
fn a_finished_process_is_not_alive_and_this_one_is() {
    assert!(alive(std::process::id()));
    let finished = finished_process();
    assert!(!alive(finished.id()));
}

#[test]
fn leftovers_of_finished_processes_are_removed_and_the_rest_stays() {
    let sandbox = Sandbox::new();
    let folder = sandbox.path();
    let finished = finished_process();
    let file = |name: String| {
        let path = folder.join(name);
        fs::write(&path, b"x").unwrap();
        path
    };
    let gone = [
        file(format!("Snipping Lite 1.png.{}.partial", finished.id())),
        file(format!("Snipping Lite 2.mp4.{}.partial", finished.id())),
    ];
    let kept = [
        // このプロセスが書いている途中。
        file(format!(
            "Snipping Lite 3.png.{}.partial",
            std::process::id()
        )),
        // 利用者のファイルと、書き終えたファイル。
        file(format!("notes.txt.{}.partial", finished.id())),
        file("Snipping Lite 4.png".to_owned()),
    ];
    remove_leftovers(folder);
    for path in gone {
        assert!(!path.exists(), "{path:?}");
    }
    for path in kept {
        assert!(path.exists(), "{path:?}");
    }
    // フォルダーがなくても何もしない。
    remove_leftovers(&folder.join("missing"));
}

#[test]
fn committing_replaces_the_file_or_removes_the_partial_one() {
    let sandbox = Sandbox::new();
    let path = sandbox.path().join("a.png");
    fs::write(&path, b"old").unwrap();
    let written = partial(&path);
    fs::write(&written, b"new").unwrap();
    commit(&written, &path).unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"new");
    assert!(!written.exists());
    // フォルダーのある場所へは名前を変えられない。一時名は残さない。
    let blocked = sandbox.path().join("b.png");
    fs::create_dir(&blocked).unwrap();
    let written = partial(&blocked);
    fs::write(&written, b"new").unwrap();
    assert!(commit(&written, &blocked).is_err());
    assert!(!written.exists() && blocked.is_dir());
}
