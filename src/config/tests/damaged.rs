//! pen.ini と markup.ini の壊れた中身のファジング。正しいファイルに、行の単位の傷（知らないキー、
//! 似たキー、前後の空白、全角の数字、巨大な数、BOM、NUL、長い行、改行の違い）と、バイト単位の傷
//! （壊れた UTF-8 を含む）を重ねる。
//!
//! オラクル（期待値は書式から決め、テスト対象のコードは呼ばない。`キー=値` を 1 行に 1 つ、行は
//! `\n` で区切る、キーは完全に一致、値は 10 進の番号）:
//! - 読んでも panic せず、値はいつも選べるもの（色 0 〜 4、太さ 0 〜 2）で、ファイルは変わらない。
//! - キーごとに、そのキーのバイトで始まる最初の行だけで決まる。その行の番号が範囲内ならそれを、
//!   そうでなければ既定（色 0、太さ 1）を読む。ほかの行がどう壊れていても響かない。
//! - UTF-8 として正しくないバイトは、その行だけを読めなくする。その行がキーの最初の行なら、
//!   そのキーは既定（後ろの同じキーの行は見ない）。ファイル全体を捨てはしない。
//! - 書き込みの切り替えは、最初の `markup=` の行がちょうど `markup=1` のときだけオン。
//! - 2 つのファイルは互いに響かず、設定ファイルにも響かない。壊れた後でも、保存し直せば元どおりに読める。

use super::*;
use crate::annotate::COLORS;
use crate::test_support::{
    png::mutate_bytes,
    rng::{self, Reach, Rng},
    Sandbox,
};

/// 壊れたファイルにありそうな行。
fn junk(rng: &mut Rng, key: &str) -> String {
    let number = rng.below(7);
    match rng.below(14) {
        0 => format!("{key} = {number}"),
        1 => format!(" {key}={number}"),
        2 => format!("{key}={number} "),
        3 => format!("{}={number}", key.to_uppercase()),
        4 => format!("{key}=-{number}"),
        5 => format!("{key}=+{number}"),
        6 => format!("{key}=0{number}"),
        7 => format!("{key}=\u{ff11}"),
        8 => format!("{key}=99999999999999999999999"),
        9 => format!("\u{feff}{key}={number}"),
        10 => format!("future_{key}={number}"),
        11 => "x".repeat(rng.range(1..=20_000usize)),
        12 => "\0\0=\0".to_owned(),
        _ => String::new(),
    }
}

/// UTF-8 として正しくないバイトのある行。0xff と 0xfe はどこにあっても正しくなく、0xe3 0x81 は
/// 3 バイトの文字が途中で切れたもの。値の中、キーの中、行の頭に置く。
fn broken(rng: &mut Rng, key: &str) -> Vec<u8> {
    let number = rng.below(7).to_string();
    let bad: &[u8] = rng.pick(&[&b"\xff"[..], b"\xfe\xff", b"\xe3\x81"]);
    let (head, tail) = key.split_at(rng.below(key.len()));
    match rng.below(4) {
        0 => [key.as_bytes(), bad, number.as_bytes()].concat(),
        1 => [key.as_bytes(), number.as_bytes(), bad].concat(),
        2 => [head.as_bytes(), bad, tail.as_bytes(), number.as_bytes()].concat(),
        _ => [bad, key.as_bytes(), number.as_bytes()].concat(),
    }
}

/// 行を並べ、行ごとに CRLF か LF で区切る（最後の改行はあったりなかったり）。
fn file(rng: &mut Rng, mut lines: Vec<Vec<u8>>) -> Vec<u8> {
    rng.shuffle(&mut lines);
    let mut bytes = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        bytes.extend_from_slice(line);
        if index + 1 < lines.len() || rng.chance(0.7) {
            bytes.extend_from_slice(if rng.chance(0.5) { b"\r\n" } else { b"\n" });
        }
    }
    if rng.chance(0.25) {
        let donor = b"color=4\r\nthickness=2\r\nmarkup=1\r\n\xff\xfe";
        mutate_bytes(rng, &mut bytes, donor);
    }
    bytes
}

/// 書式から決めた、キーの最初の行の値。バイトのまま `\n` で区切り、`key` のバイトで始まる最初の
/// 行を探す。行末の `\n` と、その直前の `\r` は値に含めない（改行のない最後の行の `\r` は残る）。
/// そういう行がなければ `None`、その行に UTF-8 として正しくないバイトがあれば `Some(None)`。
pub(super) fn first_value<'a>(bytes: &'a [u8], key: &str) -> Option<Option<&'a str>> {
    let line = bytes
        .split_inclusive(|&b| b == b'\n')
        .find(|line| line.starts_with(key.as_bytes()))?;
    let line = match line.strip_suffix(b"\n") {
        Some(line) => line.strip_suffix(b"\r").unwrap_or(line),
        None => line,
    };
    Some(std::str::from_utf8(&line[key.len()..]).ok())
}

