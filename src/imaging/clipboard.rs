//! クリップボードに画像、文字、ファイルを置く。

use std::{ffi::c_void, os::windows::ffi::OsStrExt, path::Path, ptr::null_mut};
use windows_sys::Win32::{
    Foundation::{GlobalFree, HWND},
    Graphics::Gdi::{
        CreateDIBitmap, DeleteObject, GetDC, ReleaseDC, BITMAPINFO, BITMAPINFOHEADER,
        BITMAPV5HEADER, BI_BITFIELDS, BI_RGB, CBM_INIT, DIB_RGB_COLORS,
    },
    System::{
        DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData},
        Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE},
    },
};

use super::{fits, on_white};

/// 標準のクリップボード形式（winuser.h）。
const CF_BITMAP: u32 = 2;
const CF_DIB: u32 = 8;
const CF_UNICODETEXT: u32 = 13;
const CF_HDROP: u32 = 15;
const CF_DIBV5: u32 = 17;

/// 画像を、透過付きと透過なしの形式でクリップボードに置く。
///
/// - CF_DIBV5: 透過付きの 32 ビット。透過を読むアプリはこちらを読む。
/// - CF_DIB と CF_BITMAP: 白の上に重ねた（BMP の保存と同じ規則の）不透明な画像。透過を読まない
///   アプリ（ペイントや古いアプリ）が一般に読むのはこの 2 つで、ほぼすべてのアプリが対応する。
///
/// CF_DIBV5 だけを置くと、Windows は CF_DIB と CF_BITMAP を自動で作るが、色をそのまま使って
/// アルファを捨てるので、半透明の画素が濃い色に見えていた。CF_DIB を明示して置けば、Windows は
/// CF_DIB と CF_DIBV5 の間では変換せず、それぞれ置いたとおりを返す。ただし CF_BITMAP は、CF_DIB を
/// 先に置いても後に置いても CF_DIBV5 から作られた（テストで確かめている）ので、これも明示して置く。
///
/// 登録形式の "PNG" を足さなかったのは、"PNG" を読むアプリは透過も読むので透過なしの代わりに
/// ならず、CF_DIBV5 で足りるため（PNG への符号化の手間も増える）。
pub fn copy_to_clipboard(owner: HWND, pixels: &[u8], width: i32, height: i32) -> bool {
    if !fits(pixels, width, height) {
        return false;
    }
    // SAFETY: all zeroes is a valid value for these plain Win32 headers.
    let (mut v5, mut plain): (BITMAPV5HEADER, BITMAPINFOHEADER) =
        unsafe { (std::mem::zeroed(), std::mem::zeroed()) };
    v5.bV5Size = std::mem::size_of::<BITMAPV5HEADER>() as u32;
    v5.bV5Width = width;
    v5.bV5Height = height;
    v5.bV5Planes = 1;
    v5.bV5BitCount = 32;
    v5.bV5Compression = BI_BITFIELDS;
    v5.bV5SizeImage = pixels.len() as u32;
    v5.bV5RedMask = 0x00ff0000;
    v5.bV5GreenMask = 0x0000ff00;
    v5.bV5BlueMask = 0x000000ff;
    v5.bV5AlphaMask = 0xff000000;
    // LCS_sRGB, LCS_GM_IMAGES
    v5.bV5CSType = 0x73524742;
    v5.bV5Intent = 4;
    plain.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    plain.biWidth = width;
    plain.biHeight = height;
    plain.biPlanes = 1;
    plain.biBitCount = 32;
    plain.biCompression = BI_RGB;
    plain.biSizeImage = pixels.len() as u32;
    let row = width as usize * 4;
    // SAFETY: the headers have no padding bytes, and the slices do not outlive them.
    let (v5_bytes, plain_bytes) = unsafe { (bytes_of(&v5), bytes_of(&plain)) };
    let Some(dib) = dib_global(plain_bytes, &on_white(pixels), row) else {
        return false;
    };
    let Some(dibv5) = dib_global(v5_bytes, pixels, row) else {
        // SAFETY: the memory was allocated above and is not on the clipboard.
        unsafe { GlobalFree(dib) };
        return false;
    };
    let mut items = vec![(CF_DIBV5, dibv5), (CF_DIB, dib)];
    // 大きすぎて CF_BITMAP を作れないときは、Windows が作るもの（アルファを捨てたもの）に任せる。
    if let Some(bitmap) = bitmap_from(dib) {
        items.push((CF_BITMAP, bitmap));
    }
    // SAFETY: every handle holds a complete image and is owned here until put on the clipboard.
    unsafe { put_on_clipboard(owner, &items) }
}

/// 構造体のバイト列。
///
/// # Safety
/// `T` に詰め物のバイトがないこと。
unsafe fn bytes_of<T: Copy>(value: &T) -> &[u8] {
    std::slice::from_raw_parts((value as *const T).cast::<u8>(), std::mem::size_of::<T>())
}

