//! Settings shared by the tray process and the window processes.

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::i18n::tr;

/// Capture delays in seconds offered in the settings window.
pub const DELAYS: [u32; 4] = [0, 3, 5, 10];
/// History sizes offered in the settings window; the middle one is the default.
pub const HISTORY_LIMITS: [usize; 4] = [50, 100, 200, 500];
/// Frame rates offered for screen recording.
pub const FRAME_RATES: [u32; 2] = [30, 60];
pub const DEFAULT_FRAME_RATE: u32 = 60;

/// The app theme: follow Windows, or always light or dark.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AppTheme {
    System,
    Light,
    Dark,
}

impl AppTheme {
    pub const ALL: [AppTheme; 3] = [AppTheme::System, AppTheme::Light, AppTheme::Dark];

    fn parse(text: &str) -> AppTheme {
        match text {
            "light" => AppTheme::Light,
            "dark" => AppTheme::Dark,
            _ => AppTheme::System,
        }
    }

    fn key(self) -> &'static str {
        match self {
            AppTheme::System => "system",
            AppTheme::Light => "light",
            AppTheme::Dark => "dark",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            AppTheme::System => tr("Use system setting"),
            AppTheme::Light => tr("Light"),
            AppTheme::Dark => tr("Dark"),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub copy: bool,
    pub save: bool,
    /// Ask for a file name when saving.
    pub ask: bool,
    pub delay: u32,
    /// Keep every capture in the history.
    pub history: bool,
    /// The oldest history entries beyond this count are deleted.
    pub history_limit: usize,
    /// Count down 3, 2, 1 before a recording starts.
    pub countdown: bool,
    /// Recording a clicked window leaves out its title bar.
    pub skip_title: bool,
    /// Include the mouse pointer in recorded video.
    pub record_cursor: bool,
    pub record_fps: u32,
    /// Freeze and dim the screen while selecting; otherwise the screen stays live.
    pub freeze: bool,
    /// Show a notification after a screenshot or recording is saved.
    pub notify: bool,
    /// Keep the toolbar on the screen after a snip until it is closed.
    pub keep_toolbar: bool,
    /// Draw a border around screenshots.
    pub outline: bool,
    /// Index into `annotate::COLORS`.
    pub outline_color: usize,
    /// Index into `annotate::OUTLINE_WIDTHS`.
    pub outline_width: usize,
    /// Include the microphone in recordings.
    pub mic: bool,
    /// Include the sound that plays on the PC in recordings.
    pub system_audio: bool,
    /// A window recording keeps only the sound of that window's app.
    pub app_audio: bool,
    /// 録画が終わったら、そのファイルをクリップボードに置く。
    pub copy_video: bool,
    /// 録画を録画の保存先に残す。オフなら一時フォルダーに置く。
    pub save_video: bool,
    pub theme: AppTheme,
    /// Where screenshots are saved; empty means the default folder.
    pub screenshot_folder: String,
    /// Where recordings are saved; empty means the default folder.
    pub video_folder: String,
}

/// The pen color and thickness (indexes) last used in the history editor.
pub fn load_pen() -> (usize, usize) {
    let text = std::env::var_os("APPDATA")
        .map(|x| PathBuf::from(x).join("Snipping Lite").join("pen.ini"))
        .and_then(|p| fs::read_to_string(p).ok())
        .unwrap_or_default();
    let value = |key: &str, limit: usize, default: usize| {
        text.lines()
            .find_map(|s| s.strip_prefix(key))
            .and_then(|v| v.parse().ok())
            .filter(|&v| v < limit)
            .unwrap_or(default)
    };
    (value("color=", 5, 0), value("thickness=", 3, 1))
}

/// Writes through a temporary file so a reader never sees a half-written file.
fn write_atomic(path: &Path, contents: String) -> std::io::Result<()> {
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, contents)?;
    fs::rename(&temporary, path)
}

pub fn save_pen(color: usize, thickness: usize) {
    if let Some(dir) = std::env::var_os("APPDATA").map(|x| PathBuf::from(x).join("Snipping Lite")) {
        let _ = fs::create_dir_all(&dir);
        let _ = write_atomic(
            &dir.join("pen.ini"),
            format!("color={color}\r\nthickness={thickness}\r\n"),
        );
    }
}

