//! 名前を付けて保存とフォルダーの選択のダイアログ、保存する名前。
//!
//! | 項目 | 技法 | ケース → 期待値 |
//! |---|---|---|
//! | 保存する名前 | 同値分割（打った拡張子 × 選んだ種類） | `.png`・`.bmp`（大文字も）→ そのまま、その形式で書く; 拡張子なし、ほかの形式（`.jpg`、`.jpeg`、`.pngx`、`.txt`）、名前の一部の点（`v1.2`）→ 選んだ種類の拡張子を付け足す; 終わりの点と空白 → 落としてから見る |
//! | 開き直す | 状態 | 付け足さない → 1 回; 付け足した名前が空いている → 1 回; 打った名前が既にある → 1 回（上書きはダイアログが確かめた）; 付け足した名前が既にある → その名前と種類で開き直す、取り消せば何も返さない |
//! | ダイアログ | 例（実際のダイアログ） | 取り消す → なし; 受け入れる → 出した名前; `b` → `b.png`; `c.jpg` → `c.jpg.png` |
//! | 聞かない | 例 | フォルダーと名前をつなぐだけ |

use super::*;
use crate::test_support::{answer_dialog, Sandbox};

#[test]
fn the_saved_name_always_ends_in_the_extension_of_the_format_written() {
    // （打った名前、選んだ種類（1 が PNG、2 が BMP）、保存する名前、BMP で書くか）。
    let cases = [
        ("shot", 1, "shot.png", false),
        ("shot", 2, "shot.bmp", true),
        // .png と .bmp で終わる名前は、選んだ種類にかかわらず、その形式で書く。
        ("shot.png", 2, "shot.png", false),
        ("shot.bmp", 1, "shot.bmp", true),
        ("SHOT.PNG", 2, "SHOT.PNG", false),
        ("shot.Bmp", 1, "shot.Bmp", true),
        // ほかの形式の拡張子は残して、選んだ種類の拡張子を付け足す。
        ("shot.jpg", 1, "shot.jpg.png", false),
        ("shot.jpg", 2, "shot.jpg.bmp", true),
        ("写真 1.jpeg", 1, "写真 1.jpeg.png", false),
        ("shot.pngx", 1, "shot.pngx.png", false),
        ("shot.png.txt", 2, "shot.png.txt.bmp", true),
        ("v1.2", 1, "v1.2.png", false),
        // Windows が落とす終わりの点と空白は、落としてから見る。
        ("shot.", 1, "shot.png", false),
        ("shot.png. ", 2, "shot.png", false),
        ("shot.jpg ", 1, "shot.jpg.png", false),
    ];
    let folder = Path::new("D:\\Shots");
    for (typed, selected, saved, bmp) in cases {
        let path = saved_name(folder.join(typed), selected);
        assert_eq!(path, folder.join(saved), "{typed:?} ({selected})");
        assert_eq!(
            is_bmp(&path),
            bmp,
            "{typed:?} ({selected}) is written as BMP"
        );
    }
}

/// ダイアログに出した場所、名前、選んでおいた種類。
type Shown = (PathBuf, String, u32);

/// 名前を付けて保存を、ダイアログの代わりに `answers` で答えて進める。答えは開かれた順に使う。
/// `None` は取り消し、`Some((name, chosen))` は出された場所の `name`（`None` なら出された名前のまま）を
/// 種類 `chosen` で保存する。返すのは保存する場所と、開かれたダイアログに出したもの。
fn settle_with(
    folder: &Path,
    name: &str,
    answers: &[Option<(Option<&str>, u32)>],
) -> (Option<PathBuf>, Vec<Shown>) {
    let mut shown = Vec::new();
    let mut answers = answers.iter();
    let saved = settle(folder, name, |folder, name, index| {
        shown.push((folder.to_owned(), name.to_owned(), index));
        let (typed, chosen) = (*answers.next().expect("no more dialogs than answers"))?;
        Some((folder.join(typed.unwrap_or(name)), chosen))
    });
    (saved, shown)
}

