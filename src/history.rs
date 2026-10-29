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

    fn picture(width: i32, height: i32) -> Vec<u8> {
        [30, 60, 90, 255].repeat((width * height) as usize)
    }

    fn pause() {
        // Entries are named by the millisecond, so two captures need distinct milliseconds.
        std::thread::sleep(std::time::Duration::from_millis(3));
    }

    #[test]
    fn the_history_lives_in_the_local_app_data_folder() {
        let sandbox = crate::test_support::Sandbox::new();
        assert_eq!(
            folder().unwrap(),
            sandbox.path().join("local/Snipping Lite/history")
        );
        assert!(list().is_empty(), "no folder yet");
        std::env::remove_var("LOCALAPPDATA");
        assert_eq!(folder(), None);
        assert!(list().is_empty());
        assert!(add(&picture(2, 2), 2, 2, None).is_err());
    }

    #[test]
    fn captures_are_listed_newest_first_and_can_be_removed() {
        let _sandbox = crate::test_support::Sandbox::new();
        for _ in 0..3 {
            add(&picture(4, 3), 4, 3, None).unwrap();
            pause();
        }
        let entries = list();
        assert_eq!(entries.len(), 3);
        assert!(entries[0].id > entries[1].id && entries[1].id > entries[2].id);
        for e in &entries {
            assert!(e.image.is_file() && e.thumbnail.is_file());
        }
        remove_entry(&entries[1]);
        assert_eq!(
            list().iter().map(|e| e.id).collect::<Vec<_>>(),
            vec![entries[0].id, entries[2].id]
        );
        assert!(!entries[1].image.exists() && !entries[1].thumbnail.exists());
        // Removing an entry that is already gone is harmless.
        remove_entry(&entries[1]);
        clear();
        assert!(list().is_empty());
    }

    #[test]
    fn adding_trims_the_history_to_the_configured_size() {
        let _sandbox = crate::test_support::Sandbox::new();
        let first = {
            add(&picture(2, 2), 2, 2, None).unwrap();
            list()[0].id
        };
        // Boundary: the smallest offered limit keeps exactly that many entries.
        let limit = config::HISTORY_LIMITS[0];
        let folder = folder().unwrap();
        for id in 1..=limit as u128 {
            let e = entry(&folder, id);
            fs::write(&e.image, b"x").unwrap();
            fs::write(&e.thumbnail, b"x").unwrap();
        }
        config::save_settings(&config::Settings {
            history_limit: limit,
            ..config::parse_settings("")
        })
        .unwrap();
        pause();
        add(&picture(2, 2), 2, 2, None).unwrap();
        let ids: Vec<u128> = list().iter().map(|e| e.id).collect();
        assert_eq!(ids.len(), limit);
        assert!(ids[0] > first && ids.contains(&first));
        assert!(!ids.contains(&1), "the oldest entries are deleted first");
    }

    #[test]
    fn thumbnails_keep_the_aspect_ratio_and_never_grow() {
        let _sandbox = crate::test_support::Sandbox::new();
        // Equivalence classes: small, wider than the box, taller than the box, extreme strip.
        for ((width, height), expected) in [
            ((100, 50), (100, 50)),
            ((320, 200), (320, 200)),
            ((640, 400), (320, 200)),
            ((640, 100), (320, 50)),
            ((100, 800), (25, 200)),
            ((4000, 1), (320, 1)),
        ] {
            clear();
            add(&picture(width, height), width, height, None).unwrap();
            let entry = list().remove(0);
            let (w, h, _) = imaging::decode(&entry.thumbnail).unwrap();
            assert_eq!((w, h), expected, "{width}x{height}");
            let (w, h, _) = imaging::decode(&entry.image).unwrap();
            assert_eq!((w, h), (width, height));
            pause();
        }
    }

    #[test]
    fn a_saved_file_is_reused_when_it_exists() {
        let sandbox = crate::test_support::Sandbox::new();
        let saved = sandbox.path().join("saved.png");
        imaging::encode(&saved, &picture(5, 5), 5, 5).unwrap();
        add(&picture(5, 5), 5, 5, Some(&saved)).unwrap();
        let first = list().remove(0);
        assert_eq!(fs::read(&first.image).unwrap(), fs::read(&saved).unwrap());
        // A file that cannot be copied falls back to encoding the pixels.
        pause();
        add(&picture(5, 5), 5, 5, Some(&sandbox.path().join("gone.png"))).unwrap();
        let second = list().remove(0);
        assert_ne!(second.id, first.id);
        assert_eq!(imaging::decode(&second.image).unwrap().0, 5);
    }

    #[test]
    fn replacing_an_entry_rewrites_both_pictures() {
        let _sandbox = crate::test_support::Sandbox::new();
        add(&picture(10, 10), 10, 10, None).unwrap();
        let entry = list().remove(0);
        replace(&entry, &picture(20, 8), 20, 8).unwrap();
        assert_eq!(imaging::decode(&entry.image).unwrap().0, 20);
        assert_eq!(imaging::decode(&entry.thumbnail).unwrap().1, 8);
        assert_eq!(list().len(), 1);
        // The folder is gone: nothing can be written.
        let broken = Entry {
            id: 1,
            image: entry.image.with_file_name("missing/1.png"),
            thumbnail: entry.image.with_file_name("missing/1.thumb.png"),
        };
        assert!(replace(&broken, &picture(2, 2), 2, 2).is_err());
    }

    #[test]
    fn straight_alpha_is_restored_from_premultiplied_pixels() {
        // Decision table over alpha: clear and opaque pixels are left alone.
        let mut pixels = [
            50, 60, 70, 0, //
            50, 60, 70, 255, //
            64, 32, 16, 128, //
            200, 10, 0, 100, // channel above alpha: clamped
            1, 1, 1, 1, //
        ];
        unpremultiply(&mut pixels);
        assert_eq!(&pixels[..4], &[50, 60, 70, 0]);
        assert_eq!(&pixels[4..8], &[50, 60, 70, 255]);
        assert_eq!(&pixels[8..12], &[128, 64, 32, 128]);
        assert_eq!(&pixels[12..16], &[255, 26, 0, 100]);
        assert_eq!(&pixels[16..20], &[255, 255, 255, 1]);
    }

    #[test]
    fn local_time_follows_the_calendar() {
        // 2024-06-15 12:00 UTC is the same calendar day in every time zone but UTC+12 and later.
        let noon = 1_718_452_800_000u128;
        let (year, month, day, hour, minute) = local_time(noon);
        assert_eq!((year, month), (2024, 6));
        assert!((14..=16).contains(&day) && hour < 24 && minute < 60);
        // One minute later moves the clock by one minute, and a day later moves the date.
        let (_, _, _, h2, m2) = local_time(noon + 60_000);
        assert_eq!((h2 * 60 + m2 + 1440 - (hour * 60 + minute)) % 1440, 1);
        assert_ne!(local_time(noon + 86_400_000).2, day);
        // The Unix epoch is on the last day of 1969 or the first of 1970.
        let (year, month, ..) = local_time(0);
        assert!((year == 1969 && month == 12) || (year == 1970 && month == 1));
    }
}
