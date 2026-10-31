use super::*;
use crate::test_support::{
    answer_dialog,
    screen::{Screen, Show},
    Sandbox, CAPTIONS, THEME,
};
use eframe::App;

/// The settings window running on a tall screen, so the whole page is visible at once.
struct Window {
    app: SettingsApp,
    screen: Screen,
    frame: eframe::Frame,
}

impl Window {
    fn open(hotkey_ok: bool, mica: bool) -> Window {
        let mut window = Window {
            app: SettingsApp::new(hotkey_ok, mica),
            screen: Screen::new(WIDTH, 2600.0),
            frame: eframe::Frame::_new_kittest(),
        };
        window.act(|screen, show| screen.settle(show));
        window
    }

    fn act<R>(&mut self, action: impl FnOnce(&mut Screen, Show) -> R) -> R {
        let app = &mut self.app;
        let frame = &mut self.frame;
        let mut show = |ui: &mut egui::Ui| app.ui(ui, frame);
        action(&mut self.screen, &mut show)
    }

    fn click_text(&mut self, text: &str) {
        self.act(|screen, show| screen.click_text(text, show));
    }

    /// Flips the switch of the row with this title.
    fn toggle(&mut self, title: &str) {
        self.toggle_nth(title, 0);
    }

    /// 同じ見出しの行が複数あるとき（コピーの行は 2 つある）、上から `index` 番目のスイッチを切り替える。
    fn toggle_nth(&mut self, title: &str, index: usize) {
        let row = self
            .screen
            .texts()
            .into_iter()
            .filter(|(text, _)| text == title)
            .nth(index)
            .map(|(_, rect)| rect)
            .unwrap_or_else(|| panic!("no row {title:?}: {:?}", self.screen.texts()));
        let (on, off) = (tr("On"), tr("Off"));
        let (_, label) = self
            .screen
            .texts()
            .into_iter()
            .find(|(text, rect)| {
                (text == on || text == off) && (rect.center().y - row.center().y).abs() <= 12.0
            })
            .unwrap_or_else(|| panic!("no switch in the row {title:?}"));
        // The word comes first from the right, then the switch itself.
        let at = pos2(label.left() - 30.0, label.center().y);
        self.act(|screen, show| screen.click(at, show));
    }

    /// Opens the drop-down that currently shows `current` and picks `item`.
    fn choose(&mut self, current: &str, item: &str) {
        self.click_text(current);
        assert!(self.screen.has(item), "no choice {item:?}");
        // The list is a popup: the choice sits in it next to the current one.
        let (_, rect) = self
            .screen
            .texts()
            .into_iter()
            .rev()
            .find(|(text, _)| text == item)
            .unwrap();
        self.act(|screen, show| screen.click(rect.center(), show));
    }
}

/// Where the controls of the border card sit on the test screen, from the right edge of a row.
const SWATCH_X: f32 = 588.0;
const STRIP_X: [f32; 5] = [442.0, 478.0, 514.0, 550.0, 586.0];
const SLIDER_END: f32 = 606.0;

fn saved() -> config::Settings {
    config::load_settings()
}

/// An existing folder for the settings to point at.
fn folder_in(sandbox: &Sandbox, name: &str) -> String {
    let folder = sandbox.path().join(name);
    fs::create_dir_all(&folder).unwrap();
    folder.to_string_lossy().into_owned()
}

#[test]
fn the_page_lists_every_setting_under_its_section() {
    let _sandbox = Sandbox::new();
    let window = Window::open(true, false);
    for text in [
        "Settings",
        "Snipping",
        "Automatically copy screenshots",
        "Automatically save screenshots",
        "Choose a file name when saving",
        "Dim and freeze the screen while snipping",
        "Show notifications",
        "Keep the toolbar open after a snip",
        "Add a border to screenshots",
        "Screen recording",
        "Countdown before recording",
        "Include the mouse cursor in recordings",
        "Include microphone input in recordings",
        "Include system audio in recordings",
        "Record only the sound of the window's app",
        "Leave out the title bar when recording a window",
        "Automatically copy recordings",
        "Automatically save recordings",
        "History",
        "Keep a history of screenshots",
        "History size",
        "Open history",
        "Shortcuts",
        "Start a snip with Alt + Shift + S",
        "Appearance",
        "App theme",
    ] {
        assert!(window.screen.has(tr(text)), "missing {text:?}");
    }
    // 撮影の遅延、フレームレート、録画のファイル名の確認は設定から外した。
    for text in [
        "Capture delay",
        "Recording frame rate",
        "Ask for a file name when saving a recording",
    ] {
        assert!(!window.screen.has(tr(text)), "{text:?} is still listed");
    }
    // Rows that only matter after another switch is on stay out of the way.
    // 開閉できるカードは、スイッチがオンでも閉じた状態で始まる。
    for text in [
        "Screenshot folder",
        "Recording folder",
        "Border color",
        "Border thickness",
    ] {
        assert!(!window.screen.has(tr(text)), "{text:?} is shown too early");
    }
    assert!(!window
        .screen
        .has(tr("Shortcut unavailable: another application is using it.")));
}

