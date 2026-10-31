use super::*;

#[test]
fn settings_fall_back_to_defaults() {
    assert_eq!(
        parse_settings(""),
        Settings {
            copy: true,
            save: false,
            ask: false,
            delay: 0,
            history: true,
            history_limit: 200,
            countdown: true,
            skip_title: true,
            record_cursor: true,
            record_fps: 60,
            freeze: false,
            notify: true,
            keep_toolbar: false,
            outline: false,
            outline_color: 3,
            outline_width: 1,
            mic: false,
            system_audio: true,
            app_audio: true,
            copy_video: true,
            save_video: true,
            theme: AppTheme::System,
            screenshot_folder: String::new(),
            video_folder: String::new()
        }
    );
    let parsed = parse_settings(
        "copy=0\r\nsave=1\r\nask=1\r\ndelay=5\r\nhistory=0\r\nhistory_limit=50\r\ncountdown=0\r\nskip_title=0\r\nrecord_cursor=0\r\nrecord_fps=30\r\nfreeze=1\r\nnotify=0\r\nkeep_toolbar=1\r\noutline=1\r\noutline_color=4\r\noutline_width=2\r\nmic=1\r\nsystem_audio=0\r\napp_audio=0\r\ncopy_video=0\r\nsave_video=0\r\ntheme=dark\r\nscreenshot_folder=D:\\Shots\r\n",
    );
    assert_eq!(
        parsed,
        Settings {
            copy: false,
            save: true,
            ask: true,
            delay: 5,
            history: false,
            history_limit: 50,
            countdown: false,
            skip_title: false,
            record_cursor: false,
            record_fps: 30,
            freeze: true,
            notify: false,
            keep_toolbar: true,
            outline: true,
            outline_color: 4,
            outline_width: 2,
            mic: true,
            system_audio: false,
            app_audio: false,
            copy_video: false,
            save_video: false,
            theme: AppTheme::Dark,
            screenshot_folder: "D:\\Shots".to_owned(),
            video_folder: String::new()
        }
    );
    assert_eq!(parse_settings("delay=7").delay, 0);
    for invalid in ["0", "-1", "120", "invalid"] {
        assert_eq!(
            parse_settings(&format!("record_fps={invalid}")).record_fps,
            60
        );
    }
    assert_eq!(parse_settings("history_limit=7").history_limit, 200);
}

#[test]
fn keep_toolbar_is_on_only_for_the_exact_line() {
    for (text, expected) in [
        ("keep_toolbar=1", true),
        ("notify=0\r\nkeep_toolbar=1\r\noutline=1\r\noutline_color=4\r\noutline_width=2\r\nmic=1\r\n", true),
        ("keep_toolbar=0", false),
        ("", false),
        ("keep_toolbar=", false),
        ("keep_toolbar=true", false),
        ("keep_toolbar=2", false),
        ("keep_toolbar=11", false),
        (" keep_toolbar=1", false),
        ("keep_toolbar=1 ", false),
        ("KEEP_TOOLBAR=1", false),
        ("# keep_toolbar=1", false),
    ] {
        assert_eq!(parse_settings(text).keep_toolbar, expected, "{text:?}");
    }
}

#[test]
fn outline_choices_fall_back_when_out_of_range() {
    let colors = crate::annotate::COLORS.len();
    let widths = crate::annotate::OUTLINE_WIDTHS.len();
    // Boundary: the last valid index is kept, the next one is not.
    assert_eq!(
        parse_settings(&format!("outline_color={}", colors - 1)).outline_color,
        colors - 1
    );
    assert_eq!(
        parse_settings(&format!("outline_width={}", widths - 1)).outline_width,
        widths - 1
    );
    for text in [
        format!("outline_color={colors}"),
        "outline_color=-1".to_owned(),
        "outline_color=".to_owned(),
        "outline_color=red".to_owned(),
        "outline_color=99999999999999999999".to_owned(),
    ] {
        assert_eq!(parse_settings(&text).outline_color, 3, "{text}");
    }
    for text in [
        format!("outline_width={widths}"),
        "outline_width=-1".to_owned(),
        "outline_width=x".to_owned(),
    ] {
        assert_eq!(parse_settings(&text).outline_width, 1, "{text}");
    }
    assert!(!parse_settings("outline=0").outline);
    assert!(!parse_settings("outline=true").outline);
    assert!(parse_settings("outline=1").outline);
}

