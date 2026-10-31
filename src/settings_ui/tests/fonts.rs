//! フォントファイルのフェイスの数（`face_count`）。TrueType Collection（.ttc）は先頭 12 バイトの
//! ヘッダーに `ttcf` のタグと、ビッグエンディアンの 32 ビットのフォントの数（8〜11 バイト目）を持つ
//! （OpenType の仕様の TTC Header）。それ以外のフォントファイルは 1 つのフェイスだけを持つ。
//! 数が 0 のファイルは、どの番号のフェイスも読めないので使わない。
//!
//! ヘッダーの数だけを信じると、途中で切れた TTC などを渡して epaint が最初のフレームで panic する。
//! そのため、渡す前に `face_fits` で、選んだフェイスの中身がファイルに収まっているかを確かめる
//! （OpenType の仕様の Table Directory: 版の印 4 バイト、テーブルの数 2 バイト、検索用の 6 バイト、
//! 続いてテーブルごとにタグ、チェックサム、位置、長さの 16 バイト。位置はファイルの先頭から）。
//! 収まっていても、描くのに要るテーブル（OpenType の仕様の Required Tables のうち cmap、head、hhea、
//! hmtx、maxp と、輪郭の glyf と loca、CFF、CFF2 のどれか）が読めないフェイスは、文字が欠けるので
//! 使わない。読み手（read-fonts）は位置が 0 の行と、ファイルの外へはみ出すテーブルを無いものとして扱い、
//! 長さ 0 のテーブルからは何も読めない。
//!
//! | 項目 | 技法 | ケース → 期待値 |
//! |---|---|---|
//! | 長さ | 境界値（バイト） | 0、11 バイト → 0; ちょうど 12 バイト → 読める; 後ろに続くデータ → ヘッダーだけで決まる |
//! | タグ | 同値分割 | `ttcf` → ヘッダーの数; TrueType、`OTTO`、大文字違い、先頭以外の `ttcf` → 1 |
//! | 数 | 境界値 | 0、1、2、u32::MAX; バイトの順（256、2^24） |
//! | 壊れた入力 | 悪意ある入力 | 0〜16 バイトの切れ端、0x00 と 0xff だけの列 → panic しない |
//! | フェイスの印 | 同値分割 | 0x00010000、`true`、`OTTO` → 使う; `wOFF`、0、`TTCF` → 使わない |
//! | 切れたファイル | 境界値（バイト） | 1 バイト短い〜0 バイトのどこで切れても → 使わない; 後ろに余分 → 使う |
//! | テーブルの範囲 | 境界値（バイト） | ちょうど終わりまで → 使う; 1 バイト越える、真ん中のテーブルだけ越える、空のテーブルが終わりの先 → 使わない; 足すとあふれる値 → panic しない |
//! | TTC | 同値分割 | 頼んだ番号のフェイスだけを見る; 番号が数以上、位置が外、位置に印がない → 使わない |
//! | TTC のヘッダー | 境界値、悪意ある入力 | 位置の表の途中で切れた → どの番号も使わない; 数が u32::MAX → 使わない |
//! | 要るテーブル | 同値分割、境界値（バイト） | cmap、glyf、head、hhea、hmtx、loca、maxp のどれか 1 つが無い、位置が 0、長さが 0、終わりを 1 バイト越える → 使わない; 長さ 1、ちょうど終わりまで → 使う |
//! | 輪郭 | 決定表 | glyf と loca、`CFF ` だけ、`CFF2` だけ → 使う; glyf か loca の片方だけ、どれも無い、大文字小文字や空白の違うタグ → 使わない |
//! | TTC の 1 つだけ欠ける | 同値分割 | 要るテーブルが欠けたフェイスだけ使わず、ほかのフェイスは使う |
//! | 本物のフォント | 回帰 | 手元の Windows にある候補はすべて使える |
//! | 読み込み | 結合 | 最初の候補が切れた TTC → 次の候補で文字を描き、panic しない |

