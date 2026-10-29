//! Image encoding, decoding, clipboard and file dialog helpers shared by the
//! tray process and the history window process.

use std::{
    ffi::OsString,
    fs,
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
    ptr::{null, null_mut},
    sync::atomic::{AtomicUsize, Ordering},
};
use windows_sys::{
    core::GUID,
    Win32::{
        Foundation::{GlobalFree, HWND},
        Graphics::{
            Gdi::{BITMAPV5HEADER, BI_BITFIELDS},
            GdiPlus::{
                BitmapData, GdipBitmapLockBits, GdipBitmapUnlockBits, GdipCreateBitmapFromFile,
                GdipCreateBitmapFromScan0, GdipDisposeImage, GdipGetImageHeight, GdipGetImageWidth,
                GdipSaveImageToFile, GdiplusStartup, GdiplusStartupInput, GdiplusStartupOutput,
                Rect,
            },
        },
        System::{
            DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData},
            Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE},
        },
        UI::Controls::Dialogs::{
            GetSaveFileNameW, OFN_EXPLORER, OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST, OPENFILENAMEW,
        },
    },
};

use crate::i18n::tr;
use crate::w;

static GDIPLUS: AtomicUsize = AtomicUsize::new(0);

/// GDI+ is started on first use; it is only needed for encoding and decoding.
fn start_gdiplus() -> bool {
    if GDIPLUS.load(Ordering::Acquire) != 0 {
        return true;
    }
    let input = GdiplusStartupInput {
        GdiplusVersion: 1,
        DebugEventCallback: 0,
        SuppressBackgroundThread: 0,
        SuppressExternalCodecs: 0,
    };
    // SAFETY: the structures are valid for the duration of the call.
    unsafe {
        let mut output: GdiplusStartupOutput = std::mem::zeroed();
        let mut token = 0;
        if GdiplusStartup(&mut token, &input, &mut output) != 0 {
            return false;
        }
        GDIPLUS.store(token, Ordering::Release);
    }
    true
}

fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

/// Encodes straight-alpha top-down BGRA pixels; the extension picks PNG or BMP.
pub fn encode(path: &Path, pixels: &[u8], width: i32, height: i32) -> std::io::Result<()> {
    if !start_gdiplus() {
        return Err(std::io::Error::other("Could not start GDI+"));
    }
    let bmp = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("bmp"));
    let encoder = GUID {
        data1: if bmp { 0x557cf400 } else { 0x557cf406 },
        data2: 0x1a04,
        data3: 0x11d3,
        data4: [0x9a, 0x73, 0x00, 0x00, 0xf8, 0x1e, 0xf3, 0x2e],
    };
    // SAFETY: `pixels` outlives the bitmap, which is disposed before returning.
    unsafe {
        let mut image = null_mut();
        // PixelFormat32bppARGB
        if GdipCreateBitmapFromScan0(
            width,
            height,
            width * 4,
            0x0026200A,
            pixels.as_ptr(),
            &mut image,
        ) != 0
        {
            return Err(std::io::Error::other("Could not create the image"));
        }
        let name = wide(path);
        let status = GdipSaveImageToFile(image.cast(), name.as_ptr(), &encoder, null());
        GdipDisposeImage(image.cast());
        if status == 0 {
            Ok(())
        } else {
            Err(std::io::Error::other("Could not encode the image"))
        }
    }
}

/// Decodes an image file into straight-alpha top-down BGRA pixels.
pub fn decode(path: &Path) -> Option<(i32, i32, Vec<u8>)> {
    if !start_gdiplus() {
        return None;
    }
    // SAFETY: the locked buffer is only read while the lock is held.
    unsafe {
        let name = wide(path);
        let mut bitmap = null_mut();
        if GdipCreateBitmapFromFile(name.as_ptr(), &mut bitmap) != 0 {
            return None;
        }
        let (mut width, mut height) = (0u32, 0u32);
        GdipGetImageWidth(bitmap.cast(), &mut width);
        GdipGetImageHeight(bitmap.cast(), &mut height);
        let area = Rect {
            X: 0,
            Y: 0,
            Width: width as i32,
            Height: height as i32,
        };
        let mut data: BitmapData = std::mem::zeroed();
        let mut pixels = None;
        // ImageLockModeRead, PixelFormat32bppARGB
        if width > 0
            && height > 0
            && GdipBitmapLockBits(bitmap, &area, 1, 0x0026200A, &mut data) == 0
        {
            let row = width as usize * 4;
            let mut out = vec![0u8; row * height as usize];
            for y in 0..height as usize {
                let source = data
                    .Scan0
                    .cast::<u8>()
                    .offset(y as isize * data.Stride as isize);
                std::ptr::copy_nonoverlapping(source, out.as_mut_ptr().add(y * row), row);
            }
            GdipBitmapUnlockBits(bitmap, &mut data);
            pixels = Some(out);
        }
        GdipDisposeImage(bitmap.cast());
        pixels.map(|p| (width as i32, height as i32, p))
    }
}

