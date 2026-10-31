//! 履歴のフォルダーの中身のファジング。アプリの名前の項目（サムネイルがあるものとないもの）に、
//! サムネイルだけのもの、アプリの名前に似た名前、項目の名前のフォルダー、関係のないファイルを混ぜる。
//! オラクルは小さなモデル: 一覧はアプリの名前のファイルの時刻を新しい順に重なりなく並べたもので、
//! 各項目の画像はそのファイル、時刻は日付にでき、古いものを減らすと新しいほうが残り、すべて消しても
//! アプリのものでない名前は残る。

use super::*;
use crate::test_support::rng::{self, Reach, Rng};

/// アプリの名前に似ているが、アプリが書かない名前。Windows は名前の末尾の空白と点を落とすので、
/// 末尾には置かない。
fn odd_name(rng: &mut Rng, id: u128) -> String {
    match rng.below(12) {
        0 => format!("+{id}.png"),
        1 => format!("0{id}.png"),
        2 => format!("-{id}.png"),
        3 => format!(" {id}.png"),
        4 => format!("{id}.PNG"),
        5 => format!("{id}.png.png"),
        6 => format!("{id}e0.png"),
        7 => format!("{id}.5.png"),
        8 => format!("\u{feff}{id}.png"),
        9 => format!("{id}.tmp"),
        10 => format!("{}{id}.png", LAST_ID),
        _ => format!("{id}\u{0661}.png"),
    }
}

/// 時刻。多くは今ごろの時刻、ときどき端の値（0、上限、上限の次、u128 の最大）。
fn time(rng: &mut Rng) -> u128 {
    match rng.below(8) {
        0 => *rng.pick(&[0, 1, LAST_ID, LAST_ID + 1, u64::MAX as u128, u128::MAX]),
        _ => rng.range(1_700_000_000_000u64..=1_900_000_000_000) as u128,
    }
}

#[test]
fn a_folder_of_any_names_lists_only_the_captures_newest_first() {
    let base = std::env::temp_dir().join(format!("snipping-lite-folder-{}", std::process::id()));
    let mut reach = Reach::new(&[
        "entry",
        "entry without thumbnail",
        "thumbnail only",
        "odd name",
        "folder",
        "foreign",
        "edge time",
    ]);
    let cases = rng::check(
        "a_folder_of_any_names_lists_only_the_captures_newest_first",
        200,
        |rng| {
            let folder = base.join("case");
            let _ = fs::remove_dir_all(&folder);
            fs::create_dir_all(&folder).unwrap();
            let mut used = std::collections::HashSet::new();
            let (mut entries, mut foreign) = (Vec::new(), Vec::new());
            for _ in 0..rng.range(0..=12usize) {
                let id = time(rng);
                // 大文字と小文字を区別しないファイルシステムで、同じ時刻の名前が重ならないように。
                if !used.insert(id) {
                    continue;
                }
                let kind = rng.below(6);
                let name = match kind {
                    0 | 1 => format!("{id}.png"),
                    2 => format!("{id}.thumb.png"),
                    3 => odd_name(rng, id),
                    4 => format!("{id}.png"),
                    _ => (*rng.pick(&["notes.png", "desktop.ini", "Thumbs.db", "a.thumb.png"]))
                        .to_owned(),
                };
                let path = folder.join(&name);
                if path.exists() {
                    continue;
                }
                rng.note(format!("{name} ({kind})"));
                if kind == 4 {
                    fs::create_dir(&path).unwrap();
                } else {
                    fs::write(&path, b"x").unwrap();
                }
                let canonical = kind <= 1 && id <= LAST_ID;
                if canonical {
                    entries.push(id);
                    if kind == 0 {
                        fs::write(folder.join(format!("{id}.thumb.png")), b"x").unwrap();
                    }
                } else {
                    foreign.push(path);
                }
                reach.hit(match kind {
                    0 | 1 if !canonical => "edge time",
                    0 => "entry",
                    1 => "entry without thumbnail",
                    2 => "thumbnail only",
                    3 => "odd name",
                    4 => "folder",
                    _ => "foreign",
                });
            }
            entries.sort_unstable_by(|a, b| b.cmp(a));
            let listed = list_in(&folder);
            let ids: Vec<u128> = listed.iter().map(|e| e.id).collect();
            assert_eq!(ids, entries, "the captures, newest first");
            for e in &listed {
                assert_eq!(e.image, folder.join(format!("{}.png", e.id)));
                assert!(
                    e.image.is_file(),
                    "{:?} is the file that was found",
                    e.image
                );
                let (year, month, day, hour, minute) = local_time(e.id);
                assert!(
                    (1969..=9999).contains(&year)
                        && (1..=12).contains(&month)
                        && (1..=31).contains(&day)
                        && hour < 24
                        && minute < 60,
                    "{} is a date",
                    e.id
                );
            }
            // 古いものを減らすと新しいほうが残り、すべて消すと何も並ばない。
            let limit = rng.below(entries.len() + 1);
            prune_in(&folder, limit);
            let kept: Vec<u128> = list_in(&folder).iter().map(|e| e.id).collect();
            assert_eq!(kept, entries[..limit], "pruned to {limit}");
            for e in list_in(&folder) {
                remove_entry(&e);
            }
            assert!(list_in(&folder).is_empty(), "nothing is left to list");
            for path in &foreign {
                assert!(path.exists(), "{path:?} is not the program's and stays");
            }
        },
    );
    reach.finish("a_folder_of_any_names", cases);
    let _ = fs::remove_dir_all(&base);
}
