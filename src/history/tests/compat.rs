//! 古い版が書いた履歴のフォルダーを、今の版が読めること。履歴の書式（`{時刻}.png` と
//! `{時刻}.thumb.png`、索引のファイルなし）は最初の版（38e7147、2026-10-28）から変わっていない。
//! 見本はその版のコードの `history::add` に 400 x 100 の絵（左半分が不透明な赤、右半分がアルファ 128 の
//! 青）を渡して作り、そのまま置いた（作り直さない）。今の書式の固定テストは、書式を変えたときに、
//! 古いファイルを読めなくなっていないかを確かめ直すきっかけになる。

use super::*;
use crate::test_support::png::{self as png_parts, Header};

/// 38e7147 が書いた見本の時刻（ファイル名）。2026-10-31 17:17:35.838 UTC。
const OLD_ID: u128 = 1_793_467_055_838;
const OLD_IMAGE: &[u8] = include_bytes!("compat/1793467055838.png");
const OLD_THUMBNAIL: &[u8] = include_bytes!("compat/1793467055838.thumb.png");

/// 見本の元の絵の画素（BGRA、上から下）。
fn old_picture() -> Vec<u8> {
    let row: Vec<u8> = (0..400)
        .flat_map(|x| {
            if x < 200 {
                [0, 0, 255, 255]
            } else {
                [255, 0, 0, 128]
            }
        })
        .collect();
    row.repeat(100)
}

/// サンドボックスの履歴のフォルダーに、古い版の見本をその名前で置き、その項目を返す。
fn place_old_history() -> Entry {
    let folder = folder().unwrap();
    fs::create_dir_all(&folder).unwrap();
    let old = Entry {
        id: OLD_ID,
        image: folder.join("1793467055838.png"),
        thumbnail: folder.join("1793467055838.thumb.png"),
    };
    fs::write(&old.image, OLD_IMAGE).unwrap();
    fs::write(&old.thumbnail, OLD_THUMBNAIL).unwrap();
    old
}

#[test]
fn a_history_written_by_the_first_version_lists_and_reads_as_it_was_stored() {
    let _sandbox = crate::test_support::Sandbox::new();
    let old = place_old_history();
    let listed = list();
    assert_eq!(listed, vec![old]);
    let (width, height, pixels) = imaging::decode(&listed[0].image).unwrap();
    assert_eq!((width, height), (400, 100));
    assert!(
        pixels == old_picture(),
        "the stored picture is the one captured"
    );
    // サムネイルは 320 x 200 に収まるよう縮める: min(320 / 400, 200 / 100) = 0.8 で 320 x 80。
    // 色の一様な部分は、縮めても同じ色のまま。
    let (width, height, thumbnail) = imaging::decode(&listed[0].thumbnail).unwrap();
    assert_eq!((width, height), (320, 80));
    let at = |x: usize, y: usize| &thumbnail[(y * 320 + x) * 4..][..4];
    assert_eq!(at(10, 40), [0, 0, 255, 255]);
    assert_eq!(at(310, 40), [255, 0, 0, 128]);
    // 時刻はどの時間帯でも 2026 年 10 月 31 日か 11 月 1 日。
    let (year, month, day, ..) = local_time(OLD_ID);
    assert!((year, month, day) == (2026, 10, 31) || (year, month, day) == (2026, 11, 1));
}

#[test]
fn a_history_written_by_the_first_version_still_takes_new_captures_and_edits() {
    let _sandbox = crate::test_support::Sandbox::new();
    let old = place_old_history();
    // 新しく撮ると、古い項目と並ぶ（時刻の新しい順。機械の時計には頼らない）。
    add(&[30, 60, 90, 255].repeat(6), 3, 2, None).unwrap();
    let entries = list();
    assert_eq!(entries.len(), 2);
    assert!(entries.contains(&old));
    assert!(entries[0].id > entries[1].id);
    // 古い項目を書き込みの後で置き換え、消せる。
    replace(&old, &[1, 2, 3, 255].repeat(8), 4, 2).unwrap();
    assert_eq!(imaging::decode(&old.image).unwrap().0, 4);
    assert_eq!(imaging::decode(&old.thumbnail).unwrap().0, 4);
    remove_entry(&old);
    assert!(!old.image.exists() && !old.thumbnail.exists());
    assert_eq!(list().len(), 1);
}

#[test]
fn the_current_version_writes_the_history_in_the_layout_of_the_first() {
    // 固定テスト: 1 回の撮影は `{時刻}.png` と `{時刻}.thumb.png` の 2 つのファイルだけになり、
    // どちらも 8 ビットの RGBA の PNG で、サムネイルは 320 x 200 に収まる。
    let _sandbox = crate::test_support::Sandbox::new();
    add(&old_picture(), 400, 100, None).unwrap();
    let folder = folder().unwrap();
    let id = list()[0].id;
    let mut names: Vec<String> = fs::read_dir(&folder)
        .unwrap()
        .map(|f| f.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    assert_eq!(names, [format!("{id}.png"), format!("{id}.thumb.png")]);
    for (name, size) in [("png", (400, 100)), ("thumb.png", (320, 80))] {
        let bytes = fs::read(folder.join(format!("{id}.{name}"))).unwrap();
        let header = Header::read(&png_parts::parse(&bytes).unwrap().chunks[0].data).unwrap();
        assert_eq!(header, Header::rgba(size.0, size.1), "{name}");
    }
    // 今の版が書いた絵は、古い版の見本と同じように読める。
    let (_, _, pixels) = imaging::decode(&folder.join(format!("{id}.png"))).unwrap();
    assert!(pixels == old_picture());
    let old = Header::read(&png_parts::parse(OLD_IMAGE).unwrap().chunks[0].data).unwrap();
    assert_eq!(old, Header::rgba(400, 100));
}