#[test]
fn the_dialog_opens_again_only_when_the_added_extension_meets_a_file() {
    let sandbox = Sandbox::new();
    let folder = sandbox.path();
    for name in ["shot.png", "taken.jpg.png", "taken.jpg.bmp"] {
        fs::write(folder.join(name), b"mine").unwrap();
    }
    let shown = |name: &str, index| (folder.to_owned(), name.to_owned(), index);
    // 出した名前のまま保存する。取り消せば何も返さない。
    let (saved, seen) = settle_with(folder, "a.png", &[Some((None, 1))]);
    assert_eq!(saved, Some(folder.join("a.png")));
    assert_eq!(seen, [shown("a.png", 1)]);
    assert_eq!(
        settle_with(folder, "a.png", &[None]),
        (None, vec![shown("a.png", 1)])
    );
    // 付け足した名前が空いている。
    let (saved, seen) = settle_with(folder, "a.png", &[Some((Some("shot.jpg"), 1))]);
    assert_eq!((saved, seen.len()), (Some(folder.join("shot.jpg.png")), 1));
    // 打った名前のファイルが既にある。上書きしてよいかはダイアログが確かめたので、開き直さない。
    let (saved, seen) = settle_with(folder, "a.png", &[Some((Some("shot.png"), 2))]);
    assert_eq!((saved, seen.len()), (Some(folder.join("shot.png")), 1));
    // 付け足した名前のファイルが既にある。その名前と選んだ種類で開き直し、そこで保存すれば上書きする。
    let answers = [Some((Some("taken.jpg"), 2)), Some((None, 2))];
    let (saved, seen) = settle_with(folder, "a.png", &answers);
    assert_eq!(saved, Some(folder.join("taken.jpg.bmp")));
    assert_eq!(seen, [shown("a.png", 1), shown("taken.jpg.bmp", 2)]);
    // 開き直したダイアログを取り消せば、何も返さない。既にあるファイルはそのまま。
    let (saved, seen) = settle_with(folder, "a.png", &[Some((Some("taken.jpg"), 1)), None]);
    assert_eq!(saved, None);
    assert_eq!(seen, [shown("a.png", 1), shown("taken.jpg.png", 1)]);
    assert_eq!(fs::read(folder.join("taken.jpg.png")).unwrap(), b"mine");
}

#[test]
fn without_asking_the_path_is_the_folder_and_name() {
    let folder = Path::new("D:\\Shots");
    assert_eq!(
        choose_path(null_mut(), folder, "a.png", false),
        Some(PathBuf::from("D:\\Shots\\a.png"))
    );
}

#[test]
#[ignore = "Shows dialogs"]
fn the_save_dialogs_return_the_chosen_path_or_nothing() {
    let folder = scratch("dialogs");
    fs::create_dir_all(&folder).unwrap();
    // Cancelling gives nothing.
    let answer = answer_dialog(2);
    assert_eq!(choose_path(null_mut(), &folder, "a.png", true), None);
    assert!(answer.join().unwrap());
    // Accepting keeps the proposed name; a name without an extension gets the default one.
    let answer = answer_dialog(1);
    assert_eq!(
        choose_path(null_mut(), &folder, "a.png", true),
        Some(folder.join("a.png"))
    );
    assert!(answer.join().unwrap());
    let answer = answer_dialog(1);
    assert_eq!(
        choose_path(null_mut(), &folder, "b", true),
        Some(folder.join("b.png"))
    );
    assert!(answer.join().unwrap());
    // ほかの形式の拡張子で終わる名前には、選んだ種類（PNG）の拡張子が付く。
    let answer = answer_dialog(1);
    assert_eq!(
        choose_path(null_mut(), &folder, "c.jpg", true),
        Some(folder.join("c.jpg.png"))
    );
    assert!(answer.join().unwrap());
}

#[test]
#[ignore = "Shows a dialog"]
fn the_folder_dialog_returns_the_current_folder_or_nothing() {
    let folder = scratch("folders");
    fs::create_dir_all(&folder).unwrap();
    let answer = answer_dialog(2);
    assert_eq!(choose_folder(null_mut(), &folder), None);
    assert!(answer.join().unwrap());
    let answer = answer_dialog(1);
    assert_eq!(choose_folder(null_mut(), &folder), Some(folder.clone()));
    assert!(answer.join().unwrap());
    // A folder that does not exist still opens the dialog.
    let answer = answer_dialog(2);
    assert_eq!(choose_folder(null_mut(), &folder.join("missing")), None);
    assert!(answer.join().unwrap());
}
