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
    /// Freeze and dim the screen while selecting; otherwise the screen stays live.
    pub freeze: bool,
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
        freeze: text.lines().any(|s| s == "freeze=1"),
        screenshot_folder: value(text, "screenshot_folder="),
        video_folder: value(text, "video_folder="),
    }
}

fn value(text: &str, key: &str) -> String {
    text.lines()
        .find_map(|s| s.strip_prefix(key))
        .unwrap_or_default()
        .trim()
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
    write_atomic(
        &path,
        format!(
            "copy={}\r\nsave={}\r\nask={}\r\ndelay={}\r\nhistory={}\r\nhistory_limit={}\r\ncountdown={}\r\nfreeze={}\r\nscreenshot_folder={}\r\nvideo_folder={}\r\n",
            settings.copy as u8,
            settings.save as u8,
            settings.ask as u8,
            settings.delay,
            settings.history as u8,
            settings.history_limit,
            settings.countdown as u8,
            settings.freeze as u8,
            settings.screenshot_folder,
            settings.video_folder
        ),
    )
}

#[cfg(test)]
mod tests {
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
                freeze: false,
                screenshot_folder: String::new(),
                video_folder: String::new()
            }
        );
        let parsed = parse_settings(
            "copy=0\r\nsave=1\r\nask=1\r\ndelay=5\r\nhistory=0\r\nhistory_limit=50\r\ncountdown=0\r\nfreeze=1\r\nscreenshot_folder=D:\\Shots\r\n",
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
                freeze: true,
                screenshot_folder: "D:\\Shots".to_owned(),
                video_folder: String::new()
            }
        );
        assert_eq!(parse_settings("delay=7").delay, 0);
        assert_eq!(parse_settings("history_limit=7").history_limit, 200);
    }

    #[test]
    fn atomic_write_replaces_the_file_and_leaves_no_temporary() {
        let folder =
            std::env::temp_dir().join(format!("snipping-lite-config-{}", std::process::id()));
        fs::create_dir_all(&folder).unwrap();
        let path = folder.join("settings.ini");
        write_atomic(&path, "a".to_owned()).unwrap();
        write_atomic(&path, "b".to_owned()).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "b");
        assert!(!path.with_extension("tmp").exists());
        fs::remove_dir_all(&folder).unwrap();
    }
}
