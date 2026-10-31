//! 設定ファイルの性質: どんな中身を読んでも、設定ウィンドウが示す選択肢の中の値になり、
//! 書き戻して読むと変わらない。正しい設定は書いて読むと元に戻り、1 行の傷はその行の設定にしか
//! 響かない。ペンと書き込みのファイルも同じように扱う。

use super::*;
use crate::annotate::{COLORS, OUTLINE_WIDTHS};
use crate::test_support::{
    rng::{self, Reach, Rng},
    Sandbox,
};

/// フォルダーの名前に使う文字。Windows の名前に使えない制御文字、改行、`<>:"/|?*` は含めない。
/// 全角の空白（U+3000）と NBSP は、名前の末尾に置いても Windows が残す。
const NAME_CHARS: &[char] = &[
    'a', 'Z', '0', '9', ' ', '.', '-', '_', '=', '#', '(', ')', '録', '画', '写', '真', '　',
    '\u{a0}', 'é', '📷', 'ｱ',
];

/// 壊れたファイルにありそうな値: 境界の数、全角の数字、空白つき、別の型、制御文字、パス。
const ODD_VALUES: &[&str] = &[
    "",
    "0",
    "1",
    "00",
    "01",
    "-1",
    "2",
    "3",
    "4",
    "5",
    "10",
    "30",
    "50",
    "60",
    "200",
    "500",
    " 1",
    "1 ",
    "+1",
    "1.0",
    "１",
    "true",
    "dark",
    "light",
    "system",
    "99999999999999999999",
    "\u{0}",
    "\u{feff}",
    "../x",
    "C:\\x",
    "\u{202e}",
    "録画　",
    "=",
    "==1",
];

/// 保存先として選べるフォルダー（または既定を表す空）。
fn folder(rng: &mut Rng) -> String {
    if rng.chance(0.2) {
        return String::new();
    }
    let mut path = rng
        .pick(&["C:\\", "D:\\", "\\\\server\\share\\"])
        .to_string();
    for part in 0..rng.range(1..=3) {
        if part > 0 {
            path.push('\\');
        }
        let name: String = (0..rng.range(1..=8))
            .map(|_| *rng.pick(NAME_CHARS))
            .collect();
        // Windows は名前の末尾の半角空白とピリオドを落とすので、そう終わるフォルダーはない。
        let name = name.trim_end_matches([' ', '.']);
        path.push_str(if name.is_empty() { "x" } else { name });
    }
    path
}

/// 設定ウィンドウで選べる値だけでできた設定。
fn settings(rng: &mut Rng) -> Settings {
    Settings {
        copy: rng.chance(0.5),
        save: rng.chance(0.5),
        ask: rng.chance(0.5),
        delay: *rng.pick(&DELAYS),
        history: rng.chance(0.5),
        history_limit: *rng.pick(&HISTORY_LIMITS),
        countdown: rng.chance(0.5),
        skip_title: rng.chance(0.5),
        record_cursor: rng.chance(0.5),
        record_fps: *rng.pick(&FRAME_RATES),
        freeze: rng.chance(0.5),
        notify: rng.chance(0.5),
        keep_toolbar: rng.chance(0.5),
        outline: rng.chance(0.5),
        outline_color: rng.below(COLORS.len()),
        outline_width: rng.below(OUTLINE_WIDTHS.len()),
        mic: rng.chance(0.5),
        system_audio: rng.chance(0.5),
        app_audio: rng.chance(0.5),
        copy_video: rng.chance(0.5),
        save_video: rng.chance(0.5),
        theme: *rng.pick(&AppTheme::ALL),
        screenshot_folder: folder(rng),
        video_folder: folder(rng),
    }
}

/// ファイルの各行のキー（`copy=` など）。書き出す順に並ぶ。
fn keys() -> Vec<String> {
    settings_text(&parse_settings(""))
        .lines()
        .map(|line| line[..=line.find('=').unwrap()].to_owned())
        .collect()
}

/// 1 行に入りそうな文字列。改行を含めない（含めると別の行になる）。
fn junk(rng: &mut Rng) -> String {
    const CHARS: &[char] = &[
        'a', 'z', '0', '1', '=', ' ', '\t', '\r', '#', ';', '[', ']', '\u{0}', '\u{feff}', '録',
        '　', '\u{202e}', '\u{fffd}', '📷',
    ];
    (0..rng.range(0..=12)).map(|_| *rng.pick(CHARS)).collect()
}

