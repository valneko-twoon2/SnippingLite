//! The screenshot history: every capture is stored with a small thumbnail in
//! `%LOCALAPPDATA%\Snipping Lite\history`, newest first, up to the configured size.

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{config, imaging, ui};

const THUMB_WIDTH: i32 = 320;
const THUMB_HEIGHT: i32 = 200;

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// Capture time in milliseconds since the Unix epoch.
    pub id: u128,
    pub image: PathBuf,
    pub thumbnail: PathBuf,
}

pub fn folder() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|x| PathBuf::from(x).join("Snipping Lite").join("history"))
}

fn entry(folder: &Path, id: u128) -> Entry {
    Entry {
        id,
        image: folder.join(format!("{id}.png")),
        thumbnail: folder.join(format!("{id}.thumb.png")),
    }
}

/// Parses the id out of a full-size image file name such as `1793151938315.png`.
fn parse_id(name: &str) -> Option<u128> {
    name.strip_suffix(".png")?.parse().ok()
}

/// Lists the entries in `folder`, newest first.
fn list_in(folder: &Path) -> Vec<Entry> {
    let mut ids: Vec<u128> = fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|f| parse_id(f.file_name().to_str()?))
        .collect();
    ids.sort_unstable_by(|a, b| b.cmp(a));
    ids.into_iter().map(|id| entry(folder, id)).collect()
}

pub fn list() -> Vec<Entry> {
    folder().map(|f| list_in(&f)).unwrap_or_default()
}

/// Deletes the entries beyond `limit`, oldest first.
fn prune_in(folder: &Path, limit: usize) {
    for old in list_in(folder).into_iter().skip(limit) {
        remove_entry(&old);
    }
}

pub fn remove_entry(entry: &Entry) {
    let _ = fs::remove_file(&entry.image);
    let _ = fs::remove_file(&entry.thumbnail);
}

pub fn clear() {
    for entry in list() {
        remove_entry(&entry);
    }
}

/// Stores a capture (straight-alpha top-down BGRA) and trims the history.
/// `png` is an already saved PNG of the same pixels, copied instead of encoded again.
pub fn add(pixels: &[u8], width: i32, height: i32, png: Option<&Path>) -> std::io::Result<()> {
    let folder = folder().ok_or_else(|| std::io::Error::other("LOCALAPPDATA is unavailable"))?;
    fs::create_dir_all(&folder)?;
    let new = entry(&folder, imaging::timestamp()?);
    if let Err(error) = store(&new, pixels, width, height, png) {
        remove_entry(&new);
        return Err(error);
    }
    prune_in(&folder, config::load_settings().history_limit);
    Ok(())
}

/// Replaces the pictures of an existing entry, for example after annotating it.
pub fn replace(entry: &Entry, pixels: &[u8], width: i32, height: i32) -> std::io::Result<()> {
    store(entry, pixels, width, height, None)
}

/// Writes the full-size image and its thumbnail.
fn store(
    new: &Entry,
    pixels: &[u8],
    width: i32,
    height: i32,
    png: Option<&Path>,
) -> std::io::Result<()> {
    let scale = (THUMB_WIDTH as f32 / width as f32).min(THUMB_HEIGHT as f32 / height as f32);
    let (tw, th) = if scale < 1.0 {
        (
            ((width as f32 * scale).round() as i32).max(1),
            ((height as f32 * scale).round() as i32).max(1),
        )
    } else {
        (width, height)
    };
    let mut thumbnail = ui::thumbnail(pixels, width, height, tw, th);
    unpremultiply(&mut thumbnail);
    imaging::encode(&new.thumbnail, &thumbnail, tw, th)?;
    // PNG encoding is the slowest step of a capture, so reuse the saved file.
    if png.is_some_and(|png| fs::copy(png, &new.image).is_ok()) {
        return Ok(());
    }
    imaging::encode(&new.image, pixels, width, height)
}

/// Converts premultiplied BGRA to straight alpha in place.
fn unpremultiply(pixels: &mut [u8]) {
    for p in pixels.as_chunks_mut::<4>().0 {
        let alpha = p[3] as u32;
        if alpha != 0 && alpha != 255 {
            for c in &mut p[..3] {
                *c = ((*c as u32 * 255 + alpha / 2) / alpha).min(255) as u8;
            }
        }
    }
}

/// Local time of an entry as (year, month, day, hour, minute).
pub fn local_time(id: u128) -> (u16, u16, u16, u16, u16) {
    use windows_sys::Win32::{
        Foundation::{FILETIME, SYSTEMTIME},
        System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime},
    };
    // FILETIME counts 100 ns ticks since 1601; the Unix epoch is 11644473600 s later.
    let ticks = (id as u64 + 11_644_473_600_000) * 10_000;
    let file = FILETIME {
        dwLowDateTime: ticks as u32,
        dwHighDateTime: (ticks >> 32) as u32,
    };
    // SAFETY: all structures are valid, initialized locals.
    unsafe {
        let mut utc: SYSTEMTIME = std::mem::zeroed();
        let mut local: SYSTEMTIME = std::mem::zeroed();
        FileTimeToSystemTime(&file, &mut utc);
        SystemTimeToTzSpecificLocalTime(std::ptr::null(), &utc, &mut local);
        (
            local.wYear,
            local.wMonth,
            local.wDay,
            local.wHour,
            local.wMinute,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_full_size_images_are_entries() {
        assert_eq!(parse_id("1793151938315.png"), Some(1793151938315));
        assert_eq!(parse_id("1793151938315.thumb.png"), None);
        assert_eq!(parse_id("notes.png"), None);
    }

    #[test]
    fn prune_keeps_the_newest_entries() {
        let folder =
            std::env::temp_dir().join(format!("snipping-lite-test-{}", std::process::id()));
        fs::create_dir_all(&folder).unwrap();
        for id in [10u128, 30, 20, 40] {
            let e = entry(&folder, id);
            fs::write(&e.image, b"x").unwrap();
            fs::write(&e.thumbnail, b"x").unwrap();
        }
        prune_in(&folder, 2);
        let left: Vec<u128> = list_in(&folder).iter().map(|e| e.id).collect();
        assert_eq!(left, vec![40, 30]);
        assert!(!entry(&folder, 10).thumbnail.exists());
        fs::remove_dir_all(&folder).unwrap();
    }
}
