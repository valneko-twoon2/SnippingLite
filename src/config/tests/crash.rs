//! クラッシュ時の整合性: 設定、ペン、マークアップのファイルの保存を、段階ごとに子プロセスごと殺す。
//!
//! どのファイルも `write_atomic` が一時ファイル（`settings.tmp` など）に書いてから名前を変える。
//! 一時ファイルを書き終えて名前を変える前に殺すと、
//!
//! - 最後の名前のファイルは、前の内容のまま完全に残る（書きかけが読まれない）。
//! - 次の起動は前の値を読む。それより前に保存したファイルは新しい値、後のファイルは前の値。
//! - 残った一時ファイルは、同じファイルを次に保存したときに上書きされて消える。

use super::*;
use crate::test_support::{crash, Sandbox};

fn folder() -> PathBuf {
    PathBuf::from(std::env::var_os("APPDATA").unwrap()).join("Snipping Lite")
}

fn before() -> Settings {
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
fn a_save_killed_part_way_leaves_every_file_whole_and_the_next_save_tidies_up() {
    let child = crash::test_name(
        module_path!(),
        "child_saves_the_settings_the_pen_and_the_markup",
    );
    let prepare = |_: &Sandbox| {
        save_settings(&before()).unwrap();
        save_pen(1, 0);
        save_markup(false);
    };
    let files = ["settings.ini", "pen.ini", "markup.ini"];
    let check = |_: &Sandbox, crashed: &crash::Crash| {
        // 何番目のファイルの一時ファイルを書いたところで止まったか。最後まで終えたら 3。
        let stopped = crashed.stopped.as_deref().map_or(files.len(), |label| {
            files
                .iter()
                .position(|file| label == format!("temporary written: {file}"))
                .unwrap_or_else(|| panic!("an unknown step {label}"))
        });
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
        for (index, file) in files.iter().enumerate() {
            let temporary = folder().join(file).with_extension("tmp");
            assert_eq!(temporary.exists(), index == stopped, "{file}");
        }
        // 次の保存は、残った一時ファイルを上書きしてから名前を変えるので、何も残らない。
        save_settings(&after()).unwrap();
        save_pen(4, 2);
        save_markup(true);
        assert_eq!(load_settings(), after());
        assert_eq!((load_pen(), load_markup()), ((4, 2), true));
        let mut names: Vec<String> = fs::read_dir(folder())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, ["markup.ini", "pen.ini", "settings.ini"]);
    };
    let stages = crash::sweep(&child, 10, prepare, check);
    assert_eq!(
        stages,
        files.map(|file| format!("temporary written: {file}")),
        "the child stopped after writing each temporary file"
    );
}
