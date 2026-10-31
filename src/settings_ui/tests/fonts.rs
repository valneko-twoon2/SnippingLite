//! フォントファイルのフェイスの数（`face_count`）。TrueType Collection（.ttc）は先頭 12 バイトの
//! ヘッダーに `ttcf` のタグと、ビッグエンディアンの 32 ビットのフォントの数（8〜11 バイト目）を持つ
//! （OpenType の仕様の TTC Header）。それ以外のフォントファイルは 1 つのフェイスだけを持つ。
//! 数が 0 のファイルは、どの番号のフェイスも読めないので使わない。
//!
//! | 項目 | 技法 | ケース → 期待値 |
//! |---|---|---|
//! | 長さ | 境界値（バイト） | 0、11 バイト → 0; ちょうど 12 バイト → 読める; 後ろに続くデータ → ヘッダーだけで決まる |
//! | タグ | 同値分割 | `ttcf` → ヘッダーの数; TrueType、`OTTO`、大文字違い、先頭以外の `ttcf` → 1 |
//! | 数 | 境界値 | 0、1、2、u32::MAX; バイトの順（256、2^24） |
//! | 壊れた入力 | 悪意ある入力 | 0〜16 バイトの切れ端、0x00 と 0xff だけの列 → panic しない |

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