/// `header` の後ろに、上から下の順の `pixels` を下から上の順（クリップボードの DIB の向き）に
/// 並べたメモリー。確保できなければ `None`。
fn dib_global(header: &[u8], pixels: &[u8], row: usize) -> Option<*mut c_void> {
    // SAFETY: the allocation is sized for the header and all pixel rows.
    unsafe {
        let global = GlobalAlloc(GMEM_MOVEABLE, header.len() + pixels.len());
        if global.is_null() {
            return None;
        }
        let data = GlobalLock(global).cast::<u8>();
        if data.is_null() {
            GlobalFree(global);
            return None;
        }
        std::ptr::copy_nonoverlapping(header.as_ptr(), data, header.len());
        for (i, line) in pixels.chunks_exact(row).rev().enumerate() {
            std::ptr::copy_nonoverlapping(line.as_ptr(), data.add(header.len() + i * row), row);
        }
        GlobalUnlock(global);
        Some(global)
    }
}

/// BITMAPINFOHEADER の後ろに画素が続く `dib` と同じ画像の、画面と互換のビットマップ（DDB）。
fn bitmap_from(dib: *mut c_void) -> Option<*mut c_void> {
    // SAFETY: `dib` holds a BITMAPINFOHEADER (BI_RGB, so without a color table) followed by its
    // pixels, and it stays locked while GDI reads it.
    unsafe {
        let data = GlobalLock(dib).cast::<u8>();
        if data.is_null() {
            return None;
        }
        let header = data.cast::<BITMAPINFOHEADER>();
        let bits = data.add(std::mem::size_of::<BITMAPINFOHEADER>());
        let screen = GetDC(null_mut());
        let bitmap = if screen.is_null() {
            null_mut()
        } else {
            let bitmap = CreateDIBitmap(
                screen,
                header,
                CBM_INIT as u32,
                bits.cast(),
                header.cast::<BITMAPINFO>(),
                DIB_RGB_COLORS,
            );
            ReleaseDC(null_mut(), screen);
            bitmap
        };
        GlobalUnlock(dib);
        (!bitmap.is_null()).then_some(bitmap)
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
        put_on_clipboard(owner, &[(CF_UNICODETEXT, global)])
    }
}

/// ファイルをクリップボードに置く（CF_HDROP）。エクスプローラーやチャットに貼り付けると、
/// エクスプローラーでコピーしたときと同じようにファイルが渡る。
pub fn copy_file(owner: HWND, path: &Path) -> bool {
    // DROPFILES の後ろにパスを並べ、空の文字列で終える。
    const HEADER: usize = 20;
    let mut list: Vec<u16> = path.as_os_str().encode_wide().collect();
    list.extend([0, 0]);
    let bytes = HEADER + list.len() * 2;
    // SAFETY: the allocation holds the header and every UTF-16 unit copied into it.
    unsafe {
        let global = GlobalAlloc(GMEM_MOVEABLE, bytes);
        if global.is_null() {
            return false;
        }
        let data = GlobalLock(global).cast::<u8>();
        if data.is_null() {
            GlobalFree(global);
            return false;
        }
        std::ptr::write_bytes(data, 0, HEADER);
        // pFiles: パスの始まるオフセット。fWide: パスは UTF-16。
        std::ptr::copy_nonoverlapping((HEADER as u32).to_le_bytes().as_ptr(), data, 4);
        std::ptr::copy_nonoverlapping(1u32.to_le_bytes().as_ptr(), data.add(16), 4);
        std::ptr::copy_nonoverlapping(list.as_ptr().cast::<u8>(), data.add(HEADER), list.len() * 2);
        GlobalUnlock(global);
        put_on_clipboard(owner, &[(CF_HDROP, global)])
    }
}

/// クリップボードの中身を、`items` の各形式のハンドルに置き換える（置く順は `items` の順）。
/// クリップボードが受け取ったハンドルはクリップボードのものになり、受け取らなかったものはここで
/// 解放する（CF_BITMAP は GDI のオブジェクト、ほかはグローバルメモリー）。すべて置けたときだけ `true`。
unsafe fn put_on_clipboard(owner: HWND, items: &[(u32, *mut c_void)]) -> bool {
    let mut owned = vec![false; items.len()];
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
        for (taken, &(format, handle)) in owned.iter_mut().zip(items) {
            *taken = !SetClipboardData(format, handle).is_null();
        }
        CloseClipboard();
    }
    for (&taken, &(format, handle)) in owned.iter().zip(items) {
        if !taken {
            if format == CF_BITMAP {
                DeleteObject(handle);
            } else {
                GlobalFree(handle);
            }
        }
    }
    owned.iter().all(|&taken| taken)
}