/// 正しいファイルを 1〜4 か所壊す。
fn mutated(rng: &mut Rng, keys: &[String]) -> String {
    let valid = settings(rng);
    let mut lines: Vec<String> = settings_text(&valid).lines().map(str::to_owned).collect();
    for _ in 0..rng.range(1..=4) {
        let at = rng.below(lines.len().max(1));
        let operator = rng.below(8);
        rng.note(format!("operator {operator} at line {at}"));
        match operator {
            0 if !lines.is_empty() => drop(lines.remove(at)),
            1 if !lines.is_empty() => {
                let copy = lines[at].clone();
                lines.insert(rng.below(lines.len() + 1), copy);
            }
            2 if lines.len() > 1 => {
                let other = rng.below(lines.len());
                lines.swap(at, other);
            }
            3 => lines.insert(at, format!("{}{}", rng.pick(keys), rng.pick(ODD_VALUES))),
            4 if !lines.is_empty() => lines[at] = lines[at].to_uppercase(),
            5 if !lines.is_empty() => {
                let line = &mut lines[at];
                let cut = rng.below(line.chars().count() + 1);
                let index = line.char_indices().nth(cut).map_or(line.len(), |(i, _)| i);
                line.insert_str(index, &junk(rng));
            }
            6 => lines.insert(at, junk(rng)),
            _ => {
                let key = rng.pick(keys).clone();
                if let Some(line) = lines.iter_mut().find(|l| l.starts_with(&key)) {
                    *line = format!("{key}{}", rng.pick(ODD_VALUES));
                }
            }
        }
    }
    let ending = *rng.pick(&["\r\n", "\n", "\r", "\r\r\n"]);
    let mut text = lines.join(ending);
    if rng.chance(0.1) {
        text.insert(0, '\u{feff}');
    }
    if rng.chance(0.2) {
        let cut = rng.below(text.chars().count() + 1);
        text = text.chars().take(cut).collect();
    }
    text
}