fn markup_path() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|x| PathBuf::from(x).join("Snipping Lite").join("markup.ini"))
}

/// ツールバーのペンのボタン（撮影後にその場で書き込む）がオンか。Snipping Tool と同じく次回も
/// 引き継ぐ。設定ウィンドウが開いたまま設定を書き戻しても消えないよう、別のファイルに置く。
pub fn load_markup() -> bool {
    markup_path()
        .and_then(|p| fs::read_to_string(p).ok())
        .is_some_and(|text| text.lines().any(|line| line == "markup=1"))
}

pub fn save_markup(on: bool) {
    if let Some(path) = markup_path() {
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let _ = write_atomic(&path, format!("markup={}\r\n", on as u8));
    }
}

fn settings_path() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|x| PathBuf::from(x).join("Snipping Lite").join("settings.ini"))
}

pub fn parse_settings(text: &str) -> Settings {
    let delay = text
        .lines()
        .find_map(|s| s.strip_prefix("delay="))
        .and_then(|v| v.parse().ok())
        .filter(|d| DELAYS.contains(d))
        .unwrap_or(0);
    let history_limit = text
        .lines()
        .find_map(|s| s.strip_prefix("history_limit="))
        .and_then(|v| v.parse().ok())
        .filter(|l| HISTORY_LIMITS.contains(l))
        .unwrap_or(HISTORY_LIMITS[2]);
    Settings {
        copy: !text.lines().any(|s| s == "copy=0"),
        save: text.lines().any(|s| s == "save=1"),
        ask: text.lines().any(|s| s == "ask=1"),
        delay,
        history: !text.lines().any(|s| s == "history=0"),
        history_limit,
        countdown: !text.lines().any(|s| s == "countdown=0"),
        skip_title: !text.lines().any(|s| s == "skip_title=0"),
        record_cursor: !text.lines().any(|s| s == "record_cursor=0"),
        record_fps: text
            .lines()
            .find_map(|s| s.strip_prefix("record_fps="))
            .and_then(|v| v.parse().ok())
            .filter(|fps| FRAME_RATES.contains(fps))
            .unwrap_or(DEFAULT_FRAME_RATE),
        freeze: text.lines().any(|s| s == "freeze=1"),
        notify: !text.lines().any(|s| s == "notify=0"),
        keep_toolbar: text.lines().any(|s| s == "keep_toolbar=1"),
        outline: text.lines().any(|s| s == "outline=1"),
        outline_color: index(text, "outline_color=", crate::annotate::COLORS.len(), 3),
        outline_width: index(
            text,
            "outline_width=",
            crate::annotate::OUTLINE_WIDTHS.len(),
            1,
        ),
        mic: text.lines().any(|s| s == "mic=1"),
        system_audio: !text.lines().any(|s| s == "system_audio=0"),
        app_audio: !text.lines().any(|s| s == "app_audio=0"),
        copy_video: !text.lines().any(|s| s == "copy_video=0"),
        save_video: !text.lines().any(|s| s == "save_video=0"),
        theme: AppTheme::parse(&value(text, "theme=")),
        screenshot_folder: value(text, "screenshot_folder="),
        video_folder: value(text, "video_folder="),
    }
}

/// A list index stored as a number; anything out of range gives `default`.
fn index(text: &str, key: &str, len: usize, default: usize) -> usize {
    text.lines()
        .find_map(|s| s.strip_prefix(key))
        .and_then(|v| v.parse().ok())
        .filter(|&i| i < len)
        .unwrap_or(default)
}

/// 手で書き足した前後の空白を落とす。落とすのは ASCII の空白（半角の空白、タブ、CR など）だけにする。
/// Windows は名前の末尾の全角空白や NBSP を残すので、それで終わるフォルダーも選べる。
fn value(text: &str, key: &str) -> String {
    text.lines()
        .find_map(|s| s.strip_prefix(key))
        .unwrap_or_default()
        .trim_ascii()
        .to_owned()
}