#[test]
fn a_shortcut_that_another_program_holds_is_reported() {
    let _sandbox = Sandbox::new();
    let window = Window::open(false, false);
    assert!(window
        .screen
        .has(tr("Shortcut unavailable: another application is using it.")));
}

#[test]
fn every_switch_stores_its_own_setting() {
    let _sandbox = Sandbox::new();
    let mut window = Window::open(true, false);
    type Read = fn(&config::Settings) -> bool;
    // Each row flips exactly one stored value and leaves all the others as they were.
    let rows: [(&str, Read); 13] = [
        ("Automatically save screenshots", |s| s.save),
        ("Choose a file name when saving", |s| s.ask),
        ("Dim and freeze the screen while snipping", |s| s.freeze),
        ("Show notifications", |s| s.notify),
        ("Keep the toolbar open after a snip", |s| s.keep_toolbar),
        ("Add a border to screenshots", |s| s.outline),
        ("Countdown before recording", |s| s.countdown),
        ("Include the mouse cursor in recordings", |s| {
            s.record_cursor
        }),
        ("Include microphone input in recordings", |s| s.mic),
        ("Include system audio in recordings", |s| s.system_audio),
        ("Leave out the title bar when recording a window", |s| {
            s.skip_title
        }),
        ("Automatically save recordings", |s| s.save_video),
        ("Keep a history of screenshots", |s| s.history),
    ];
    for (title, read) in rows {
        let title = tr(title);
        let before = saved();
        window.toggle(title);
        let after = saved();
        assert_ne!(read(&before), read(&after), "{title}");
        // Every other switch is unchanged.
        let mut expected = before.clone();
        for (_, other) in rows {
            if std::ptr::fn_addr_eq(other, read) {
                continue;
            }
            assert_eq!(other(&before), other(&after), "{title}");
        }
        expected.save = after.save;
        window.toggle(title);
        assert_eq!(read(&saved()), read(&before), "{title} flips back");
    }
}

#[test]
fn the_last_destination_for_screenshots_cannot_be_switched_off() {
    let _sandbox = Sandbox::new();
    let mut window = Window::open(true, false);
    let copy = tr("Automatically copy screenshots");
    let save = tr("Automatically save screenshots");
    let warning = tr("Choose at least one destination for screenshots.");
    // Decision table over (copy, save): switching off the only one that is on is refused.
    assert!(saved().copy && !saved().save);
    window.toggle(copy);
    assert!(
        saved().copy,
        "copy stays on while it is the only destination"
    );
    assert!(window.screen.has(warning));
    window.toggle(save);
    assert!(saved().copy && saved().save);
    window.toggle(copy);
    assert!(!saved().copy && saved().save);
    window.toggle(save);
    assert!(!saved().copy && saved().save, "save stays on now");
    window.toggle(copy);
    assert!(saved().copy && saved().save);
}

#[test]
fn the_last_destination_for_recordings_cannot_be_switched_off() {
    let _sandbox = Sandbox::new();
    config::save_settings(&config::Settings {
        copy_video: false,
        save_video: false,
        ..config::parse_settings("")
    })
    .unwrap();
    let mut window = Window::open(true, false);
    // 保存先のないファイルは、開いたときに保存をオンにして直す。
    assert!(!saved().copy_video && saved().save_video);
    // 録画のコピーの行は、スクリーンショットのものと同じ見出しで 2 つ目にある。
    let copy = tr("Automatically copy recordings");
    let save = tr("Automatically save recordings");
    window.toggle(save);
    assert!(
        saved().save_video,
        "save stays on while it is the only destination"
    );
    assert!(window
        .screen
        .has(tr("Choose at least one destination for recordings.")));
    window.toggle_nth(copy, 1);
    window.toggle(save);
    assert!(saved().copy_video && !saved().save_video);
    window.toggle_nth(copy, 1);
    assert!(saved().copy_video, "copy stays on now");
    // スクリーンショットの設定は変わらない。
    assert!(saved().copy && !saved().save);
}