#[test]
fn saved_settings_read_back_unchanged() {
    let base = parse_settings("");
    let mut variants = vec![base.clone()];
    // Flip every switch on its own so a key mismatch in one of them shows up.
    for flip in [
        |s: &mut Settings| s.copy ^= true,
        |s: &mut Settings| s.save ^= true,
        |s: &mut Settings| s.ask ^= true,
        |s: &mut Settings| s.history ^= true,
        |s: &mut Settings| s.countdown ^= true,
        |s: &mut Settings| s.skip_title ^= true,
        |s: &mut Settings| s.record_cursor ^= true,
        |s: &mut Settings| s.freeze ^= true,
        |s: &mut Settings| s.notify ^= true,
        |s: &mut Settings| s.keep_toolbar ^= true,
        |s: &mut Settings| s.outline ^= true,
        |s: &mut Settings| s.mic ^= true,
        |s: &mut Settings| s.system_audio ^= true,
        |s: &mut Settings| s.app_audio ^= true,
        |s: &mut Settings| s.copy_video ^= true,
        |s: &mut Settings| s.save_video ^= true,
    ] {
        let mut settings = base.clone();
        flip(&mut settings);
        assert_ne!(settings, base);
        variants.push(settings);
    }
    for delay in DELAYS {
        variants.push(Settings {
            delay,
            ..base.clone()
        });
    }
    for history_limit in HISTORY_LIMITS {
        variants.push(Settings {
            history_limit,
            ..base.clone()
        });
    }
    for record_fps in FRAME_RATES {
        variants.push(Settings {
            record_fps,
            ..base.clone()
        });
    }
    for outline_color in 0..crate::annotate::COLORS.len() {
        variants.push(Settings {
            outline_color,
            ..base.clone()
        });
    }
    for outline_width in 0..crate::annotate::OUTLINE_WIDTHS.len() {
        variants.push(Settings {
            outline_width,
            ..base.clone()
        });
    }
    for theme in AppTheme::ALL {
        variants.push(Settings {
            theme,
            ..base.clone()
        });
    }
    variants.push(Settings {
        screenshot_folder: "D:\\Shots".to_owned(),
        video_folder: "E:\\録画".to_owned(),
        ..base.clone()
    });
    for settings in variants {
        assert_eq!(parse_settings(&settings_text(&settings)), settings);
    }
}

#[test]
fn atomic_write_replaces_the_file_and_leaves_no_temporary() {
    let folder = std::env::temp_dir().join(format!("snipping-lite-config-{}", std::process::id()));
    fs::create_dir_all(&folder).unwrap();
    let path = folder.join("settings.ini");
    write_atomic(&path, "a".to_owned()).unwrap();
    write_atomic(&path, "b".to_owned()).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "b");
    assert!(!path.with_extension("tmp").exists());
    fs::remove_dir_all(&folder).unwrap();
}

#[test]
fn atomic_write_fails_when_the_folder_is_missing() {
    let folder = std::env::temp_dir()
        .join("snipping-lite-no-such-folder")
        .join("a");
    assert!(write_atomic(&folder.join("settings.ini"), "a".to_owned()).is_err());
}

#[test]
fn themes_have_a_label_and_survive_a_round_trip() {
    for theme in AppTheme::ALL {
        assert!(!theme.label().is_empty());
        assert_eq!(AppTheme::parse(theme.key()), theme);
    }
    let labels: Vec<_> = AppTheme::ALL.iter().map(|t| t.label()).collect();
    assert_eq!(labels.len(), 3);
    assert!(labels[0] != labels[1] && labels[1] != labels[2] && labels[0] != labels[2]);
    // Equivalence: anything that is not a known key means "follow Windows".
    for unknown in ["", "Dark", "LIGHT", " dark", "blue"] {
        assert_eq!(AppTheme::parse(unknown), AppTheme::System, "{unknown:?}");
    }
}