/// A Windows known folder such as the (possibly OneDrive-redirected) Pictures folder.
fn known_folder(id: &windows_sys::core::GUID) -> Option<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::{System::Com::CoTaskMemFree, UI::Shell::SHGetKnownFolderPath};
    // SAFETY: the returned string is read up to its terminator and freed once.
    unsafe {
        let mut path = std::ptr::null_mut();
        if SHGetKnownFolderPath(id, 0, std::ptr::null_mut(), &mut path) != 0 || path.is_null() {
            return None;
        }
        let mut length = 0;
        while *path.add(length) != 0 {
            length += 1;
        }
        let folder = PathBuf::from(std::ffi::OsString::from_wide(std::slice::from_raw_parts(
            path, length,
        )));
        CoTaskMemFree(path.cast());
        Some(folder)
    }
}

fn user_folder(id: &windows_sys::core::GUID, fallback: &str) -> PathBuf {
    known_folder(id).unwrap_or_else(|| {
        std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .unwrap_or_default()
            .join(fallback)
    })
}

/// The screenshot folder: the chosen one, else the same default as the Windows
/// Snipping Tool (the Pictures folder, then a localized "Screenshots").
pub fn screenshot_folder(settings: &Settings) -> PathBuf {
    if !settings.screenshot_folder.is_empty() {
        return PathBuf::from(&settings.screenshot_folder);
    }
    user_folder(
        &windows_sys::Win32::UI::Shell::FOLDERID_Pictures,
        "Pictures",
    )
    .join(tr("Screenshots"))
}

/// The recording folder: the chosen one, else the Videos folder, then a
/// localized "Screen Recordings", as the Windows Snipping Tool does.
pub fn video_folder(settings: &Settings) -> PathBuf {
    if !settings.video_folder.is_empty() {
        return PathBuf::from(&settings.video_folder);
    }
    user_folder(&windows_sys::Win32::UI::Shell::FOLDERID_Videos, "Videos")
        .join(tr("Screen Recordings"))
}

pub fn load_settings() -> Settings {
    parse_settings(
        &settings_path()
            .and_then(|p| fs::read_to_string(p).ok())
            .unwrap_or_default(),
    )
}

pub fn save_settings(settings: &Settings) -> std::io::Result<()> {
    let path = settings_path().ok_or_else(|| std::io::Error::other("APPDATA is unavailable"))?;
    fs::create_dir_all(
        path.parent()
            .ok_or_else(|| std::io::Error::other("Invalid settings path"))?,
    )?;
    write_atomic(&path, settings_text(settings))
}

/// The settings file contents that `parse_settings` reads back.
fn settings_text(settings: &Settings) -> String {
    format!(
        "copy={}\r\nsave={}\r\nask={}\r\ndelay={}\r\nhistory={}\r\nhistory_limit={}\r\ncountdown={}\r\nskip_title={}\r\nrecord_cursor={}\r\nrecord_fps={}\r\nfreeze={}\r\nnotify={}\r\nkeep_toolbar={}\r\noutline={}\r\noutline_color={}\r\noutline_width={}\r\nmic={}\r\nsystem_audio={}\r\napp_audio={}\r\ncopy_video={}\r\nsave_video={}\r\ntheme={}\r\nscreenshot_folder={}\r\nvideo_folder={}\r\n",
        settings.copy as u8,
        settings.save as u8,
        settings.ask as u8,
        settings.delay,
        settings.history as u8,
        settings.history_limit,
        settings.countdown as u8,
        settings.skip_title as u8,
        settings.record_cursor as u8,
        settings.record_fps,
        settings.freeze as u8,
        settings.notify as u8,
        settings.keep_toolbar as u8,
        settings.outline as u8,
        settings.outline_color,
        settings.outline_width,
        settings.mic as u8,
        settings.system_audio as u8,
        settings.app_audio as u8,
        settings.copy_video as u8,
        settings.save_video as u8,
        settings.theme.key(),
        settings.screenshot_folder,
        settings.video_folder
    )
}

#[cfg(test)]
mod tests;