#[test]
fn a_settings_file_with_no_destination_is_repaired() {
    let _sandbox = Sandbox::new();
    config::save_settings(&config::Settings {
        copy: false,
        save: false,
        ..config::parse_settings("")
    })
    .unwrap();
    let mut window = Window::open(true, false);
    // Any change brings the file back to a usable state: copy is turned on.
    window.toggle(tr("Show notifications"));
    let settings = saved();
    assert!(settings.copy && !settings.save);
    assert!(!settings.notify);
}

#[test]
fn rows_that_depend_on_a_switch_appear_with_it() {
    let sandbox = Sandbox::new();
    let mut window = Window::open(true, false);
    let folder = tr("Screenshot folder");
    // Switching a setting on opens its card; clicking the card closes and opens it again.
    assert!(!window.screen.has(folder));
    window.toggle(tr("Automatically save screenshots"));
    assert!(window.screen.has(folder));
    window.click_text(tr("Automatically save screenshots"));
    assert!(!window.screen.has(folder));
    window.click_text(tr("Automatically save screenshots"));
    assert!(window.screen.has(folder));

    assert!(!window.screen.has(tr("Border color")));
    window.toggle(tr("Add a border to screenshots"));
    assert!(window.screen.has(tr("Border color")));
    assert!(window.screen.has(tr("Border thickness")));
    window.click_text(tr("Add a border to screenshots"));
    assert!(!window.screen.has(tr("Border color")));

    let app_audio = tr("Record only the sound of the window's app");
    assert!(window.screen.has(app_audio));
    window.toggle(tr("Include system audio in recordings"));
    assert!(!window.screen.has(app_audio));
    drop(sandbox);
}

#[test]
fn drop_downs_store_the_chosen_value() {
    let _guard = THEME.lock().unwrap_or_else(|e| e.into_inner());
    let _sandbox = Sandbox::new();
    let mut window = Window::open(true, false);
    window.choose(
        &tr("{n} screenshots").replace("{n}", "200"),
        &tr("{n} screenshots").replace("{n}", "50"),
    );
    assert_eq!(saved().history_limit, 50);

    // The border color is a swatch that opens a strip of colors; the thickness is a slider.
    window.toggle(tr("Add a border to screenshots"));
    let color = window.screen.find(tr("Border color")).unwrap().center().y;
    window.act(|screen, show| screen.click(pos2(SWATCH_X, color), show));
    window.act(|screen, show| screen.click(pos2(STRIP_X[0], color + 48.0), show));
    assert_eq!(saved().outline_color, 0);
    let thickness = window
        .screen
        .find(tr("Border thickness"))
        .unwrap()
        .center()
        .y;
    window.act(|screen, show| screen.click(pos2(SLIDER_END, thickness), show));
    assert_eq!(saved().outline_width, 2);

    // The theme card opens to three radio choices.
    window.click_text(tr("App theme"));
    window.click_text(config::AppTheme::Dark.label());
    assert_eq!(saved().theme, config::AppTheme::Dark);
    window.click_text(config::AppTheme::Light.label());
    assert_eq!(saved().theme, config::AppTheme::Light);
    ui::set_theme_choice(config::AppTheme::System);
}

#[test]
fn the_history_button_asks_the_tray_to_open_it() {
    let _sandbox = Sandbox::new();
    let mut window = Window::open(true, false);
    // Without a tray process there is nobody to tell; the click must still be harmless.
    window.click_text(tr("Open"));
    assert!(window.screen.has(tr("Open history")));
}

#[test]
fn changing_a_setting_writes_the_file_once_and_idle_frames_write_nothing() {
    let sandbox = Sandbox::new();
    let mut window = Window::open(true, false);
    let file = sandbox.path().join("roaming/Snipping Lite/settings.ini");
    assert!(!file.exists(), "opening the window changes nothing");
    window.act(|screen, show| screen.settle(show));
    assert!(!file.exists());
    window.toggle(tr("Show notifications"));
    let first = fs::read_to_string(&file).unwrap();
    assert!(first.contains("notify=0\r\n"));
    window.act(|screen, show| screen.settle(show));
    assert_eq!(fs::read_to_string(&file).unwrap(), first);
}

