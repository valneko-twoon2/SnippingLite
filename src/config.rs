//! Settings shared by the tray process and the window processes.

use std::{fs, path::PathBuf};

/// Capture delays in seconds offered in the settings window.
pub const DELAYS: [u32; 4] = [0, 3, 5, 10];
/// History sizes offered in the settings window; the middle one is the default.
pub const HISTORY_LIMITS: [usize; 4] = [50, 100, 200, 500];

#[derive(Clone, Copy, Debug, PartialEq)]
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
    }
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
    fs::write(
        path,
        format!(
            "copy={}\r\nsave={}\r\nask={}\r\ndelay={}\r\nhistory={}\r\nhistory_limit={}\r\n",
            settings.copy as u8,
            settings.save as u8,
            settings.ask as u8,
            settings.delay,
            settings.history as u8,
            settings.history_limit
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
                history_limit: 200
            }
        );
        let parsed = parse_settings(
            "copy=0\r\nsave=1\r\nask=1\r\ndelay=5\r\nhistory=0\r\nhistory_limit=50\r\n",
        );
        assert_eq!(
            parsed,
            Settings {
                copy: false,
                save: true,
                ask: true,
                delay: 5,
                history: false,
                history_limit: 50
            }
        );
        assert_eq!(parse_settings("delay=7").delay, 0);
        assert_eq!(parse_settings("history_limit=7").history_limit, 200);
    }
}
