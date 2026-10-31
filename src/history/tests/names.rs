//! 履歴のフォルダーにある名前のうち、項目になるのはアプリが書く名前（`{時刻}.png`、時刻は先頭に 0 や
//! 符号のない 10 進のミリ秒）のファイルだけ。ほかの名前を項目にすると、その画像の場所（`{時刻}.png`）に
//! ファイルがなく、消しても消えない項目が一覧に残る。

use super::*;

#[test]
fn only_the_names_the_program_writes_are_entries() {
    // 同値分割: アプリの名前、0、上限とその隣、符号や 0 のついた数、数字でない数、大文字の拡張子。
    for (name, id) in [
        ("1793151938315.png", Some(1793151938315)),
        ("0.png", Some(0)),
        ("253402214400000.png", Some(LAST_ID)),
        ("253402214400001.png", None),
        ("340282366920938463463374607431768211455.png", None),
        ("+1793151938315.png", None),
        ("01793151938315.png", None),
        ("00.png", None),
        ("-5.png", None),
        (" 5.png", None),
        ("５.png", None),
        ("١٢.png", None),
        ("5e3.png", None),
        (".png", None),
        ("5.PNG", None),
    ] {
        assert_eq!(parse_id(name), id, "{name}");
    }
    // 上限の時刻も 4 桁の年の日付になる（日付にできなかった時刻は 0 年になる）。
    let (year, ..) = local_time(LAST_ID);
    assert_eq!(year, 9999);
}

#[test]
fn a_name_the_program_does_not_write_is_neither_listed_nor_deleted() {
    let folder = std::env::temp_dir().join(format!("snipping-lite-names-{}", std::process::id()));
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(folder.join("7.png")).unwrap();
    for name in ["5.png", "5.thumb.png", "05.png", "+5.png", "notes.png"] {
        fs::write(folder.join(name), b"x").unwrap();
    }
    let listed = list_in(&folder);
    assert_eq!(listed, vec![entry(&folder, 5)]);
    for entry in listed {
        remove_entry(&entry);
    }
    assert!(list_in(&folder).is_empty(), "nothing is left to list");
    for name in ["05.png", "+5.png", "notes.png", "7.png"] {
        assert!(
            folder.join(name).exists(),
            "{name} is not the program's and stays"
        );
    }
    fs::remove_dir_all(&folder).unwrap();
}
