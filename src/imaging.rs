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
        Foundation::HWND,
        Graphics::GdiPlus::{
            BitmapData, GdipBitmapLockBits, GdipBitmapUnlockBits, GdipCreateBitmapFromFile,
            GdipCreateBitmapFromScan0, GdipDisposeImage, GdipGetImageHeight, GdipGetImageWidth,
            GdipSaveImageToFile, GdiplusStartup, GdiplusStartupInput, GdiplusStartupOutput, Rect,
        },
        UI::Controls::Dialogs::{
            GetSaveFileNameW, OFN_EXPLORER, OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST, OPENFILENAMEW,
        },
    },
};

use crate::i18n::tr;
use crate::{staging, w};

mod clipboard;
pub use clipboard::{copy_file, copy_text, copy_to_clipboard};

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

/// Whether `pixels` holds exactly the 4 bytes of each pixel of a `width` x `height` image.
pub fn fits(pixels: &[u8], width: i32, height: i32) -> bool {
    width > 0
        && height > 0
        && (width as usize)
            .checked_mul(height as usize)
            .and_then(|n| n.checked_mul(4))
            == Some(pixels.len())
}

/// The error for pixels that do not match the stated size.
pub fn bad_size() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        "The pixels do not match the image size",
    )
}

/// Encodes straight-alpha top-down BGRA pixels; the extension picks PNG or BMP.
/// 一時名で書き終えてから `path` に名前を変えるので、途中で止まっても `path` に書きかけは残らない。
pub fn encode(path: &Path, pixels: &[u8], width: i32, height: i32) -> std::io::Result<()> {
    let partial = encode_partial(path, pixels, width, height)?;
    staging::step("picture written", path);
    staging::commit(&partial, path)
}

/// `path` の画像を一時名（`staging::partial`）で書き、その名前を返す。`staging::commit` で置く。
pub fn encode_partial(
    path: &Path,
    pixels: &[u8],
    width: i32,
    height: i32,
) -> std::io::Result<PathBuf> {
    if !fits(pixels, width, height) {
        return Err(bad_size());
    }
    if !start_gdiplus() {
        return Err(std::io::Error::other("Could not start GDI+"));
    }
    let partial = staging::partial(path);
    let bmp = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("bmp"));
    match save(&partial, pixels, width, height, bmp) {
        Ok(()) => Ok(partial),
        Err(error) => {
            let _ = fs::remove_file(&partial);
            Err(error)
        }
    }
}

/// GDI+ で `path` に書く。種類は名前でなく `bmp` で選ぶ（一時名には `.png` も `.bmp` も付かない）。
fn save(path: &Path, pixels: &[u8], width: i32, height: i32, bmp: bool) -> std::io::Result<()> {
    let encoder = GUID {
        data1: if bmp { 0x557cf400 } else { 0x557cf406 },
        data2: 0x1a04,
        data3: 0x11d3,
        data4: [0x9a, 0x73, 0x00, 0x00, 0xf8, 0x1e, 0xf3, 0x2e],
    };
    // BMP には透明がなく、GDI+ はアルファを捨てて色だけを書く。README のとおり透明な部分を白にし、
    // 半透明の画素（自由形式の外側に描いた線）も、PNG を白い背景で見たときと同じ色にする。
    let flattened;
    let pixels = if bmp {
        flattened = on_white(pixels);
        &flattened
    } else {
        pixels
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

/// 白の上に重ねた、不透明な画素。`c * a + 255 * (255 - a)` を 255 で割って四捨五入する
/// （255 は奇数なので、ちょうど半分にはならない）。
fn on_white(pixels: &[u8]) -> Vec<u8> {
    let mut opaque = pixels.to_vec();
    for pixel in opaque.as_chunks_mut::<4>().0 {
        let alpha = pixel[3] as u32;
        for channel in &mut pixel[..3] {
            *channel = ((*channel as u32 * alpha + 255 * (255 - alpha) + 127) / 255) as u8;
        }
        pixel[3] = 255;
    }
    opaque
}

/// ファイル 1 バイトあたりの画素の上限。PNG の deflate は 1 バイトを最大 1032 バイトに広げ、PNG の
/// 画素は最小 1 ビットなので、どの PNG もこれを超えない（GIF の LZW や JPEG も下回る）。超えるのは
/// 中身のない大きさの宣言だけで、信じて読むと、GDI+ が足りない行を埋めて、数十バイトのファイルから
/// 何 GB もの画素を返す。
const PIXELS_PER_BYTE: u64 = 1032 * 8;

/// Decodes an image file into straight-alpha top-down BGRA pixels.
pub fn decode(path: &Path) -> Option<(i32, i32, Vec<u8>)> {
    if !start_gdiplus() {
        return None;
    }
    let length = fs::metadata(path).map_or(0, |m| m.len());
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
        let declared = width as u64 * height as u64;
        // ImageLockModeRead, PixelFormat32bppARGB
        if width > 0
            && height > 0
            && i32::try_from(width.max(height)).is_ok()
            && declared <= length.saturating_mul(PIXELS_PER_BYTE)
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

/// The screenshot folder from the settings, created when missing.
/// 前に止まったプロセスが残した書きかけの画像は、ここで消す。
pub fn screenshot_folder() -> std::io::Result<PathBuf> {
    let folder = crate::config::screenshot_folder(&crate::config::load_settings());
    fs::create_dir_all(&folder)?;
    staging::remove_leftovers(&folder);
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

/// Milliseconds since the Unix epoch, used for file names.
pub fn timestamp() -> std::io::Result<u128> {
    Ok(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(std::io::Error::other)?
        .as_millis())
}

#[cfg(test)]
mod tests;