#[test]
fn the_settings_file_has_a_stable_layout() {
    // Golden: the file other versions and the settings window read.
    let settings = Settings {
        screenshot_folder: "D:\\Shots".to_owned(),
        ..parse_settings("")
    };
    assert_eq!(
        settings_text(&settings),
        "copy=1\r\nsave=0\r\nask=0\r\ndelay=0\r\nhistory=1\r\nhistory_limit=200\r\n\
         countdown=1\r\nskip_title=1\r\nrecord_cursor=1\r\nrecord_fps=60\r\nfreeze=0\r\n\
         notify=1\r\nkeep_toolbar=0\r\noutline=0\r\noutline_color=3\r\noutline_width=1\r\n\
         mic=0\r\nsystem_audio=1\r\napp_audio=1\r\ncopy_video=1\r\nsave_video=1\r\n\
         theme=system\r\n\
         screenshot_folder=D:\\Shots\r\nvideo_folder=\r\n"
    );
}

#[test]
fn list_values_accept_exactly_the_offered_choices() {
    // Decision table: a value is kept only when it is one of the offered choices.
    for delay in DELAYS {
        assert_eq!(parse_settings(&format!("delay={delay}")).delay, delay);
    }
    for delay in [1, 2, 4, 9, 11] {
        assert_eq!(parse_settings(&format!("delay={delay}")).delay, 0);
    }
    for limit in HISTORY_LIMITS {
        let text = format!("history_limit={limit}");
        assert_eq!(parse_settings(&text).history_limit, limit);
    }
    for limit in [0, 49, 51, 499, 501] {
        let text = format!("history_limit={limit}");
        assert_eq!(parse_settings(&text).history_limit, HISTORY_LIMITS[2]);
    }
    for fps in FRAME_RATES {
        assert_eq!(parse_settings(&format!("record_fps={fps}")).record_fps, fps);
    }
    assert_eq!(
        parse_settings("record_fps=29").record_fps,
        DEFAULT_FRAME_RATE
    );
    assert_eq!(
        parse_settings("record_fps=61").record_fps,
        DEFAULT_FRAME_RATE
    );
}

type Switch = fn(&Settings) -> bool;

#[test]
fn switches_follow_their_defaults_and_only_flip_on_the_exact_line() {
    // On by default: only "=0" turns them off. Off by default: only "=1" turns them on.
    let on_by_default: [(&str, Switch); 8] = [
        ("copy", |s| s.copy),
        ("copy_video", |s| s.copy_video),
        ("save_video", |s| s.save_video),
        ("history", |s| s.history),
        ("countdown", |s| s.countdown),
        ("skip_title", |s| s.skip_title),
        ("record_cursor", |s| s.record_cursor),
        ("notify", |s| s.notify),
    ];
    for (key, read) in on_by_default {
        assert!(read(&parse_settings("")), "{key}");
        assert!(!read(&parse_settings(&format!("{key}=0"))), "{key}=0");
        assert!(read(&parse_settings(&format!("{key}=1"))), "{key}=1");
        assert!(read(&parse_settings(&format!("{key}=00"))), "{key}=00");
    }
    let off_by_default: [(&str, Switch); 4] = [
        ("save", |s| s.save),
        ("ask", |s| s.ask),
        ("freeze", |s| s.freeze),
        ("mic", |s| s.mic),
    ];
    for (key, read) in off_by_default {
        assert!(!read(&parse_settings("")), "{key}");
        assert!(read(&parse_settings(&format!("{key}=1"))), "{key}=1");
        assert!(!read(&parse_settings(&format!("{key}=0"))), "{key}=0");
        assert!(!read(&parse_settings(&format!("{key}=11"))), "{key}=11");
    }
    assert!(parse_settings("").system_audio && parse_settings("").app_audio);
    assert!(!parse_settings("system_audio=0").system_audio);
    assert!(!parse_settings("app_audio=0").app_audio);
}

#[test]
fn folders_are_trimmed_and_may_contain_spaces_and_japanese() {
    let parsed = parse_settings("screenshot_folder= D:\\My Shots \r\nvideo_folder=E:\\録画\r\n");
    assert_eq!(parsed.screenshot_folder, "D:\\My Shots");
    assert_eq!(parsed.video_folder, "E:\\録画");
}