use super::*;

/// `count` 個のフェイスを持つことを示す TTC のヘッダー（バージョン 2.0）。
fn collection(count: [u8; 4]) -> Vec<u8> {
    [b"ttcf".as_slice(), &[0, 2, 0, 0], &count].concat()
}

#[test]
fn a_file_shorter_than_the_header_has_no_face_and_one_of_exactly_the_header_is_read() {
    assert_eq!(face_count(&[]), 0);
    let header = collection([0, 0, 0, 2]);
    // 11 バイトでは数の最後のバイトが欠ける。タグが合っていても読まない。
    assert_eq!(face_count(&header[..11]), 0);
    assert_eq!(face_count(&header), 2);
    // ふつうのフォントでも 12 バイトに満たなければ使わない。
    let plain = [0, 1, 0, 0, 0, 0x10, 0x01, 0x00, 0x00, 0x04, 0x00, 0x00];
    assert_eq!(face_count(&plain[..11]), 0);
    assert_eq!(face_count(&plain), 1);
}

#[test]
fn only_a_ttcf_tag_at_the_start_makes_a_collection() {
    // 数の欄に 5 を置き、コレクションと読んだときだけ 5 になるようにする。
    let count = [0, 0, 0, 5];
    assert_eq!(face_count(&collection(count)), 5);
    for tag in [
        [0x00, 0x01, 0x00, 0x00], // TrueType
        *b"OTTO",                 // CFF の OpenType
        *b"true",
        *b"TTCF",
        *b"ttcF",
        *b"ttc ",
    ] {
        let bytes = [tag.as_slice(), &[0, 2, 0, 0], &count].concat();
        assert_eq!(face_count(&bytes), 1, "{tag:?}");
    }
    // 先頭以外にある `ttcf` は、ただのデータ。
    let shifted = [b"OTTOttcf".as_slice(), &count].concat();
    assert_eq!(face_count(&shifted), 1);
}

#[test]
fn the_count_is_a_big_endian_number_from_zero_to_the_largest() {
    for (count, expected) in [
        ([0, 0, 0, 0], 0),
        ([0, 0, 0, 1], 1),
        ([0, 0, 0, 2], 2),
        ([0, 0, 1, 0], 256),
        ([1, 0, 0, 0], 1 << 24),
        ([0xff, 0xff, 0xff, 0xff], u32::MAX),
    ] {
        assert_eq!(face_count(&collection(count)), expected, "{count:?}");
    }
}

#[test]
fn data_after_the_header_does_not_change_the_count() {
    // 本物の TTC はヘッダーの後ろに各フォントの位置の表とフォント本体が続く。
    let mut bytes = collection([0, 0, 0, 3]);
    bytes.extend_from_slice(&[0, 0, 0, 0x1c, 0, 0, 0x01, 0x00, 0, 0, 0x02, 0x00]);
    bytes.extend(std::iter::repeat_n(0xab, 1000));
    assert_eq!(face_count(&bytes), 3);
}

#[test]
fn broken_files_never_panic() {
    let header = collection([0, 0, 0, 2]);
    for length in 0..=header.len() {
        let expected = if length < 12 { 0 } else { 2 };
        assert_eq!(face_count(&header[..length]), expected, "{length} bytes");
    }
    for fill in [0x00, 0xff] {
        for length in 0..=16 {
            let expected = u32::from(length >= 12);
            assert_eq!(
                face_count(&vec![fill; length]),
                expected,
                "{length} x {fill}"
            );
        }
    }
}

/// 手で組むフェイスがどれも持つ、描くのに要るテーブル（TrueType の輪郭のもの）。
const NEEDED: [&[u8; 4]; 7] = [
    b"cmap", b"glyf", b"head", b"hhea", b"hmtx", b"loca", b"maxp",
];
/// 要るテーブルの中身の長さ（バイト）。中身までは確かめないので、値は何でもよい。
const NEEDED_LENGTH: usize = 4;