/// 番号の値: 10 進の数で `limit` より小さければそれ、読めないか範囲の外なら `default`。
pub(super) fn index(value: Option<Option<&str>>, limit: usize, default: usize) -> usize {
    value
        .flatten()
        .and_then(|v| v.parse().ok())
        .filter(|&v| v < limit)
        .unwrap_or(default)
}

#[test]
fn damaged_pen_and_markup_files_read_as_written_or_as_the_defaults() {
    let sandbox = Sandbox::new();
    let folder = sandbox.path().join("roaming").join("Snipping Lite");
    fs::create_dir_all(&folder).unwrap();
    // ほかのファイルに響かないことを確かめるための、既定と違う設定。
    let settings = Settings {
        save: true,
        delay: 5,
        theme: AppTheme::Dark,
        ..parse_settings("")
    };
    save_settings(&settings).unwrap();
    let mut reach = Reach::new(&[
        "pen as written",
        "pen default",
        "pen duplicated",
        "not UTF-8",
        "broken first line",
        "broken later line",
        "markup on",
        "markup off",
    ]);
    let cases = rng::check(
        "damaged_pen_and_markup_files_read_as_written_or_as_the_defaults",
        300,
        |rng| {
            let (color, thickness) = (rng.below(COLORS.len() + 2), rng.below(5));
            let mut lines = vec![
                format!("color={color}").into_bytes(),
                format!("thickness={thickness}").into_bytes(),
            ];
            for _ in 0..rng.range(0..=4usize) {
                let key = *rng.pick(&["color=", "thickness=", "markup="]);
                lines.push(junk(rng, key.trim_end_matches('=')).into_bytes());
            }
            if rng.chance(0.4) {
                let key = *rng.pick(&["color=", "thickness="]);
                lines.push(broken(rng, key));
            }
            if rng.chance(0.2) {
                lines.remove(rng.below(lines.len()));
            }
            let pen = file(rng, lines);
            let mut lines = vec![format!("markup={}", rng.below(2)).into_bytes()];
            for _ in 0..rng.range(0..=3usize) {
                lines.push(junk(rng, "markup").into_bytes());
            }
            if rng.chance(0.3) {
                lines.push(broken(rng, "markup="));
            }
            let markup = file(rng, lines);
            rng.note(format!("pen.ini {:?}", String::from_utf8_lossy(&pen)));
            rng.note(format!("markup.ini {:?}", String::from_utf8_lossy(&markup)));
            fs::write(folder.join("pen.ini"), &pen).unwrap();
            fs::write(folder.join("markup.ini"), &markup).unwrap();

            let read = load_pen();
            assert!(read.0 < COLORS.len() && read.1 < 3, "{read:?}");
            let on = load_markup();
            // UTF-8 として正しくても壊れていても、同じオラクルで決まる。
            let (c, t) = (first_value(&pen, "color="), first_value(&pen, "thickness="));
            let expected = (index(c, COLORS.len(), 0), index(t, 3, 1));
            assert_eq!(read, expected, "pen.ini {pen:?}");
            reach.hit(if expected == (color, thickness) {
                "pen as written"
            } else {
                "pen default"
            });
            let lines = |key: &str| {
                pen.split(|&b| b == b'\n')
                    .filter(|l| l.starts_with(key.as_bytes()))
                    .count()
            };
            if lines("color=") > 1 || lines("thickness=") > 1 {
                reach.hit("pen duplicated");
            }
            if std::str::from_utf8(&pen).is_err() {
                reach.hit("not UTF-8");
                reach.hit(if c == Some(None) || t == Some(None) {
                    "broken first line"
                } else {
                    "broken later line"
                });
            }
            // 同じキーが重なっても、そのキーで始まる最初の行だけで決まり、その値がちょうど
            // 1 のときだけオン（それ以外は既定のオフ）。後ろの正しい行は見ない。壊れたバイトの
            // ある最初の行も、値が読めないのでオフ。
            let first = first_value(&markup, "markup=");
            assert_eq!(on, first == Some(Some("1")), "markup.ini {markup:?}");
            reach.hit(if on { "markup on" } else { "markup off" });
            // 読むだけではファイルは変わらず、ほかのファイルにも響かない。
            assert_eq!(fs::read(folder.join("pen.ini")).unwrap(), pen);
            assert_eq!(fs::read(folder.join("markup.ini")).unwrap(), markup);
            assert_eq!(load_settings(), settings);
            // 壊れたファイルの上に保存し直すと、元どおりに読め、作業用のファイルは残らない。
            let (color, thickness) = (rng.below(COLORS.len()), rng.below(3));
            save_pen(color, thickness);
            assert_eq!(load_pen(), (color, thickness));
            assert_eq!(load_markup(), on, "saving the pen leaves the toggle alone");
            save_markup(!on);
            assert_eq!(load_markup(), !on);
            assert_eq!(load_pen(), (color, thickness), "and the toggle the pen");
            assert!(!folder.join("pen.tmp").exists() && !folder.join("markup.tmp").exists());
        },
    );
    reach.finish("damaged_pen_and_markup_files_read_as_written", cases);
}
