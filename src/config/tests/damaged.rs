//! pen.ini と markup.ini の壊れた中身のファジング。正しいファイルに、行の単位の傷（知らないキー、
//! 似たキー、前後の空白、全角の数字、巨大な数、BOM、NUL、長い行、改行の違い）と、バイト単位の傷
//! （壊れた UTF-8 を含む）を重ねる。
//!
//! オラクル（期待値は書式から決める。`キー=値` を 1 行に 1 つ、キーは完全に一致、値は 10 進の番号）:
//! - 読んでも panic せず、値はいつも選べるもの（色 0 〜 4、太さ 0 〜 2）で、ファイルは変わらない。
//! - ファイルが UTF-8 として読め、キーの行が 1 つだけなら、その行の番号が範囲内ならそれを、
//!   そうでなければ既定（色 0、太さ 1）を読む。ほかの行がどう壊れていても響かない。キーの行が
//!   2 つ以上あるときの扱いは別に決める（重複キー）ので、ここでは範囲だけを確かめる。
//! - 書き込みの切り替えは、`markup=0` か `markup=1` の行が 1 つだけならその値、なければオフ。
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

/// 行を並べ、行ごとに CRLF か LF で区切る（最後の改行はあったりなかったり）。
fn file(rng: &mut Rng, mut lines: Vec<String>) -> Vec<u8> {
    rng.shuffle(&mut lines);
    let mut text = String::new();
    for (index, line) in lines.iter().enumerate() {
        text.push_str(line);
        if index + 1 < lines.len() || rng.chance(0.7) {
            text.push_str(if rng.chance(0.5) { "\r\n" } else { "\n" });
        }
    }
    let mut bytes = text.into_bytes();
    if rng.chance(0.25) {
        let donor = b"color=4\r\nthickness=2\r\nmarkup=1\r\n\xff\xfe";
        mutate_bytes(rng, &mut bytes, donor);
    }
    bytes
}

/// 書式から決めた期待値: キーの行（`key=` で始まる行）が 1 つだけならその値、なければ `None`。
/// 2 つ以上なら `Err`（重複キーの扱いは別に決める）。
fn single<'a>(text: &'a str, key: &str) -> Result<Option<&'a str>, ()> {
    let mut found = text.lines().filter_map(|line| line.strip_prefix(key));
    match (found.next(), found.next()) {
        (value, None) => Ok(value),
        _ => Err(()),
    }
}

fn index(value: Option<&str>, limit: usize, default: usize) -> usize {
    value
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
        "markup on",
        "markup off",
    ]);
    let cases = rng::check(
        "damaged_pen_and_markup_files_read_as_written_or_as_the_defaults",
        300,
        |rng| {
            let (color, thickness) = (rng.below(COLORS.len() + 2), rng.below(5));
            let mut lines = vec![format!("color={color}"), format!("thickness={thickness}")];
            for _ in 0..rng.range(0..=4usize) {
                let key = *rng.pick(&["color=", "thickness=", "markup="]);
                lines.push(junk(rng, key.trim_end_matches('=')));
            }
            if rng.chance(0.2) {
                lines.remove(rng.below(lines.len()));
            }
            let pen = file(rng, lines);
            let mut lines = vec![format!("markup={}", rng.below(2))];
            for _ in 0..rng.range(0..=3usize) {
                lines.push(junk(rng, "markup"));
            }
            let markup = file(rng, lines);
            rng.note(format!("pen.ini {:?}", String::from_utf8_lossy(&pen)));
            rng.note(format!("markup.ini {:?}", String::from_utf8_lossy(&markup)));
            fs::write(folder.join("pen.ini"), &pen).unwrap();
            fs::write(folder.join("markup.ini"), &markup).unwrap();

            let read = load_pen();
            assert!(read.0 < COLORS.len() && read.1 < 3, "{read:?}");
            let on = load_markup();
            match std::str::from_utf8(&pen) {
                Err(_) => reach.hit("not UTF-8"),
                Ok(text) => match (single(text, "color="), single(text, "thickness=")) {
                    (Ok(c), Ok(t)) => {
                        let expected = (index(c, COLORS.len(), 0), index(t, 3, 1));
                        assert_eq!(read, expected, "pen.ini {text:?}");
                        reach.hit(if expected == (color, thickness) {
                            "pen as written"
                        } else {
                            "pen default"
                        });
                    }
                    _ => reach.hit("pen duplicated"),
                },
            }
            if let Ok(text) = std::str::from_utf8(&markup) {
                let mut values = text
                    .lines()
                    .filter(|line| *line == "markup=0" || *line == "markup=1");
                if let (first, None) = (values.next(), values.next()) {
                    assert_eq!(on, first == Some("markup=1"), "markup.ini {text:?}");
                }
            }
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