/// テーブルの一覧の 1 行（タグ、チェックサム、位置、長さ）を足す。
fn push_record(bytes: &mut Vec<u8>, tag: &[u8], offset: usize, length: u32) {
    bytes.extend_from_slice(tag);
    bytes.extend_from_slice(&[0; 4]);
    bytes.extend_from_slice(&(offset as u32).to_be_bytes());
    bytes.extend_from_slice(&length.to_be_bytes());
}

/// フォントの中のフェイス: TrueType の印、テーブルの数、3 つの検索用の欄、テーブルの一覧
/// （タグ、チェックサム、位置、長さ）と、一覧の後ろに並べた各テーブルの中身。位置はファイルの
/// 先頭から数えるので、フェイスを `base` バイト目に置くときはそれを渡す。一覧は `lengths` の
/// テーブル（`tab0`、`tab1`…）から始まり、`NEEDED` のテーブルが続く。中身は `NEEDED` を先に
/// 置くので、`lengths` の最後のテーブルがフェイスの終わりで終わる。
fn face_at(base: usize, lengths: &[u32]) -> Vec<u8> {
    let tables = lengths.len() + NEEDED.len();
    let mut bytes = vec![0, 1, 0, 0];
    bytes.extend_from_slice(&(tables as u16).to_be_bytes());
    bytes.extend_from_slice(&[0; 6]);
    let contents = base + 12 + 16 * tables;
    let mut offset = contents + NEEDED_LENGTH * NEEDED.len();
    for (index, &length) in lengths.iter().enumerate() {
        push_record(&mut bytes, format!("tab{index}").as_bytes(), offset, length);
        offset += length as usize;
    }
    for (index, tag) in NEEDED.iter().enumerate() {
        let at = contents + NEEDED_LENGTH * index;
        push_record(&mut bytes, tag.as_slice(), at, NEEDED_LENGTH as u32);
    }
    bytes.extend(std::iter::repeat_n(0xee, NEEDED_LENGTH * NEEDED.len()));
    for (index, &length) in lengths.iter().enumerate() {
        bytes.extend(std::iter::repeat_n(index as u8 + 1, length as usize));
    }
    bytes
}

/// 1 つのフェイスだけのフォントファイル（.ttf）。
fn font(lengths: &[u32]) -> Vec<u8> {
    face_at(0, lengths)
}

/// フェイスを並べた TTC。ヘッダー（バージョン 2.0）、位置の表、各フェイスの順に置く。
fn collection_of(faces: &[&[u32]]) -> Vec<u8> {
    let count = faces.len() as u32;
    let mut bytes = collection(count.to_be_bytes());
    let mut base = 12 + 4 * faces.len();
    let mut bodies = Vec::new();
    for lengths in faces {
        bytes.extend_from_slice(&(base as u32).to_be_bytes());
        let body = face_at(base, lengths);
        base += body.len();
        bodies.extend(body);
    }
    bytes.extend(bodies);
    bytes
}

/// `face` バイト目から始まるフェイスの、テーブル `table` の一覧の欄（タグ、位置か長さ）を書き換える。
fn set_record(bytes: &mut [u8], face: usize, table: usize, field: usize, value: u32) {
    let at = face + 12 + 16 * table + field;
    bytes[at..at + 4].copy_from_slice(&value.to_be_bytes());
}
const TAG: usize = 0;
const OFFSET: usize = 8;
const LENGTH: usize = 12;

/// `face` バイト目から始まるフェイスの一覧で、`tag` のテーブルの番号。
fn table_of(bytes: &[u8], face: usize, tag: &[u8; 4]) -> usize {
    let tables = u16::from_be_bytes([bytes[face + 4], bytes[face + 5]]) as usize;
    (0..tables)
        .find(|table| bytes[face + 12 + 16 * table..][..4] == tag[..])
        .unwrap_or_else(|| panic!("the face has no {:?} table", tag_name(tag)))
}