#[test]
fn a_chosen_folder_ending_in_a_wide_space_survives_a_restart() {
    // Windows は名前の末尾の全角空白（U+3000）と NBSP を残すので、そう終わるフォルダーを選べる。
    // 読み込みで落とすと、再起動の後は選んだのとは別のフォルダーに保存してしまう。
    for folder in ["D:\\録画\u{3000}", "D:\\Shots\u{a0}"] {
        let chosen = Settings {
            screenshot_folder: folder.to_owned(),
            video_folder: folder.to_owned(),
            ..parse_settings("")
        };
        assert_eq!(
            parse_settings(&settings_text(&chosen)),
            chosen,
            "{folder:?}"
        );
    }
    // 手で書き足した半角の空白とタブは、これまでどおり落とす。
    let typed = parse_settings("screenshot_folder=\t D:\\Shots \t\r\ntheme= dark \r\n");
    assert_eq!(typed.screenshot_folder, "D:\\Shots");
    assert_eq!(typed.theme, AppTheme::Dark);
}

#[test]
fn pen_choices_round_trip_and_fall_back() {
    let _sandbox = crate::test_support::Sandbox::new();
    assert_eq!(load_pen(), (0, 1), "no file yet");
    // Boundary: the last valid color (4) and thickness (2) are kept.
    save_pen(4, 2);
    assert_eq!(load_pen(), (4, 2));
    save_pen(0, 0);
    assert_eq!(load_pen(), (0, 0));
    // One past the end, and unreadable values, give the defaults.
    for (color, thickness) in [(5, 3), (99, 99)] {
        save_pen(color, thickness);
        assert_eq!(load_pen(), (0, 1), "{color},{thickness}");
    }
    let path = std::path::PathBuf::from(std::env::var_os("APPDATA").unwrap())
        .join("Snipping Lite")
        .join("pen.ini");
    fs::write(&path, "color=red\r\nthickness=-1\r\n").unwrap();
    assert_eq!(load_pen(), (0, 1));
    // Golden: the file layout.
    save_pen(2, 1);
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "color=2\r\nthickness=1\r\n"
    );
}

#[test]
fn settings_are_stored_in_the_roaming_folder_and_read_back() {
    let sandbox = crate::test_support::Sandbox::new();
    assert_eq!(load_settings(), parse_settings(""), "no file yet");
    let settings = Settings {
        save: true,
        delay: 10,
        theme: AppTheme::Light,
        video_folder: "E:\\録画".to_owned(),
        ..parse_settings("")
    };
    save_settings(&settings).unwrap();
    assert_eq!(load_settings(), settings);
    let file = sandbox.path().join("roaming/Snipping Lite/settings.ini");
    assert_eq!(fs::read_to_string(file).unwrap(), settings_text(&settings));
}

#[test]
fn saving_fails_without_a_place_to_store_settings() {
    let _sandbox = crate::test_support::Sandbox::new();
    std::env::remove_var("APPDATA");
    assert!(save_settings(&parse_settings("")).is_err());
    assert_eq!(load_settings(), parse_settings(""));
    assert_eq!(load_pen(), (0, 1));
    save_pen(3, 2); // Must not panic.
}

#[test]
fn a_chosen_folder_wins_and_the_defaults_are_per_user() {
    let sandbox = crate::test_support::Sandbox::new();
    let chosen = Settings {
        screenshot_folder: "D:\\Shots".to_owned(),
        video_folder: "D:\\Clips".to_owned(),
        ..parse_settings("")
    };
    assert_eq!(screenshot_folder(&chosen), PathBuf::from("D:\\Shots"));
    assert_eq!(video_folder(&chosen), PathBuf::from("D:\\Clips"));
    let defaults = parse_settings("");
    assert!(screenshot_folder(&defaults).ends_with(tr("Screenshots")));
    assert!(video_folder(&defaults).ends_with(tr("Screen Recordings")));
    assert_ne!(screenshot_folder(&defaults), video_folder(&defaults));
    // A folder id Windows does not know falls back to the profile.
    let unknown = windows_sys::core::GUID {
        data1: 0,
        data2: 0,
        data3: 0,
        data4: [0; 8],
    };
    assert_eq!(known_folder(&unknown), None);
    assert_eq!(
        user_folder(&unknown, "Pictures"),
        sandbox.path().join("profile").join("Pictures")
    );
    assert!(known_folder(&windows_sys::Win32::UI::Shell::FOLDERID_Pictures).is_some());
}

mod compat;
mod damaged;
mod properties;