/// Puts the image on the clipboard as a 32-bit DIB with alpha.
pub fn copy_to_clipboard(owner: HWND, pixels: &[u8], width: i32, height: i32) -> bool {
    // SAFETY: the allocation is sized for the header and all pixel rows.
    unsafe {
        let header = std::mem::size_of::<BITMAPV5HEADER>();
        let global = GlobalAlloc(GMEM_MOVEABLE, header + pixels.len());
        if global.is_null() {
            return false;
        }
        let data = GlobalLock(global).cast::<u8>();
        if data.is_null() {
            GlobalFree(global);
            return false;
        }
        let mut info: BITMAPV5HEADER = std::mem::zeroed();
        info.bV5Size = header as u32;
        info.bV5Width = width;
        info.bV5Height = height;
        info.bV5Planes = 1;
        info.bV5BitCount = 32;
        info.bV5Compression = BI_BITFIELDS;
        info.bV5SizeImage = pixels.len() as u32;
        info.bV5RedMask = 0x00ff0000;
        info.bV5GreenMask = 0x0000ff00;
        info.bV5BlueMask = 0x000000ff;
        info.bV5AlphaMask = 0xff000000;
        info.bV5CSType = 0x73524742;
        info.bV5Intent = 4;
        std::ptr::copy_nonoverlapping((&info as *const BITMAPV5HEADER).cast::<u8>(), data, header);
        // Clipboard DIBs are bottom-up while the rows are top-down.
        let row = width as usize * 4;
        for (i, line) in pixels.chunks_exact(row).rev().enumerate() {
            std::ptr::copy_nonoverlapping(line.as_ptr(), data.add(header + i * row), row);
        }
        GlobalUnlock(global);
        put_on_clipboard(owner, 17, global)
    }
}

/// Puts text on the clipboard as Unicode text.
pub fn copy_text(owner: HWND, text: &str) -> bool {
    let units: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    let bytes = units.len() * 2;
    // SAFETY: the allocation holds exactly the UTF-16 units copied into it.
    unsafe {
        let global = GlobalAlloc(GMEM_MOVEABLE, bytes);
        if global.is_null() {
            return false;
        }
        let data = GlobalLock(global).cast::<u16>();
        if data.is_null() {
            GlobalFree(global);
            return false;
        }
        std::ptr::copy_nonoverlapping(units.as_ptr(), data, units.len());
        GlobalUnlock(global);
        put_on_clipboard(owner, 13, global)
    }
}