fn tag_name(tag: &[u8; 4]) -> String {
    String::from_utf8_lossy(tag).into_owned()
}

/// 描くのに使わないタグ。要るテーブルのタグをこれに替えると、そのテーブルは無いことになる。
const UNUSED: u32 = u32::from_be_bytes(*b"zzzz");

#[test]
fn a_whole_font_is_used_at_its_only_face() {
    let bytes = font(&[4, 8, 2]);
    assert!(face_fits(&bytes, 0));
    assert!(!face_fits(&bytes, 1), "a plain font has one face");
    // TrueType の別の印と CFF の印も読める。ほかの印（WOFF、ゼロ、`ttcf` の大文字違い）は読めない。
    for (tag, expected) in [
        (*b"true", true),
        (*b"OTTO", true),
        (*b"wOFF", false),
        ([0, 0, 0, 0], false),
        (*b"TTCF", false),
    ] {
        let mut tagged = bytes.clone();
        tagged[..4].copy_from_slice(&tag);
        assert_eq!(face_fits(&tagged, 0), expected, "{tag:?}");
    }
}

#[test]
fn a_font_cut_short_anywhere_is_not_used() {
    let bytes = font(&[4, 8, 2]);
    // ヘッダーの途中、テーブルの一覧の途中、テーブルの中身の途中、最後のテーブルの 1 バイト手前。
    for length in 0..bytes.len() {
        assert!(
            !face_fits(&bytes[..length], 0),
            "{length} of {} bytes",
            bytes.len()
        );
    }
    // 後ろに余分なデータが続いても読める。
    let mut longer = bytes.clone();
    longer.extend_from_slice(&[0; 16]);
    assert!(face_fits(&longer, 0));
}

#[test]
fn a_table_that_runs_past_the_end_of_the_file_is_not_used() {
    let bytes = font(&[4, 8, 2]);
    let end = bytes.len() as u32;
    // 境界値: 最後のテーブル（位置 end - 2）がちょうどファイルの終わりまでなら読め、1 バイトでも越えると読めない。
    let mut longer = bytes.clone();
    set_record(&mut longer, 0, 2, LENGTH, 3);
    assert!(!face_fits(&longer, 0), "one byte past the end");
    // 真ん中のテーブルがはみ出しても、最後のテーブルが収まっていても読めない。
    let mut middle = bytes.clone();
    set_record(&mut middle, 0, 1, OFFSET, end - 4);
    assert!(!face_fits(&middle, 0), "the middle table runs past the end");
    set_record(&mut middle, 0, 1, OFFSET, end - 8);
    assert!(face_fits(&middle, 0), "the middle table ends at the end");
    // 長さ 0 のテーブルは、ちょうど終わりの位置までなら置ける。
    let mut empty = bytes.clone();
    set_record(&mut empty, 0, 0, OFFSET, end);
    set_record(&mut empty, 0, 0, LENGTH, 0);
    assert!(face_fits(&empty, 0));
    set_record(&mut empty, 0, 0, OFFSET, end + 1);
    assert!(!face_fits(&empty, 0), "an empty table past the end");
    // 足すとあふれる位置と長さでも panic しない。
    let mut huge = bytes.clone();
    set_record(&mut huge, 0, 0, OFFSET, u32::MAX);
    set_record(&mut huge, 0, 0, LENGTH, u32::MAX);
    assert!(!face_fits(&huge, 0));
}

