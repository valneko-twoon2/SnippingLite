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
            owned = !SetClipboardData(17, global).is_null();
            CloseClipboard();
        }
        if !owned {
            GlobalFree(global);
        }
        owned
    }
}

/// The screenshot folder from the settings, created when missing.
pub fn screenshot_folder() -> std::io::Result<PathBuf> {
    let folder = crate::config::screenshot_folder(&crate::config::load_settings());
    fs::create_dir_all(&folder)?;
    Ok(folder)
}

/// Opens a folder in Explorer, creating it first when it does not exist yet.
pub fn open_folder(folder: &Path) {
    use windows_sys::Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL};
    let _ = fs::create_dir_all(folder);
    // SAFETY: all strings are valid, nul-terminated UTF-16.
    unsafe {
        ShellExecuteW(
            null_mut(),
            w("open").as_ptr(),
            wide(folder).as_ptr(),
            null(),
            null(),
            SW_SHOWNORMAL,
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
    Some(if path.extension().is_none() {
        path.with_extension(if index == 2 { "bmp" } else { "png" })
    } else {
        path
    })
}

/// Asks where to save a recording; `None` means the user cancelled.
pub fn choose_video_path(owner: HWND, folder: &Path, name: &str) -> Option<PathBuf> {
    let filter = format!("{}\0*.mp4\0\0", tr("MP4 video"));
    let (path, _) = save_dialog(owner, folder, name, &filter, "mp4")?;
    Some(if path.extension().is_none() {
        path.with_extension("mp4")
    } else {
        path
    })
}

/// Milliseconds since the Unix epoch, used for file names.
pub fn timestamp() -> std::io::Result<u128> {
    Ok(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(std::io::Error::other)?
        .as_millis())
}
