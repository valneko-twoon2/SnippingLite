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
    let text = read_text(
        std::env::var_os("APPDATA").map(|x| PathBuf::from(x).join("Snipping Lite").join("pen.ini")),
    );
    (
        index(&text, "color=", 5, 0),
        index(&text, "thickness=", 3, 1),
    )
}

/// 設定ファイルを読む。ないか読めなければ空。UTF-8 として正しくないバイトがあっても、
/// ファイル全体は捨てず、壊れた行だけを `readable_lines` の規則で読めない行にする。
fn read_text(path: Option<PathBuf>) -> String {
    path.and_then(|p| fs::read(p).ok())
        .map(|bytes| readable_lines(&bytes))
        .unwrap_or_default()
}

/// `\n` で区切った行のうち、UTF-8 として正しい行はそのまま残す（`\n` は複数バイトの文字の途中に
/// 現れないので、行ごとに確かめてよい）。壊れたバイトのある行の値は捨てる。
///
/// 壊れた行が、あるキーの最初の行だった場合: その行を飛ばして次の同じキーの行を使うのではなく、
/// そのキーを既定値にする。最初の `=` までが読める壊れた行は `キー=`（値が空）として残すので、
/// `first` はその行を最初の行として選び、空の値はどの設定でも既定値になる（オン・オフ、番号、
/// テーマ、既定のフォルダー）。重なったキーは最初の行だけで決まり、最初の行の値が使えなければ
/// 既定値、という `first` / `switch` の規則のままにするため。最初の `=` までに壊れたバイトがある
/// 行は、どのキーの行とも決められないので丸ごと捨てる（キーはどれも ASCII で `=` を 1 つだけ含む）。
fn readable_lines(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len());
    for line in bytes.split_inclusive(|&b| b == b'\n') {
        match std::str::from_utf8(line) {
            Ok(line) => text.push_str(line),
            Err(_) => {
                let key = line
                    .iter()
                    .position(|&b| b == b'=')
                    .and_then(|end| std::str::from_utf8(&line[..=end]).ok());
                if let Some(key) = key {
                    text.push_str(key);
                    text.push('\n');
                }
            }
        }
    }
    text
}

/// Writes through a temporary file so a reader never sees a half-written file.
fn write_atomic(path: &Path, contents: String) -> std::io::Result<()> {
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, contents)?;
    crate::staging::step("temporary written", path);
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
    switch(&read_text(markup_path()), "markup=", false)
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

/// 同じキーの行が何行あっても、そのキーで始まる最初の行だけを読む。手で書き足した行や、
/// 古い版が残した行があっても、どの設定も同じ規則で決まる（最初の行の値が使えなければ既定値）。
fn first<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines().find_map(|s| s.strip_prefix(key))
}

/// オン・オフの設定。最初の行がちょうど 0 か 1 のときだけ従い、それ以外は既定値。
fn switch(text: &str, key: &str, default: bool) -> bool {
    match first(text, key) {
        Some("0") => false,
        Some("1") => true,
        _ => default,
    }
}

pub fn parse_settings(text: &str) -> Settings {
    let delay = first(text, "delay=")
        .and_then(|v| v.parse().ok())
        .filter(|d| DELAYS.contains(d))
        .unwrap_or(0);
    let history_limit = first(text, "history_limit=")
        .and_then(|v| v.parse().ok())
        .filter(|l| HISTORY_LIMITS.contains(l))
        .unwrap_or(HISTORY_LIMITS[2]);
    Settings {
        copy: switch(text, "copy=", true),
        save: switch(text, "save=", false),
        ask: switch(text, "ask=", false),
        delay,
        history: switch(text, "history=", true),
        history_limit,
        countdown: switch(text, "countdown=", true),
        skip_title: switch(text, "skip_title=", true),
        record_cursor: switch(text, "record_cursor=", true),
        record_fps: first(text, "record_fps=")
            .and_then(|v| v.parse().ok())
            .filter(|fps| FRAME_RATES.contains(fps))
            .unwrap_or(DEFAULT_FRAME_RATE),
        freeze: switch(text, "freeze=", false),
        notify: switch(text, "notify=", true),
        keep_toolbar: switch(text, "keep_toolbar=", false),
        outline: switch(text, "outline=", false),
        outline_color: index(text, "outline_color=", crate::annotate::COLORS.len(), 3),
        outline_width: index(
            text,
            "outline_width=",
            crate::annotate::OUTLINE_WIDTHS.len(),
            1,
        ),
        mic: switch(text, "mic=", false),
        system_audio: switch(text, "system_audio=", true),
        app_audio: switch(text, "app_audio=", true),
        copy_video: switch(text, "copy_video=", true),
        save_video: switch(text, "save_video=", true),
        theme: AppTheme::parse(&value(text, "theme=")),
        screenshot_folder: value(text, "screenshot_folder="),
        video_folder: value(text, "video_folder="),
    }
}

/// A list index stored as a number; anything out of range gives `default`.
fn index(text: &str, key: &str, len: usize, default: usize) -> usize {
    first(text, key)
        .and_then(|v| v.parse().ok())
        .filter(|&i| i < len)
        .unwrap_or(default)
}

/// 手で書き足した前後の空白を落とす。落とすのは ASCII の空白（半角の空白、タブ、CR など）だけにする。
/// Windows は名前の末尾の全角空白や NBSP を残すので、それで終わるフォルダーも選べる。
fn value(text: &str, key: &str) -> String {
    first(text, key).unwrap_or_default().trim_ascii().to_owned()
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
    parse_settings(&read_text(settings_path()))
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