#[test]
fn a_collection_is_checked_at_the_face_that_is_asked_for() {
    let bytes = collection_of(&[&[4, 4], &[8]]);
    assert!(face_fits(&bytes, 0) && face_fits(&bytes, 1));
    assert!(!face_fits(&bytes, 2), "there are two faces");
    // 2 つ目のフェイスのテーブルが切れていると、2 つ目だけが読めない。
    let cut = &bytes[..bytes.len() - 1];
    assert!(face_fits(cut, 0) && !face_fits(cut, 1));
    // 位置の表がファイルの外を指すと、そのフェイスは読めない。
    let mut outside = bytes.clone();
    let end = bytes.len() as u32;
    outside[16..20].copy_from_slice(&end.to_be_bytes());
    assert!(face_fits(&outside, 0) && !face_fits(&outside, 1));
    // 2 つ目の位置にフェイスの印がなければ読めない。
    let mut misplaced = bytes.clone();
    let second = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    misplaced[16..20].copy_from_slice(&(second + 1).to_be_bytes());
    assert!(!face_fits(&misplaced, 1));
}

#[test]
fn a_collection_cut_inside_its_header_has_no_usable_face() {
    // ヘッダーは 4 つのフェイスがあると言うが、ファイルが位置の表の途中で終わる（途中で切れた TTC）。
    // 読み手は表を丸ごと読むので、収まっている番号の位置も使わない。
    let whole = collection_of(&[&[4], &[4], &[4], &[4]]);
    for length in 12..=12 + 4 * 4 - 1 {
        for index in 0..4 {
            assert!(
                !face_fits(&whole[..length], index),
                "{length} bytes, face {index}"
            );
        }
    }
    // 数が大きすぎて表がファイルに収まらない。
    let mut claims = whole.clone();
    claims[8..12].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(!face_fits(&claims, 0));
    assert!(!face_fits(&claims, u32::MAX - 1));
}

#[test]
fn a_face_whose_table_needed_to_draw_cannot_be_read_is_not_used() {
    let bytes = font(&[4]);
    assert!(face_fits(&bytes, 0), "every needed table is there");
    let end = bytes.len() as u32;
    for tag in NEEDED {
        let name = tag_name(tag);
        let table = table_of(&bytes, 0, tag);
        let changed = |field: usize, value: u32| {
            let mut changed = bytes.clone();
            set_record(&mut changed, 0, table, field, value);
            face_fits(&changed, 0)
        };
        // 一覧に無い。
        assert!(!changed(TAG, UNUSED), "{name} is missing");
        // 読み手は位置が 0 の行を無いものとして扱う（その位置にはフェイスの印がある）。
        assert!(!changed(OFFSET, 0), "{name} at 0");
        // 境界値: 長さ 0 では何も読めず、1 バイトあれば置ける。
        assert!(!changed(LENGTH, 0), "{name} is empty");
        assert!(changed(LENGTH, 1), "{name} of one byte");
        // 境界値: ちょうどファイルの終わりまでなら読め、1 バイトでも越えると読めない。
        let at = end - NEEDED_LENGTH as u32;
        assert!(changed(OFFSET, at), "{name} ends at the end");
        assert!(
            !changed(OFFSET, at + 1),
            "{name} runs one byte past the end"
        );
    }
}

#[test]
fn the_outlines_may_be_truetype_cff_or_cff2_and_truetype_needs_both_its_tables() {
    let bytes = font(&[4]);
    let (glyf, loca) = (table_of(&bytes, 0, b"glyf"), table_of(&bytes, 0, b"loca"));
    // glyf と loca の一覧の行のタグを替えたフェイスを使うか。
    let renamed = |glyf_tag: &[u8; 4], loca_tag: &[u8; 4]| {
        let mut renamed = bytes.clone();
        set_record(&mut renamed, 0, glyf, TAG, u32::from_be_bytes(*glyf_tag));
        set_record(&mut renamed, 0, loca, TAG, u32::from_be_bytes(*loca_tag));
        face_fits(&renamed, 0)
    };
    assert!(renamed(b"glyf", b"loca"), "TrueType outlines");
    // TrueType の輪郭は、字形の位置（loca）と中身（glyf）の両方が要る。
    assert!(!renamed(b"zzzz", b"loca"), "loca without glyf");
    assert!(!renamed(b"glyf", b"zzzz"), "glyf without loca");
    assert!(!renamed(b"zzzz", b"yyyy"), "no outlines at all");
    // CFF と CFF2 は 1 つのテーブルで足りる。
    assert!(renamed(b"CFF ", b"zzzz"), "CFF outlines");
    assert!(renamed(b"CFF2", b"zzzz"), "CFF2 outlines");
    // タグは大文字と小文字、後ろの空白まで一致しなければならない。
    for tag in [b"cff ", b"CFF\0", b"cff2"] {
        assert!(!renamed(tag, b"zzzz"), "{}", tag_name(tag));
    }
    assert!(!renamed(b"Glyf", b"loca"), "Glyf");
}

