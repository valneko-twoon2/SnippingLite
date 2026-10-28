//! Settings shared by the tray process and the settings window process.

use std::{fs, path::PathBuf};

/// Capture delays in seconds offered in the settings window.
pub const DELAYS: [u32; 4] = [0, 3, 5, 10];

fn settings_path() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|x| PathBuf::from(x).join("Snipping Lite").join("settings.ini"))
}

/// Returns (copy, save, ask for a file name, delay in seconds).
pub fn parse_settings(text: &str) -> (bool, bool, bool, u32) {
    let delay = text
        .lines()
        .find_map(|s| s.strip_prefix("delay="))
        .and_then(|v| v.parse().ok())
        .filter(|d| DELAYS.contains(d))
        .unwrap_or(0);
    (
        !text.lines().any(|s| s == "copy=0"),
        text.lines().any(|s| s == "save=1"),
        text.lines().any(|s| s == "ask=1"),
        delay,
    )
}

pub fn load_settings() -> (bool, bool, bool, u32) {
    parse_settings(
        &settings_path()
            .and_then(|p| fs::read_to_string(p).ok())
            .unwrap_or_default(),
    )
}

pub fn save_settings(copy: bool, save: bool, ask: bool, delay: u32) -> std::io::Result<()> {
    let path = settings_path().ok_or_else(|| std::io::Error::other("APPDATA is unavailable"))?;
    fs::create_dir_all(
        path.parent()
            .ok_or_else(|| std::io::Error::other("Invalid settings path"))?,
    )?;
    fs::write(
        path,
        format!(
            "copy={}\r\nsave={}\r\nask={}\r\ndelay={}\r\n",
            copy as u8, save as u8, ask as u8, delay
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_fall_back_to_defaults() {
        assert_eq!(parse_settings(""), (true, false, false, 0));
        assert_eq!(
            parse_settings("copy=0\r\nsave=1\r\nask=1\r\ndelay=5\r\n"),
            (false, true, true, 5)
        );
        assert_eq!(parse_settings("delay=7").3, 0);
    }
}