/// 有効な設定と、壊れた UTF-8 を lossy で読んだ文字列。
fn broken_utf8(rng: &mut Rng) -> String {
    let mut bytes = settings_text(&settings(rng)).into_bytes();
    for _ in 0..rng.range(1..=6) {
        let at = rng.below(bytes.len() + 1);
        match rng.below(3) {
            0 => bytes.insert(at, *rng.pick(&[0xff, 0xfe, 0x80, 0xc3, 0xe3, 0xed])),
            1 if at < bytes.len() => bytes[at] ^= 1 << rng.below(8),
            _ => {
                let length = rng.below(4);
                bytes.splice(at..at, rng.bytes(length)).for_each(drop);
            }
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

/// 読み込んだ設定が、設定ウィンドウの示す選択肢の中にあり、書き戻して読むと変わらない。
fn assert_offered(parsed: &Settings, input: &str) {
    assert!(DELAYS.contains(&parsed.delay), "{input:?}");
    assert!(HISTORY_LIMITS.contains(&parsed.history_limit), "{input:?}");
    assert!(FRAME_RATES.contains(&parsed.record_fps), "{input:?}");
    assert!(parsed.outline_color < COLORS.len(), "{input:?}");
    assert!(parsed.outline_width < OUTLINE_WIDTHS.len(), "{input:?}");
    for folder in [&parsed.screenshot_folder, &parsed.video_folder] {
        assert!(!folder.contains('\n'), "{folder:?} from {input:?}");
    }
    assert_eq!(
        &parse_settings(&settings_text(parsed)),
        parsed,
        "writing back what was read changes it: {input:?}"
    );
}

#[test]
fn any_text_reads_as_offered_choices_that_survive_a_rewrite() {
    let keys = keys();
    let defaults = parse_settings("");
    let mut reach = Reach::new(&[
        "mutated file",
        "random lines",
        "broken utf-8",
        "kept a chosen value",
        "all defaults",
    ]);
    let cases = rng::check(
        "any_text_reads_as_offered_choices_that_survive_a_rewrite",
        300,
        |rng| {
            let input = match rng.below(3) {
                0 => {
                    reach.hit("mutated file");
                    mutated(rng, &keys)
                }
                1 => {
                    reach.hit("random lines");
                    let lines: Vec<String> = (0..rng.range(0..=30))
                        .map(|_| match rng.below(3) {
                            0 => junk(rng),
                            _ => format!("{}{}", rng.pick(&keys), rng.pick(ODD_VALUES)),
                        })
                        .collect();
                    lines.join(*rng.pick(&["\r\n", "\n"]))
                }
                _ => {
                    reach.hit("broken utf-8");
                    broken_utf8(rng)
                }
            };
            let parsed = parse_settings(&input);
            assert_offered(&parsed, &input);
            reach.hit(if parsed == defaults {
                "all defaults"
            } else {
                "kept a chosen value"
            });
        },
    );
    reach.finish("any_text_reads_as_offered_choices", cases);
}

#[test]
fn every_choice_the_settings_window_offers_survives_writing_and_reading() {
    let mut reach = Reach::new(&[
        "default folder",
        "chosen folder",
        "folder ends in a wide space",
    ]);
    let cases = rng::check(
        "every_choice_the_settings_window_offers_survives_writing_and_reading",
        300,
        |rng| {
            let chosen = settings(rng);
            let text = settings_text(&chosen);
            assert_eq!(parse_settings(&text), chosen, "{text}");
            let folder = &chosen.screenshot_folder;
            reach.hit(if folder.is_empty() {
                "default folder"
            } else if folder.ends_with(['　', '\u{a0}']) {
                "folder ends in a wide space"
            } else {
                "chosen folder"
            });
        },
    );
    reach.finish("every_choice_survives_writing_and_reading", cases);
}

#[test]
fn damaging_one_line_changes_only_its_own_setting() {
    let keys = keys();
    let mut reach = Reach::new(&[
        "line removed",
        "value replaced",
        "key mangled",
        "setting changed",
        "setting kept",
    ]);
    let cases = rng::check(
        "damaging_one_line_changes_only_its_own_setting",
        300,
        |rng| {
            let chosen = settings(rng);
            let written = settings_text(&chosen);
            let mut lines: Vec<String> = written.lines().map(str::to_owned).collect();
            let at = rng.below(lines.len());
            let key = &keys[at];
            match rng.below(3) {
                0 => {
                    reach.hit("line removed");
                    lines.remove(at);
                }
                1 => {
                    reach.hit("value replaced");
                    lines[at] = format!("{key}{}", rng.pick(ODD_VALUES));
                    if rng.chance(0.5) {
                        lines[at].push_str(&junk(rng));
                    }
                }
                _ => {
                    reach.hit("key mangled");
                    let value = lines[at][key.len()..].to_owned();
                    let mangled = match rng.below(4) {
                        0 => key.to_uppercase(),
                        1 => format!(" {key}"),
                        2 => format!("#{key}"),
                        _ => key.replace('=', " ="),
                    };
                    lines[at] = format!("{mangled}{value}");
                }
            }
            let damaged = lines.join("\r\n");
            let parsed = parse_settings(&damaged);
            // 書き出した行どうしを比べる。傷を付けた行のほかは、すべて元の設定のまま。
            let before: Vec<String> = written.lines().map(str::to_owned).collect();
            let after: Vec<String> = settings_text(&parsed).lines().map(str::to_owned).collect();
            for (index, (old, new)) in before.iter().zip(&after).enumerate() {
                if index != at {
                    assert_eq!(old, new, "damaging line {at} changed another: {damaged:?}");
                }
            }
            reach.hit(if before[at] == after[at] {
                "setting kept"
            } else {
                "setting changed"
            });
        },
    );
    reach.finish("damaging_one_line_changes_only_its_own_setting", cases);
}

#[test]
fn a_repeated_key_reads_as_if_only_its_first_line_were_there() {
    let keys = keys();
    let mut reach = Reach::new(&["the order matters", "the order does not matter"]);
    let cases = rng::check(
        "a_repeated_key_reads_as_if_only_its_first_line_were_there",
        300,
        |rng| {
            // 正しい設定 2 つ分の行と、壊れた値の行を混ぜる。どのキーも 2 行以上になる。
            let first = settings(rng);
            let second = if rng.chance(0.25) {
                first.clone()
            } else {
                settings(rng)
            };
            let mut lines: Vec<String> = [settings_text(&first), settings_text(&second)]
                .iter()
                .flat_map(|text| text.lines().map(str::to_owned).collect::<Vec<_>>())
                .collect();
            let odd = if rng.chance(0.25) {
                0
            } else {
                rng.range(1..=10)
            };
            for _ in 0..odd {
                lines.push(format!("{}{}", rng.pick(&keys), rng.pick(ODD_VALUES)));
            }
            if rng.chance(0.3) {
                lines.push(junk(rng));
            }
            rng.shuffle(&mut lines);
            // 各キーの最初の行だけを残す。どのキーでもない行はそのまま。
            let mut seen: Vec<&str> = Vec::new();
            let firsts: Vec<&str> = lines
                .iter()
                .map(String::as_str)
                .filter(
                    |line| match keys.iter().find(|key| line.starts_with(key.as_str())) {
                        Some(key) if seen.contains(&key.as_str()) => false,
                        Some(key) => {
                            seen.push(key);
                            true
                        }
                        None => true,
                    },
                )
                .collect();
            let text = lines.join("\r\n");
            let parsed = parse_settings(&text);
            assert_eq!(parsed, parse_settings(&firsts.join("\r\n")), "{text:?}");
            let reversed: Vec<&str> = lines.iter().rev().map(String::as_str).collect();
            reach.hit(if parse_settings(&reversed.join("\r\n")) == parsed {
                "the order does not matter"
            } else {
                "the order matters"
            });
        },
    );
    reach.finish("a_repeated_key_reads_as_if_only_its_first_line", cases);
}

#[test]
fn pen_and_markup_choices_read_back_and_fall_back_one_by_one() {
    let _sandbox = Sandbox::new();
    let mut reach = Reach::new(&["both kept", "color fell back", "thickness fell back"]);
    let cases = rng::check(
        "pen_and_markup_choices_read_back_and_fall_back_one_by_one",
        200,
        |rng| {
            // 範囲の外の番号も少し混ぜる。番号はそれぞれ別に、範囲の外なら既定（色 0、太さ 1）になる。
            let (color, thickness) = (rng.below(COLORS.len() + 2), rng.below(3 + 2));
            save_pen(color, thickness);
            let expected = (
                if color < COLORS.len() { color } else { 0 },
                if thickness < 3 { thickness } else { 1 },
            );
            assert_eq!(load_pen(), expected, "saved {color},{thickness}");
            reach.hit(match (color < COLORS.len(), thickness < 3) {
                (true, true) => "both kept",
                (false, _) => "color fell back",
                (true, false) => "thickness fell back",
            });
            let on = rng.chance(0.5);
            save_markup(on);
            assert_eq!(load_markup(), on);
        },
    );
    reach.finish("pen_and_markup_choices_read_back", cases);
}

#[test]
fn damaged_pen_and_markup_files_read_as_offered_choices() {
    let sandbox = Sandbox::new();
    let folder = sandbox.path().join("roaming").join("Snipping Lite");
    fs::create_dir_all(&folder).unwrap();
    let mut reach = Reach::new(&["pen kept", "pen fell back", "markup on", "markup off"]);
    let cases = rng::check(
        "damaged_pen_and_markup_files_read_as_offered_choices",
        200,
        |rng| {
            let (color, thickness) = (rng.below(COLORS.len()), rng.below(3));
            let valid = format!("color={color}\r\nthickness={thickness}\r\n");
            let mut bytes = valid.clone().into_bytes();
            for _ in 0..rng.range(0..=3) {
                let at = rng.below(bytes.len() + 1);
                match rng.below(3) {
                    0 => bytes.insert(at, *rng.pick(&[0xff, 0x80, b'\n', b'=', b'9', 0])),
                    1 if at < bytes.len() => drop(bytes.remove(at)),
                    _ => {
                        let length = rng.below(3);
                        bytes.splice(at..at, rng.bytes(length)).for_each(drop);
                    }
                }
            }
            fs::write(folder.join("pen.ini"), &bytes).unwrap();
            let (read_color, read_thickness) = load_pen();
            assert!(read_color < COLORS.len() && read_thickness < 3, "{bytes:?}");
            if bytes == valid.as_bytes() {
                assert_eq!((read_color, read_thickness), (color, thickness));
            }
            reach.hit(if (read_color, read_thickness) == (color, thickness) {
                "pen kept"
            } else {
                "pen fell back"
            });
            // 書き込みの切り替えは、壊れたファイルからはオンにならない（オンの行がそのまま残るときだけ）。
            let mut markup = format!("markup={}\r\n", rng.below(2)).into_bytes();
            if rng.chance(0.7) {
                let at = rng.below(markup.len() + 1);
                let length = rng.range(1..=3);
                markup.splice(at..at, rng.bytes(length)).for_each(drop);
            }
            fs::write(folder.join("markup.ini"), &markup).unwrap();
            let on = load_markup();
            if on {
                let text = String::from_utf8_lossy(&markup);
                assert!(text.lines().any(|line| line == "markup=1"), "{markup:?}");
            }
            reach.hit(if on { "markup on" } else { "markup off" });
        },
    );
    reach.finish("damaged_pen_and_markup_files", cases);
}
