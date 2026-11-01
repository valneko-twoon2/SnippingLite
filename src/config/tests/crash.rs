//! クラッシュ時の整合性: 設定、ペン、マークアップのファイルの保存を、段階ごとに子プロセスごと殺す。
//!
//! どのファイルも `write_atomic` が一時ファイル（`settings.ini.<プロセス ID>.tmp` など）に書いてから
//! 名前を変える。一時ファイルを書き終えて名前を変える前に殺すと、
//!
//! - 最後の名前のファイルは、前の内容のまま完全に残る（書きかけが読まれない）。
//! - 一時ファイルは新しい中身を書き終えて残るが、次の起動はそれを使わずに前の値を読む
//!   （`config::temporary` の決め事）。それより前に保存したファイルは新しい値、後のファイルは前の値。
//! - 残った一時ファイルは、次にそのファイルを読むとき（次の起動）に消える。
//!
//! 書いている途中のプロセスがまだ動いているとき（もう 1 つの Snipping Lite のプロセス）は、
//! 読んでも保存しても、その一時ファイルに触れない。

use super::*;
use crate::test_support::{crash, Sandbox};

pub(super) fn folder() -> PathBuf {
    PathBuf::from(std::env::var_os("APPDATA").unwrap()).join("Snipping Lite")
}

pub(super) fn names() -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(folder())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

pub(super) const FILES: [&str; 3] = ["settings.ini", "pen.ini", "markup.ini"];

pub(super) fn before() -> Settings {
    Settings {
        delay: 3,
        theme: AppTheme::Dark,
        screenshot_folder: "D:\\前".to_owned(),
        ..parse_settings("")
    }
}

fn after() -> Settings {
    Settings {
        save: true,
        delay: 10,
        history_limit: 50,
        theme: AppTheme::Light,
        screenshot_folder: "E:\\後の フォルダー".to_owned(),
        ..parse_settings("")
    }
}

/// 子が書く新しい中身。`FILES` の順。ペンとマークアップは、最初の版からの書式（`compat.rs` の
/// `OLD_PEN`、`OLD_MARKUP` と同じ `キー=値` と CRLF の行）。
fn written() -> [String; 3] {
    [
        settings_text(&after()),
        "color=4\r\nthickness=2\r\n".to_owned(),
        "markup=1\r\n".to_owned(),
    ]
}

pub(super) fn prepare(_: &Sandbox) {
    save_settings(&before()).unwrap();
    save_pen(1, 0);
    save_markup(false);
}

#[test]
#[ignore = "Runs only as the child process of a crash test"]
fn child_saves_the_settings_the_pen_and_the_markup() {
    if !crash::is_child() {
        return;
    }
    save_settings(&after()).unwrap();
    save_pen(4, 2);
    save_markup(true);
}

#[test]
fn a_save_killed_part_way_leaves_every_file_whole_and_the_next_read_tidies_up() {
    let child = crash::test_name(
        module_path!(),
        "child_saves_the_settings_the_pen_and_the_markup",
    );
    let check = |_: &Sandbox, crashed: &crash::Crash| {
        // 何番目のファイルの一時ファイルを書いたところで止まったか。最後まで終えたら 3。
        let stopped = crashed.stopped.as_deref().map_or(FILES.len(), |label| {
            FILES
                .iter()
                .position(|file| label == format!("temporary written: {file}"))
                .unwrap_or_else(|| panic!("an unknown step {label}"))
        });
        let mut left = FILES.map(str::to_owned).to_vec();
        if let Some(file) = FILES.get(stopped) {
            // 名前を変える直前で止まったので、一時ファイルは子の ID の名前で、新しい中身を書き終えている。
            let temporary = format!("{file}.{}.tmp", crashed.id());
            assert_eq!(
                fs::read_to_string(folder().join(&temporary)).unwrap(),
                written()[stopped],
                "{temporary}"
            );
            left.push(temporary);
        }
        left.sort();
        assert_eq!(names(), left, "stopped at {stopped}");
        let saved = |index: usize| index < stopped;
        let expected_settings = if saved(0) { after() } else { before() };
        assert_eq!(load_settings(), expected_settings, "stopped at {stopped}");
        assert_eq!(
            fs::read_to_string(folder().join("settings.ini")).unwrap(),
            settings_text(&expected_settings),
            "the settings file is whole"
        );
        assert_eq!(load_pen(), if saved(1) { (4, 2) } else { (1, 0) });
        assert_eq!(load_markup(), saved(2));
        // 読んだので、止まったプロセスの一時ファイルはもうない。
        assert_eq!(names(), ["markup.ini", "pen.ini", "settings.ini"]);
        // 次の保存も何も残さない。
        save_settings(&after()).unwrap();
        save_pen(4, 2);
        save_markup(true);
        assert_eq!(load_settings(), after());
        assert_eq!((load_pen(), load_markup()), ((4, 2), true));
        assert_eq!(names(), ["markup.ini", "pen.ini", "settings.ini"]);
    };
    let stages = crash::sweep(&child, 10, prepare, check);
    assert_eq!(
        stages,
        FILES.map(|file| format!("temporary written: {file}")),
        "the child stopped after writing each temporary file"
    );
}

#[test]
fn a_temporary_file_another_running_process_is_writing_is_left_alone() {
    let child = crash::test_name(
        module_path!(),
        "child_saves_the_settings_the_pen_and_the_markup",
    );
    let sandbox = Sandbox::new();
    prepare(&sandbox);
    // 子は設定の一時ファイルを書き終え、名前を変える前で、動いたまま待っている。
    let mut writer = crash::pause(&sandbox, &child, 1);
    assert_eq!(
        writer.stopped.as_deref(),
        Some("temporary written: settings.ini")
    );
    let temporary = folder().join(format!("settings.ini.{}.tmp", writer.id()));
    let untouched = || {
        assert_eq!(
            fs::read_to_string(&temporary).unwrap(),
            written()[0],
            "the file the other process is writing"
        );
    };
    // このプロセスが読んでも、同じファイルを保存しても、もう一方の一時ファイルはそのまま。
    assert_eq!(load_settings(), before());
    untouched();
    let mine = Settings {
        notify: false,
        ..before()
    };
    save_settings(&mine).unwrap();
    assert_eq!(load_settings(), mine);
    untouched();
    // もう一方が止まったら、次に読むときに消える。中身は使わない。
    writer.kill();
    assert_eq!(load_settings(), mine);
    assert!(!temporary.exists());
    assert_eq!(names(), ["markup.ini", "pen.ini", "settings.ini"]);
}
