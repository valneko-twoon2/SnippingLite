//! UTF-8 として正しくないバイトがある設定ファイル。壊れたバイトのある行だけが読めなくなり、
//! ほかの行の設定は生きる。期待値は規則から手で決めた（`config::readable_lines` のコメント）。
//! - 壊れた行が、あるキーで始まる（最初の `=` までが読めて、そのキーと一致する）なら、その行は
//!   そのキーの行として数え、値は読めないので既定値。重なったキーは最初の行で決まる規則のまま。
//! - 最初の `=` までに壊れたバイトがある行は、どのキーの行でもなく、捨てる。

use super::*;
use crate::annotate::{COLORS, OUTLINE_WIDTHS};
use crate::test_support::{
    rng::{self, Reach},
    Sandbox,
};

fn folder(sandbox: &Sandbox) -> std::path::PathBuf {
    let folder = sandbox.path().join("roaming").join("Snipping Lite");
    fs::create_dir_all(&folder).unwrap();
    folder
}

#[test]
fn a_broken_line_in_pen_ini_keeps_the_other_lines() {
    let sandbox = Sandbox::new();
    let path = folder(&sandbox).join("pen.ini");
    for (bytes, expected, why) in [
        (
            &b"color=3\r\n\xff\xfe junk\r\nthickness=2\r\n"[..],
            (3, 2),
            "どのキーでもない壊れた行は、色にも太さにも響かない",
        ),
        (
            b"color=\xff4\r\ncolor=2\r\nthickness=0\r\n",
            (0, 0),
            "色の最初の行が壊れていれば色は既定（後ろの color=2 は見ない）、太さは生きる",
        ),
        (
            b"co\xfflor=4\r\ncolor=2\r\nthickness=0\n",
            (2, 0),
            "キーの部分が壊れた行は色の行ではないので、次の color=2 が最初の色の行になる",
        ),
        (
            b"thickness=2\r\ncolor=1\r\n\xc3",
            (1, 2),
            "改行のない最後の行が切れた文字で終わっていても、前の行は生きる",
        ),
    ] {
        fs::write(&path, bytes).unwrap();
        assert_eq!(load_pen(), expected, "{why}: {bytes:?}");
        assert_eq!(
            fs::read(&path).unwrap(),
            bytes,
            "読むだけではファイルは変わらない"
        );
    }
}

#[test]
fn a_broken_line_in_markup_ini_keeps_the_other_lines() {
    let sandbox = Sandbox::new();
    let path = folder(&sandbox).join("markup.ini");
    for (bytes, on) in [
        (&b"\xff\r\nmarkup=1\r\n"[..], true),
        (b"markup=1\r\n\x80\x80\r\n", true),
        // 最初の markup= の行が壊れていれば、既定のオフ。後ろの markup=1 は見ない。
        (b"markup=\xff1\r\nmarkup=1\r\n", false),
    ] {
        fs::write(&path, bytes).unwrap();
        assert_eq!(load_markup(), on, "{bytes:?}");
    }
}

#[test]
fn a_broken_line_in_settings_ini_resets_only_its_own_setting() {
    let sandbox = Sandbox::new();
    let path = folder(&sandbox).join("settings.ini");
    // delay の値の途中で 3 バイトの文字が切れ、スクリーンショットの保存先に 0xff がある。
    // 最後の行は改行なしで壊れたバイトを含み、どのキーでもない。
    let bytes = b"save=1\r\ndelay=\xe3\x815\r\ntheme=dark\r\nscreenshot_folder=D:\\\xff\r\n\
                  video_folder=E:\\Clips\r\nhistory_limit=50\r\nmic=1\r\n\xfemic=0";
    fs::write(&path, bytes).unwrap();
    assert_eq!(
        load_settings(),
        Settings {
            save: true,
            theme: AppTheme::Dark,
            video_folder: "E:\\Clips".into(),
            history_limit: 50,
            mic: true,
            ..parse_settings("")
        }
    );
}

#[test]
fn broken_bytes_in_settings_ini_reset_exactly_the_lines_they_land_in() {
    let sandbox = Sandbox::new();
    let path = folder(&sandbox).join("settings.ini");
    let defaults: Vec<String> = settings_text(&parse_settings(""))
        .lines()
        .map(str::to_owned)
        .collect();
    let mut reach = Reach::new(&["key broken", "value broken", "setting kept"]);
    let cases = rng::check(
        "broken_bytes_in_settings_ini_reset_exactly_the_lines_they_land_in",
        200,
        |rng| {
            // 既定と違いそうな値を選ぶ（範囲の中だけ）。
            let chosen = Settings {
                copy: rng.chance(0.5),
                save: rng.chance(0.5),
                delay: *rng.pick(&DELAYS),
                history_limit: *rng.pick(&HISTORY_LIMITS),
                outline_color: rng.below(COLORS.len()),
                outline_width: rng.below(OUTLINE_WIDTHS.len()),
                theme: *rng.pick(&AppTheme::ALL),
                screenshot_folder: rng.pick(&["", "D:\\録画", "C:\\Shots"]).to_string(),
                video_folder: rng.pick(&["", "E:\\Clips"]).to_string(),
                ..parse_settings("")
            };
            let written: Vec<String> = settings_text(&chosen).lines().map(str::to_owned).collect();
            // 0xff と 0xfe は UTF-8 のどこにも現れないので、入れた行は必ず壊れる。
            let damaged: Vec<bool> = written.iter().map(|_| rng.chance(0.2)).collect();
            let mut bytes = Vec::new();
            for (line, &broken) in written.iter().zip(&damaged) {
                let mut line = line.clone().into_bytes();
                if broken {
                    let at = rng.below(line.len() + 1);
                    let key = line.iter().position(|&b| b == b'=').unwrap();
                    reach.hit(if at <= key {
                        "key broken"
                    } else {
                        "value broken"
                    });
                    line.insert(at, *rng.pick(&[0xff, 0xfe]));
                }
                bytes.extend_from_slice(&line);
                bytes.extend_from_slice(b"\r\n");
            }
            fs::write(&path, &bytes).unwrap();
            let read: Vec<String> = settings_text(&load_settings())
                .lines()
                .map(str::to_owned)
                .collect();
            // 各キーは 1 行だけなので、壊れた行の設定は既定、壊れていない行の設定はそのまま。
            for (index, broken) in damaged.iter().enumerate() {
                let expected = if *broken {
                    &defaults[index]
                } else {
                    reach.hit("setting kept");
                    &written[index]
                };
                assert_eq!(&read[index], expected, "line {index} of {bytes:?}");
            }
        },
    );
    reach.finish("broken_bytes_in_settings_ini", cases);
}
