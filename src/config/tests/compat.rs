//! Settings files written by older versions must keep working after an update: what the user
//! chose stays, and every setting the old file does not know takes its default.

use super::*;

/// The keys each released layout added to the file, with values that differ from the defaults
/// so that a lost value shows. Every layout contains the ones before it.
const GENERATIONS: [(&str, &str); 10] = [
    ("first", "copy=0\r\nsave=1\r\nask=1\r\ndelay=3\r\n"),
    ("history", "history=0\r\n"),
    ("history size", "history_limit=50\r\n"),
    (
        "live capture and folders",
        "countdown=0\r\nfreeze=1\r\nscreenshot_folder=D:/Shots\r\nvideo_folder=D:/Clips\r\n",
    ),
    ("window recording", "skip_title=0\r\n"),
    (
        "cursor and frame rate",
        "record_cursor=0\r\nrecord_fps=30\r\n",
    ),
    (
        "notifications and theme",
        "notify=0\r\ntheme=dark\r\nask_video=1\r\n",
    ),
    (
        "audio and kept toolbar",
        "mic=1\r\nsystem_audio=0\r\napp_audio=0\r\nkeep_toolbar=1\r\n",
    ),
    (
        "border",
        "outline=1\r\noutline_color=1\r\noutline_width=2\r\n",
    ),
    // ask_video= の行は録画の保存先の設定に置き換わり、読み飛ばされる。
    ("recording destinations", "copy_video=0\r\nsave_video=0\r\n"),
];

/// The settings that the first `count` layouts describe on top of the defaults.
fn expected(count: usize) -> Settings {
    let mut s = parse_settings("");
    for layout in 0..count {
        match layout {
            0 => (s.copy, s.save, s.ask, s.delay) = (false, true, true, 3),
            1 => s.history = false,
            2 => s.history_limit = 50,
            3 => {
                s.countdown = false;
                s.freeze = true;
                s.screenshot_folder = "D:/Shots".into();
                s.video_folder = "D:/Clips".into();
            }
            4 => s.skip_title = false,
            5 => (s.record_cursor, s.record_fps) = (false, 30),
            6 => (s.notify, s.theme) = (false, AppTheme::Dark),
            7 => (s.mic, s.system_audio, s.app_audio, s.keep_toolbar) = (true, false, false, true),
            8 => (s.outline, s.outline_color, s.outline_width) = (true, 1, 2),
            _ => (s.copy_video, s.save_video) = (false, false),
        }
    }
    s
}

#[test]
fn a_file_from_every_older_layout_keeps_its_values_and_defaults_the_new_settings() {
    for count in 1..=GENERATIONS.len() {
        let text: String = GENERATIONS[..count].iter().map(|(_, t)| *t).collect();
        assert_eq!(
            parse_settings(&text),
            expected(count),
            "layout {}",
            GENERATIONS[count - 1].0
        );
    }
}

#[test]
fn the_key_order_of_the_file_does_not_matter() {
    let forward: String = GENERATIONS.iter().map(|(_, t)| *t).collect();
    let mut lines: Vec<&str> = forward.split("\r\n").filter(|l| !l.is_empty()).collect();
    lines.reverse();
    assert_eq!(
        parse_settings(&lines.join("\r\n")),
        parse_settings(&forward)
    );
}

#[test]
fn line_ending_style_and_unknown_keys_are_tolerated() {
    let crlf = "save=1\r\ndelay=5\r\nfuture_setting=7\r\ntheme=light\r\n";
    let lf = crlf.replace("\r\n", "\n");
    assert_eq!(parse_settings(crlf), parse_settings(&lf));
    let s = parse_settings(crlf);
    assert!(s.save);
    assert_eq!((s.delay, s.theme), (5, AppTheme::Light));
}

#[test]
fn a_damaged_file_reads_as_defaults_instead_of_failing() {
    for text in [
        "\u{0}\u{0}\u{0}",
        "=\r\n==\r\n",
        "delay=\r\ntheme=",
        "\u{feff}save=1",
    ] {
        assert_eq!(parse_settings(text), parse_settings(""), "{text:?}");
    }
    // A byte-order mark hides only the first key; the rest of the file still counts.
    assert_eq!(
        parse_settings("\u{feff}save=1\r\nask=1\r\n"),
        Settings {
            ask: true,
            ..parse_settings("")
        }
    );
}

#[test]
fn what_the_current_version_writes_still_starts_with_the_first_release_keys() {
    let text = settings_text(&parse_settings(""));
    assert!(text.starts_with("copy=1\r\nsave=0\r\nask=0\r\ndelay=0\r\n"));
}
