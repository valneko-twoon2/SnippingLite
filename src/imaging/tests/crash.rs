//! クラッシュ時の整合性: 名前を付けて保存で選んだフォルダーへの保存を、画像を書いたところで
//! 子プロセスごと殺す。
//!
//! 名前を付けて保存は、ダイアログが返した場所へ `encode` で書く（`screenshot::save_image` と履歴の
//! ウィンドウ）。既定の保存先と違い、そのフォルダーを用意する処理はないので、止まったプロセスの
//! 書きかけは、次にそのフォルダーへ保存する `encode` が消す。利用者のファイル、書き終えた録画の
//! 別名（`staging::keep` の `(2).mp4`）、一時名に似ているが規則に合わない名前、動いているプロセスが
//! 書いている途中の書きかけには触れない。
//!
//! 利用者がほかの形式の拡張子（`chosen.jpg`）を打っても、保存する名前は選んだ種類の拡張子で終わる
//! （`chosen.jpg.png`）。書きかけ（`chosen.jpg.png.<ID>.partial`）も掃除の規則に合い、次の保存で消える。

use super::*;
use crate::test_support::{crash, Sandbox};

/// 利用者が名前を付けて保存で選んだフォルダー（サンドボックスのデスクトップ）。子も同じ環境を受け継ぐ。
fn chosen() -> PathBuf {
    PathBuf::from(std::env::var_os("USERPROFILE").unwrap()).join("Desktop")
}

/// 4x3 の画像。画素はどれも違う。
fn picture() -> Vec<u8> {
    (0..12u8).flat_map(|i| [i, 20 * i, 255 - i, 255]).collect()
}

fn names() -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(chosen())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// 利用者がもともと置いていたファイル。どれも消えてはいけない。
const MINE: [&str; 6] = [
    "notes.txt",
    "photo.png",
    // 名前を変えられなかった録画を `staging::keep` が残した名前。
    "Snipping Lite 1 (2).mp4",
    // 一時名に似ているが規則に合わない: 画像と録画でない種類、数字でない ID、ID がない。
    "notes.txt.4242.partial",
    "photo.png.12x.partial",
    "photo.png.partial",
];

fn prepare(_: &Sandbox) {
    fs::create_dir_all(chosen()).unwrap();
    for name in MINE {
        fs::write(chosen().join(name), b"mine").unwrap();
    }
}

/// フォルダーにあるはずの名前: 利用者のファイルと `more`。
fn expected(more: &[&str]) -> Vec<String> {
    let mut names: Vec<String> = MINE.iter().chain(more).map(|n| n.to_string()).collect();
    names.sort();
    names
}

fn mine_untouched() {
    for name in MINE {
        assert_eq!(fs::read(chosen().join(name)).unwrap(), b"mine", "{name}");
    }
}

/// 名前を付けて保存で `typed` を打って保存する（ダイアログの代わりに、出された場所に打った名前で
/// 答える。PNG を選んだまま）。
fn save_as(typed: &str) {
    let path = settle(&chosen(), "Snipping Lite 1.png", |folder, _, _| {
        Some((folder.join(typed), 1))
    })
    .unwrap();
    encode(&path, &picture(), 4, 3).unwrap();
}

#[test]
#[ignore = "Runs only as the child process of a crash test"]
fn child_saves_a_picture_where_the_save_dialog_points() {
    if !crash::is_child() {
        return;
    }
    save_as("chosen.png");
}

#[test]
#[ignore = "Runs only as the child process of a crash test"]
fn child_saves_a_picture_typed_with_another_extension() {
    if !crash::is_child() {
        return;
    }
    save_as("chosen.jpg");
}

/// 子が `saved`（保存する名前）へ書いたところで殺し、残ったものと次の保存の片付けを確かめる。
fn killed_and_tidied(child: &str, saved: &'static str) {
    let child = crash::test_name(module_path!(), child);
    let check = |_: &Sandbox, crashed: &crash::Crash| {
        if crashed.stopped.is_some() {
            let partial = format!("{saved}.{}.partial", crashed.id());
            assert_eq!(names(), expected(&[&partial]), "only the half-written one");
            // 次に同じフォルダーへ保存すると、止まったプロセスの書きかけだけが消える。
            encode(&chosen().join("next.png"), &picture(), 4, 3).unwrap();
            assert_eq!(names(), expected(&["next.png"]));
            assert_eq!(decode(&chosen().join("next.png")), Some((4, 3, picture())));
        } else {
            assert_eq!(names(), expected(&[saved]));
            assert_eq!(decode(&chosen().join(saved)), Some((4, 3, picture())));
            // 名前のとおり PNG で書いてある。
            let bytes = fs::read(chosen().join(saved)).unwrap();
            assert!(bytes.starts_with(b"\x89PNG"), "{saved} is a PNG");
        }
        mine_untouched();
    };
    let stages = crash::sweep(&child, 5, prepare, check);
    assert_eq!(stages, [format!("picture written: {saved}")]);
}

#[test]
fn a_picture_saved_as_and_killed_part_way_is_tidied_by_the_next_save_there() {
    killed_and_tidied(
        "child_saves_a_picture_where_the_save_dialog_points",
        "chosen.png",
    );
}

#[test]
fn a_picture_typed_as_chosen_jpg_is_saved_as_png_and_tidied_by_the_next_save() {
    killed_and_tidied(
        "child_saves_a_picture_typed_with_another_extension",
        "chosen.jpg.png",
    );
}

#[test]
fn a_picture_another_process_is_saving_there_is_left_alone() {
    let child = crash::test_name(
        module_path!(),
        "child_saves_a_picture_where_the_save_dialog_points",
    );
    let sandbox = Sandbox::new();
    prepare(&sandbox);
    // 子は画像を一時名で書き終え、名前を変える前で、動いたまま待っている。
    let mut writer = crash::pause(&sandbox, &child, 1);
    assert_eq!(
        writer.stopped.as_deref(),
        Some("picture written: chosen.png")
    );
    let partial = format!("chosen.png.{}.partial", writer.id());
    let written = fs::read(chosen().join(&partial)).unwrap();
    encode(&chosen().join("next.png"), &picture(), 4, 3).unwrap();
    assert_eq!(names(), expected(&[&partial, "next.png"]));
    assert_eq!(fs::read(chosen().join(&partial)).unwrap(), written);
    // もう一方が止まったら、次の保存で消える。
    writer.kill();
    encode(&chosen().join("last.png"), &picture(), 4, 3).unwrap();
    assert_eq!(names(), expected(&["last.png", "next.png"]));
    mine_untouched();
}