#[test]
fn in_a_collection_only_the_face_missing_a_needed_table_is_not_used() {
    let bytes = collection_of(&[&[4], &[4], &[4]]);
    let starts: Vec<usize> = (0..3)
        .map(|face| {
            let at = 12 + 4 * face;
            u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]) as usize
        })
        .collect();
    for (broken, &start) in starts.iter().enumerate() {
        let mut missing = bytes.clone();
        let table = table_of(&missing, start, b"cmap");
        set_record(&mut missing, start, table, TAG, UNUSED);
        for face in 0..3 {
            assert_eq!(
                face_fits(&missing, face as u32),
                face != broken,
                "face {face} when face {broken} has no cmap"
            );
        }
    }
}

#[test]
fn the_system_fonts_the_windows_choose_from_pass_the_check() {
    // 本物のフォントを誤って除くと、日本語や太字が出なくなる。手元にあるものだけを確かめる。
    let folder = fonts_folder();
    let mut checked = 0;
    for &(file, index) in REGULAR_FACES
        .iter()
        .chain(&SEMIBOLD_FACES)
        .chain(&JAPANESE_FACES)
        .chain(&JAPANESE_BOLD_FACES)
    {
        if let Ok(bytes) = std::fs::read(folder.join(file)) {
            assert!(face_fits(&bytes, index), "{file} #{index}");
            checked += 1;
        }
    }
    assert!(checked > 0, "no font of the list was found in {folder:?}");
}

#[test]
fn a_cut_off_font_gives_way_to_the_next_one_and_text_still_draws() {
    // 最初の候補は途中で切れた TTC（4 つのフェイスがあると言うが 16 バイトしかない）。
    // そのまま渡すと、epaint は最初のフレームで panic する。
    let sandbox = Sandbox::new();
    let fonts = sandbox.path().join("Fonts");
    std::fs::create_dir_all(&fonts).unwrap();
    let cut = &collection_of(&[&[4], &[4], &[4], &[4]])[..16];
    for (file, _) in REGULAR_FACES
        .iter()
        .take(1)
        .chain(JAPANESE_FACES.iter().take(1))
    {
        std::fs::write(fonts.join(file), cut).unwrap();
    }
    // 次の候補は本物。
    let (next, _) = REGULAR_FACES[1];
    std::fs::copy(fonts_folder().join(next), fonts.join(next)).unwrap();
    let ctx = egui::Context::default();
    assert!(install_fonts_from(&ctx, &fonts));
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(400.0, 200.0),
        )),
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| {
        ui.label("Snipping Lite");
    });
    // 画面に送らないので、テクスチャの変更は捨てる。
    output.textures_delta.clear();
    fn widths(shape: &egui::Shape, out: &mut Vec<f32>) {
        match shape {
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| widths(s, out)),
            egui::Shape::Text(text) => out.push(text.galley.rect.width()),
            _ => {}
        }
    }
    let mut drawn = Vec::new();
    for clipped in &output.shapes {
        widths(&clipped.shape, &mut drawn);
    }
    assert!(
        drawn.iter().any(|&w| w > 0.0),
        "the label is drawn: {drawn:?}"
    );
}