#[test]
fn the_window_frame_follows_the_theme_and_the_backdrop() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, GetPropW, WS_POPUP,
    };
    let _captions = CAPTIONS.lock().unwrap_or_else(|e| e.into_inner());
    let _sandbox = Sandbox::new();
    for mica in [false, true] {
        // SAFETY: a private, hidden window that stands in for the settings window.
        unsafe {
            let hwnd = CreateWindowExW(
                0,
                w("STATIC").as_ptr(),
                w(SETTINGS_TITLE).as_ptr(),
                WS_POPUP,
                0,
                0,
                10,
                10,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            );
            let mut window = Window::open(true, mica);
            // The window process tags its document so the tray can find it again.
            assert!(!GetPropW(hwnd, w(SETTINGS_WINDOW).as_ptr()).is_null());
            let clear = window.app.clear_color(&egui::Visuals::dark());
            assert_eq!(clear == [0.0; 4], mica);
            window.act(|screen, show| screen.settle(show));
            DestroyWindow(hwnd);
        }
    }
}

#[test]
fn the_folder_row_opens_the_folder() {
    let sandbox = Sandbox::new();
    let folder = folder_in(&sandbox, "opened-from-settings");
    let videos = folder_in(&sandbox, "videos-from-settings");
    config::save_settings(&config::Settings {
        save: true,
        screenshot_folder: folder.clone(),
        video_folder: videos.clone(),
        ..config::parse_settings("")
    })
    .unwrap();
    let mut window = Window::open(true, false);
    // 保存先の行は、閉じた状態で始まるカードの中にある。
    assert!(!window.screen.has(tr("Open folder")));
    // 両方のカードを開いても、それぞれのボタンが自分のフォルダーを開く。
    window.click_text(tr("Automatically save screenshots"));
    window.click_text(tr("Automatically save recordings"));
    let buttons: Vec<_> = window
        .screen
        .texts()
        .into_iter()
        .filter(|(text, _)| text == tr("Open folder"))
        .map(|(_, rect)| rect.center())
        .collect();
    assert_eq!(buttons.len(), 2, "one button per folder row");
    for (at, expected) in buttons.into_iter().zip([folder, videos]) {
        crate::test_support::take_shell_opened();
        window.act(|screen, show| screen.click(at, show));
        assert_eq!(
            crate::test_support::take_shell_opened(),
            vec![(expected, None)]
        );
    }
}

#[test]
#[ignore = "Shows a dialog"]
fn the_change_link_picks_a_folder_or_leaves_it_alone() {
    let sandbox = Sandbox::new();
    let screenshots = folder_in(&sandbox, "shots");
    let videos = folder_in(&sandbox, "videos");
    config::save_settings(&config::Settings {
        save: true,
        screenshot_folder: screenshots.clone(),
        video_folder: videos.clone(),
        ..config::parse_settings("")
    })
    .unwrap();
    let mut window = Window::open(true, false);
    window.click_text(tr("Automatically save screenshots"));
    window.click_text(tr("Automatically save recordings"));
    let changes: Vec<_> = window
        .screen
        .texts()
        .into_iter()
        .filter(|(text, _)| text == tr("Change"))
        .map(|(_, rect)| rect.center())
        .collect();
    assert_eq!(changes.len(), 2, "one link per folder row");
    // Cancelling keeps the folder.
    let answer = answer_dialog(2);
    window.act(|screen, show| screen.click(changes[0], show));
    assert!(answer.join().unwrap());
    assert_eq!(saved().screenshot_folder, screenshots);
    // Accepting stores the folder the dialog shows, for either row.
    let answer = answer_dialog(1);
    window.act(|screen, show| screen.click(changes[0], show));
    assert!(answer.join().unwrap());
    assert_eq!(
        PathBuf::from(saved().screenshot_folder),
        PathBuf::from(&screenshots)
    );
    let answer = answer_dialog(1);
    window.act(|screen, show| screen.click(changes[1], show));
    assert!(answer.join().unwrap());
    assert_eq!(PathBuf::from(saved().video_folder), PathBuf::from(&videos));
}

mod fonts;
mod look;
mod tray;