/// Replaces the clipboard content with `global` in `format`. On success the
/// clipboard owns the memory; otherwise it is freed here.
unsafe fn put_on_clipboard(owner: HWND, format: u32, global: *mut std::ffi::c_void) -> bool {
    let mut owned = false;
    // Another application (a clipboard manager, for example) may hold it briefly.
    let mut opened = false;
    for attempt in 0..10 {
        opened = OpenClipboard(owner) != 0;
        if opened || attempt == 9 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    if opened {
        EmptyClipboard();
        owned = !SetClipboardData(format, global).is_null();
        CloseClipboard();
    }
    if !owned {
        GlobalFree(global);
    }
    owned
}

/// The screenshot folder from the settings, created when missing.
pub fn screenshot_folder() -> std::io::Result<PathBuf> {
    let folder = crate::config::screenshot_folder(&crate::config::load_settings());
    fs::create_dir_all(&folder)?;
    Ok(folder)
}

/// Opens a folder in Explorer, creating it first when it does not exist yet.
pub fn open_folder(folder: &Path) {
    let _ = fs::create_dir_all(folder);
    shell_open(&wide(folder), None);
}

/// Asks the shell to open `target` (a file, a folder or a program), with the given
/// parameters. The tests only note what would be opened, so no window appears on the desktop.
pub fn shell_open(target: &[u16], parameters: Option<&[u16]>) {
    #[cfg(test)]
    crate::test_support::shell_opened(target, parameters);
    #[cfg(not(test))]
    // SAFETY: all strings are valid, nul-terminated UTF-16.
    unsafe {
        windows_sys::Win32::UI::Shell::ShellExecuteW(
            null_mut(),
            w("open").as_ptr(),
            target.as_ptr(),
            parameters.map_or(null(), |p| p.as_ptr()),
            null(),
            windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
        );
    }
}

/// Lets the user pick a folder; `None` means the dialog was cancelled.
pub fn choose_folder(owner: HWND, current: &Path) -> Option<PathBuf> {
    use windows::{
        core::HSTRING,
        Win32::{
            System::Com::{
                CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_INPROC_SERVER,
                COINIT_APARTMENTTHREADED,
            },
            UI::Shell::{
                FileOpenDialog, IFileOpenDialog, IShellItem, SHCreateItemFromParsingName,
                FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, SIGDN_FILESYSPATH,
            },
        },
    };
    // SAFETY: plain COM calls; the path string is freed once after it is copied.
    unsafe {
        // The thread may already be initialized (possibly differently), which is fine.
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let dialog: IFileOpenDialog =
            CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        dialog
            .SetOptions(dialog.GetOptions().ok()? | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM)
            .ok()?;
        let _ = dialog.SetTitle(&HSTRING::from(tr("Choose a folder")));
        if let Ok(item) = SHCreateItemFromParsingName::<_, _, IShellItem>(
            &HSTRING::from(current.to_string_lossy().as_ref()),
            None,
        ) {
            let _ = dialog.SetFolder(&item);
        }
        dialog
            .Show(Some(windows::Win32::Foundation::HWND(owner)))
            .ok()?;
        let name = dialog
            .GetResult()
            .ok()?
            .GetDisplayName(SIGDN_FILESYSPATH)
            .ok()?;
        let path = name.to_string().ok().map(PathBuf::from);
        CoTaskMemFree(Some(name.0.cast()));
        path
    }
}

/// Shows the Save As dialog; returns the chosen path and the selected filter (1-based).
fn save_dialog(
    owner: HWND,
    folder: &Path,
    name: &str,
    filter: &str,
    extension: &str,
) -> Option<(PathBuf, u32)> {
    let mut file = [0u16; 520];
    let initial: Vec<u16> = name.encode_utf16().collect();
    file[..initial.len()].copy_from_slice(&initial);
    let directory = wide(folder);
    let filter: Vec<u16> = filter.encode_utf16().collect();
    let extension = w(extension);
    // SAFETY: every pointer in the structure outlives the call.
    unsafe {
        let mut dialog: OPENFILENAMEW = std::mem::zeroed();
        dialog.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
        dialog.hwndOwner = owner;
        dialog.lpstrFilter = filter.as_ptr();
        dialog.nFilterIndex = 1;
        dialog.lpstrFile = file.as_mut_ptr();
        dialog.nMaxFile = file.len() as u32;
        dialog.lpstrInitialDir = directory.as_ptr();
        dialog.lpstrDefExt = extension.as_ptr();
        dialog.Flags = OFN_EXPLORER | OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST;
        if GetSaveFileNameW(&mut dialog) == 0 {
            return None;
        }
        let length = file.iter().position(|&c| c == 0).unwrap_or(file.len());
        Some((
            PathBuf::from(OsString::from_wide(&file[..length])),
            dialog.nFilterIndex,
        ))
    }
}

/// Returns the path to save to; with `ask` a Save As dialog picks it and
/// `None` means the user cancelled.
pub fn choose_path(owner: HWND, folder: &Path, name: &str, ask: bool) -> Option<PathBuf> {
    if !ask {
        return Some(folder.join(name));
    }
    let filter = format!("{}\0*.png\0{}\0*.bmp\0\0", tr("PNG image"), tr("BMP image"));
    let (path, index) = save_dialog(owner, folder, name, &filter, "png")?;
    Some(default_extension(
        path,
        if index == 2 { "bmp" } else { "png" },
    ))
}

/// A name typed without an extension gets the one of the chosen file type.
fn default_extension(path: PathBuf, extension: &str) -> PathBuf {
    if path.extension().is_none() {
        path.with_extension(extension)
    } else {
        path
    }
}

/// Asks where to save a recording; `None` means the user cancelled.
pub fn choose_video_path(owner: HWND, folder: &Path, name: &str) -> Option<PathBuf> {
    let filter = format!("{}\0*.mp4\0\0", tr("MP4 video"));
    let (path, _) = save_dialog(owner, folder, name, &filter, "mp4")?;
    Some(default_extension(path, "mp4"))
}

/// Milliseconds since the Unix epoch, used for file names.
pub fn timestamp() -> std::io::Result<u128> {
    Ok(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(std::io::Error::other)?
        .as_millis())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::clipboard_text;
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, OpenClipboard,
    };

    fn scratch(name: &str) -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("snipping-lite-imaging-{}", std::process::id()));
        fs::create_dir_all(&folder).unwrap();
        folder.join(name)
    }

    /// A 3x2 image whose pixels all differ, including every alpha class.
    fn sample() -> Vec<u8> {
        vec![
            0, 0, 255, 255, 0, 255, 0, 255, 255, 0, 0, 255, // opaque red, green, blue
            10, 20, 30, 128, 40, 50, 60, 1, 255, 255, 255, 0, // partial, nearly clear, clear
        ]
    }

    #[test]
    fn png_files_keep_size_and_pixels() {
        let path = scratch("sample.png");
        encode(&path, &sample(), 3, 2).unwrap();
        let bytes = fs::read(&path).unwrap();
        // Golden: the PNG signature and the header that carries the size.
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(&bytes[12..16], b"IHDR");
        assert_eq!(u32::from_be_bytes(bytes[16..20].try_into().unwrap()), 3);
        assert_eq!(u32::from_be_bytes(bytes[20..24].try_into().unwrap()), 2);
        let (width, height, pixels) = decode(&path).unwrap();
        assert_eq!((width, height), (3, 2));
        let expected = sample();
        // Contract: opaque and translucent pixels are unchanged; a clear pixel stays clear.
        assert_eq!(pixels[..20], expected[..20]);
        assert_eq!(pixels[23], 0);
    }

    #[test]
    fn the_extension_picks_the_format_without_regard_to_case() {
        for (name, signature) in [
            ("a.bmp", &b"BM"[..]),
            ("b.BMP", &b"BM"[..]),
            ("c.png", &b"\x89PNG"[..]),
            ("d", &b"\x89PNG"[..]),
        ] {
            let path = scratch(name);
            encode(&path, &sample(), 3, 2).unwrap();
            assert!(fs::read(&path).unwrap().starts_with(signature), "{name}");
        }
        // A bitmap has no alpha channel: decoding gives opaque pixels of the same size.
        let (width, height, pixels) = decode(&scratch("a.bmp")).unwrap();
        assert_eq!((width, height, pixels.len()), (3, 2, 24));
    }

    #[test]
    fn encoding_and_decoding_report_failures() {
        // Boundary: an image without pixels cannot be created.
        assert!(encode(&scratch("empty.png"), &[], 0, 0).is_err());
        assert!(encode(&scratch("empty.png"), &[], 0, 3).is_err());
        // The folder does not exist.
        let missing = std::env::temp_dir()
            .join("snipping-lite-missing-folder")
            .join("a.png");
        assert!(encode(&missing, &sample(), 3, 2).is_err());
        assert!(decode(&missing).is_none());
        // A file that is not an image.
        let text = scratch("not-an-image.png");
        fs::write(&text, b"hello").unwrap();
        assert!(decode(&text).is_none());
        assert!(decode(&scratch("zero.png")).is_none());
    }

    #[test]
    fn a_name_without_an_extension_gets_the_chosen_one() {
        let bare = PathBuf::from("D:\\Shots\\shot");
        assert_eq!(
            default_extension(bare.clone(), "png"),
            PathBuf::from("D:\\Shots\\shot.png")
        );
        assert_eq!(
            default_extension(bare, "mp4"),
            PathBuf::from("D:\\Shots\\shot.mp4")
        );
        let named = PathBuf::from("D:\\Shots\\shot.bmp");
        assert_eq!(default_extension(named.clone(), "png"), named);
    }

    #[test]
    fn without_asking_the_path_is_the_folder_and_name() {
        let folder = Path::new("D:\\Shots");
        assert_eq!(
            choose_path(null_mut(), folder, "a.png", false),
            Some(PathBuf::from("D:\\Shots\\a.png"))
        );
    }

    #[test]
    fn timestamps_never_go_backwards() {
        let first = timestamp().unwrap();
        let second = timestamp().unwrap();
        assert!(first > 1_700_000_000_000 && second >= first);
    }

    #[test]
    fn the_screenshot_folder_is_created_from_the_settings() {
        let sandbox = crate::test_support::Sandbox::new();
        let chosen = sandbox.path().join("my shots").join("nested");
        let settings = crate::config::Settings {
            screenshot_folder: chosen.to_string_lossy().into_owned(),
            ..crate::config::parse_settings("")
        };
        crate::config::save_settings(&settings).unwrap();
        assert_eq!(screenshot_folder().unwrap(), chosen);
        assert!(chosen.is_dir());
        // A file in the way makes the folder impossible to create.
        let blocked = sandbox.path().join("blocked");
        fs::write(&blocked, b"x").unwrap();
        let settings = crate::config::Settings {
            screenshot_folder: blocked.join("inside").to_string_lossy().into_owned(),
            ..settings
        };
        crate::config::save_settings(&settings).unwrap();
        assert!(screenshot_folder().is_err());
    }

    fn clipboard_image() -> Option<(i32, i32, Vec<u8>)> {
        // SAFETY: the clipboard is opened and closed here, and the data is copied out while locked.
        unsafe {
            if OpenClipboard(null_mut()) == 0 {
                return None;
            }
            let global = GetClipboardData(17);
            let image = (!global.is_null()).then(|| {
                let data = GlobalLock(global).cast::<u8>();
                let header = std::ptr::read_unaligned(data.cast::<BITMAPV5HEADER>());
                let size = header.bV5SizeImage as usize;
                let pixels =
                    std::slice::from_raw_parts(data.add(header.bV5Size as usize), size).to_vec();
                GlobalUnlock(global);
                (header.bV5Width, header.bV5Height, pixels)
            });
            CloseClipboard();
            image
        }
    }

    #[test]
    #[ignore = "Replaces the clipboard content"]
    fn a_copied_image_reads_back_bottom_up() {
        assert!(copy_to_clipboard(null_mut(), &sample(), 3, 2));
        let (width, height, pixels) = clipboard_image().unwrap();
        assert_eq!((width, height), (3, 2));
        // Contract: the clipboard holds rows bottom-up, so the second row comes first.
        let expected = sample();
        assert_eq!(pixels[..12], expected[12..]);
        assert_eq!(pixels[12..], expected[..12]);
    }

    use crate::test_support::answer_dialog;

    #[test]
    #[ignore = "Shows dialogs"]
    fn the_save_dialogs_return_the_chosen_path_or_nothing() {
        let folder = scratch("dialogs");
        fs::create_dir_all(&folder).unwrap();
        // Cancelling gives nothing.
        let answer = answer_dialog(2);
        assert_eq!(choose_path(null_mut(), &folder, "a.png", true), None);
        assert!(answer.join().unwrap());
        let answer = answer_dialog(2);
        assert_eq!(choose_video_path(null_mut(), &folder, "a.mp4"), None);
        assert!(answer.join().unwrap());
        // Accepting keeps the proposed name; a name without an extension gets the default one.
        let answer = answer_dialog(1);
        assert_eq!(
            choose_path(null_mut(), &folder, "a.png", true),
            Some(folder.join("a.png"))
        );
        assert!(answer.join().unwrap());
        let answer = answer_dialog(1);
        assert_eq!(
            choose_path(null_mut(), &folder, "b", true),
            Some(folder.join("b.png"))
        );
        assert!(answer.join().unwrap());
        let answer = answer_dialog(1);
        assert_eq!(
            choose_video_path(null_mut(), &folder, "c"),
            Some(folder.join("c.mp4"))
        );
        assert!(answer.join().unwrap());
    }

    #[test]
    #[ignore = "Shows a dialog"]
    fn the_folder_dialog_returns_the_current_folder_or_nothing() {
        let folder = scratch("folders");
        fs::create_dir_all(&folder).unwrap();
        let answer = answer_dialog(2);
        assert_eq!(choose_folder(null_mut(), &folder), None);
        assert!(answer.join().unwrap());
        let answer = answer_dialog(1);
        assert_eq!(choose_folder(null_mut(), &folder), Some(folder.clone()));
        assert!(answer.join().unwrap());
        // A folder that does not exist still opens the dialog.
        let answer = answer_dialog(2);
        assert_eq!(choose_folder(null_mut(), &folder.join("missing")), None);
        assert!(answer.join().unwrap());
    }

    #[test]
    fn opening_a_folder_creates_it_and_asks_the_shell_to_open_it() {
        let folder = scratch("opened");
        let _ = fs::remove_dir_all(&folder);
        crate::test_support::take_shell_opened();
        open_folder(&folder);
        assert!(folder.is_dir());
        assert_eq!(
            crate::test_support::take_shell_opened(),
            vec![(folder.to_string_lossy().into_owned(), None)]
        );
    }

    #[test]
    #[ignore = "Replaces the clipboard content"]
    fn copied_text_reads_back_unchanged() {
        for text in [
            "#E81123",
            "",
            "日本語 text\r\nline 2",
            "emoji 😀 and \u{3042}",
            &"x".repeat(100_000),
        ] {
            assert!(copy_text(null_mut(), text), "{text:.20}");
            assert_eq!(clipboard_text().as_deref(), Some(text));
        }
    }
}
