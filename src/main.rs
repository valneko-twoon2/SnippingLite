#![windows_subsystem = "windows"]

use std::{
    ffi::OsString,
    fs,
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::PathBuf,
    ptr::{null, null_mut},
};
use windows_sys::{
    core::GUID,
    Win32::{
        Foundation::*,
        Graphics::{
            Dwm::DwmFlush,
            Gdi::*,
            GdiPlus::{
                GdipCreateBitmapFromScan0, GdipDisposeImage, GdipSaveImageToFile, GdiplusShutdown,
                GdiplusStartup, GdiplusStartupInput, GdiplusStartupOutput,
            },
        },
        System::{DataExchange::*, LibraryLoader::GetModuleHandleW, Memory::*},
        UI::{
            Controls::Dialogs::{
                GetSaveFileNameW, OFN_EXPLORER, OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST,
                OPENFILENAMEW,
            },
            HiDpi::*,
            Input::KeyboardAndMouse::*,
            Shell::*,
            WindowsAndMessaging::*,
        },
    },
};

const MAIN: &str = "SnippingLiteMain";
const OVERLAY: &str = "SnippingLiteOverlay";
const NEW: usize = 100;
const COPY: usize = 101;
const SAVE: usize = 102;
const DELAY: usize = 103;
const ASK: usize = 104;
const DELAYS: [u32; 4] = [0, 3, 5, 10];
const DELAY_TIMER: usize = 1;
const HOTKEY: i32 = 1;
const TRAY_MESSAGE: u32 = WM_APP + 1;
const TRAY_SETTINGS: usize = 201;
const TRAY_EXIT: usize = 202;
const MODE_RECT: i32 = 0;
const MODE_FREE: i32 = 1;
const MODE_WINDOW: i32 = 2;
const MODE_SCREEN: i32 = 3;

struct State {
    owner: HWND,
    overlay: HWND,
    screen: HBITMAP,
    screen_dc: HDC,
    previous: HGDIOBJ,
    origin: POINT,
    width: i32,
    height: i32,
    mode: i32,
    start: POINT,
    end: POINT,
    points: Vec<POINT>,
    dragging: bool,
    return_visible: bool,
    copy: bool,
    save: bool,
    ask: bool,
    delay: u32,
    pending: bool,
}
static mut STATE: *mut State = null_mut();
fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
unsafe fn state() -> &'static mut State {
    &mut *STATE
}
unsafe fn message(hwnd: HWND, text: &str) {
    MessageBoxW(
        hwnd,
        w(text).as_ptr(),
        w("Snipping Lite").as_ptr(),
        MB_OK | MB_ICONINFORMATION,
    );
}
fn settings_path() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|x| PathBuf::from(x).join("Snipping Lite").join("settings.ini"))
}
fn parse_settings(text: &str) -> (bool, bool, bool, u32) {
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
fn load_settings() -> (bool, bool, bool, u32) {
    parse_settings(
        &settings_path()
            .and_then(|p| fs::read_to_string(p).ok())
            .unwrap_or_default(),
    )
}
fn save_settings(copy: bool, save: bool, ask: bool, delay: u32) -> std::io::Result<()> {
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
unsafe fn button(
    parent: HWND,
    text: &str,
    id: usize,
    x: i32,
    y: i32,
    width: i32,
    style: u32,
) -> HWND {
    CreateWindowExW(
        0,
        w("BUTTON").as_ptr(),
        w(text).as_ptr(),
        WS_CHILD | WS_VISIBLE | style,
        x,
        y,
        width,
        30,
        parent,
        id as HMENU,
        GetModuleHandleW(null()),
        null(),
    )
}
unsafe fn text(parent: HWND, value: &str, x: i32, y: i32, width: i32) {
    CreateWindowExW(
        0,
        w("STATIC").as_ptr(),
        w(value).as_ptr(),
        WS_CHILD | WS_VISIBLE,
        x,
        y,
        width,
        36,
        parent,
        null_mut(),
        GetModuleHandleW(null()),
        null(),
    );
}
unsafe fn begin_capture() {
    if !state().overlay.is_null() || state().pending {
        return;
    }
    let owner = state().owner;
    state().return_visible = IsWindowVisible(owner) != 0;
    ShowWindow(owner, SW_HIDE);
    if state().delay > 0 && SetTimer(owner, DELAY_TIMER, state().delay * 1000, None) != 0 {
        state().pending = true;
        return;
    }
    capture();
}
unsafe fn capture() {
    let owner = state().owner;
    DwmFlush();
    let x = GetSystemMetrics(SM_XVIRTUALSCREEN);
    let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
    let width = GetSystemMetrics(SM_CXVIRTUALSCREEN);
    let height = GetSystemMetrics(SM_CYVIRTUALSCREEN);
    let dc = GetDC(null_mut());
    let mem = CreateCompatibleDC(dc);
    let bitmap = CreateCompatibleBitmap(dc, width, height);
    if dc.is_null() || mem.is_null() || bitmap.is_null() {
        if !bitmap.is_null() {
            DeleteObject(bitmap);
        }
        if !mem.is_null() {
            DeleteDC(mem);
        }
        if !dc.is_null() {
            ReleaseDC(null_mut(), dc);
        }
        ShowWindow(owner, SW_SHOW);
        message(owner, "Could not capture the desktop.");
        return;
    }
    let old = SelectObject(mem, bitmap);
    let captured = BitBlt(mem, 0, 0, width, height, dc, x, y, SRCCOPY | CAPTUREBLT) != 0;
    ReleaseDC(null_mut(), dc);
    if !captured {
        SelectObject(mem, old);
        DeleteDC(mem);
        DeleteObject(bitmap);
        ShowWindow(owner, SW_SHOW);
        message(owner, "Could not capture the desktop.");
        return;
    }
    state().origin = POINT { x, y };
    state().width = width;
    state().height = height;
    state().screen = bitmap;
    state().screen_dc = mem;
    state().previous = old;
    state().mode = MODE_RECT;
    state().points.clear();
    let overlay = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_LAYERED | WS_EX_TOOLWINDOW,
        w(OVERLAY).as_ptr(),
        w("Snipping Lite").as_ptr(),
        WS_POPUP,
        x,
        y,
        width,
        height,
        null_mut(),
        null_mut(),
        GetModuleHandleW(null()),
        null(),
    );
    if overlay.is_null() {
        cleanup();
        ShowWindow(owner, SW_SHOW);
        return;
    }
    state().overlay = overlay;
    SetLayeredWindowAttributes(overlay, 0x00ff00ff, 175, LWA_ALPHA | LWA_COLORKEY);
    ShowWindow(overlay, SW_SHOW);
    SetForegroundWindow(overlay);
    SetFocus(overlay);
}
unsafe fn cleanup() {
    if !state().screen_dc.is_null() {
        SelectObject(state().screen_dc, state().previous);
        DeleteDC(state().screen_dc);
        state().screen_dc = null_mut();
    }
    if !state().screen.is_null() {
        DeleteObject(state().screen);
        state().screen = null_mut();
    }
    state().overlay = null_mut();
    state().dragging = false;
    state().points.clear();
}
fn point(lp: LPARAM) -> POINT {
    POINT {
        x: (lp as u32 as u16 as i16) as i32,
        y: ((lp as u32 >> 16) as u16 as i16) as i32,
    }
}
fn bounds(a: POINT, b: POINT, width: i32, height: i32) -> RECT {
    RECT {
        left: a.x.min(b.x).clamp(0, width),
        top: a.y.min(b.y).clamp(0, height),
        right: a.x.max(b.x).clamp(0, width),
        bottom: a.y.max(b.y).clamp(0, height),
    }
}
unsafe fn toolbar() -> (i32, i32) {
    (
        (GetSystemMetrics(SM_CXSCREEN) - 520) / 2 - state().origin.x,
        30 - state().origin.y,
    )
}
unsafe fn select_window(p: POINT) -> Option<RECT> {
    let absolute = POINT {
        x: p.x + state().origin.x,
        y: p.y + state().origin.y,
    };
    let mut window = GetWindow(state().overlay, GW_HWNDNEXT);
    while !window.is_null() {
        let mut rect: RECT = std::mem::zeroed();
        if window != state().owner
            && IsWindowVisible(window) != 0
            && IsIconic(window) == 0
            && GetWindowRect(window, &mut rect) != 0
            && absolute.x >= rect.left
            && absolute.x < rect.right
            && absolute.y >= rect.top
            && absolute.y < rect.bottom
        {
            return Some(RECT {
                left: (rect.left - state().origin.x).max(0),
                top: (rect.top - state().origin.y).max(0),
                right: (rect.right - state().origin.x).min(state().width),
                bottom: (rect.bottom - state().origin.y).min(state().height),
            });
        }
        window = GetWindow(window, GW_HWNDNEXT);
    }
    None
}
unsafe fn read_pixels(dc: HDC, bitmap: HBITMAP, width: i32, height: i32) -> Option<Vec<u8>> {
    let len = (width as usize)
        .checked_mul(4)?
        .checked_mul(height as usize)?;
    let mut pixels = vec![0u8; len];
    let mut info: BITMAPINFO = std::mem::zeroed();
    info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    info.bmiHeader.biWidth = width;
    info.bmiHeader.biHeight = -height;
    info.bmiHeader.biPlanes = 1;
    info.bmiHeader.biBitCount = 32;
    info.bmiHeader.biCompression = BI_RGB;
    let ok = GetDIBits(
        dc,
        bitmap,
        0,
        height as u32,
        pixels.as_mut_ptr().cast(),
        &mut info,
        DIB_RGB_COLORS,
    );
    (ok != 0).then_some(pixels)
}
unsafe fn apply_freeform_mask(
    dc: HDC,
    pixels: &mut [u8],
    points: &[POINT],
    width: i32,
    height: i32,
) -> bool {
    let screen = GetDC(null_mut());
    let mask = CreateCompatibleBitmap(screen, width, height);
    ReleaseDC(null_mut(), screen);
    if mask.is_null() {
        return false;
    }
    let old = SelectObject(dc, mask);
    PatBlt(dc, 0, 0, width, height, BLACKNESS);
    let region = CreatePolygonRgn(points.as_ptr(), points.len() as i32, ALTERNATE);
    FillRgn(dc, region, GetStockObject(WHITE_BRUSH) as HBRUSH);
    DeleteObject(region);
    SelectObject(dc, old);
    let covered = read_pixels(dc, mask, width, height);
    DeleteObject(mask);
    let Some(covered) = covered else {
        return false;
    };
    for (pixel, inside) in pixels
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(covered.as_chunks::<4>().0)
    {
        *pixel = if inside[0] | inside[1] | inside[2] != 0 {
            [pixel[0], pixel[1], pixel[2], 255]
        } else {
            [255, 255, 255, 0]
        };
    }
    true
}
fn screenshot_folder() -> std::io::Result<PathBuf> {
    let folder = std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .ok_or_else(|| std::io::Error::other("USERPROFILE is unavailable"))?
        .join("Pictures")
        .join("Screenshots");
    fs::create_dir_all(&folder)?;
    Ok(folder)
}
unsafe fn choose_path(folder: &std::path::Path, name: &str, ask: bool) -> Option<PathBuf> {
    if !ask {
        return Some(folder.join(name));
    }
    let mut file = [0u16; 520];
    let initial: Vec<u16> = name.encode_utf16().collect();
    file[..initial.len()].copy_from_slice(&initial);
    let directory: Vec<u16> = folder.as_os_str().encode_wide().chain(Some(0)).collect();
    let filter: Vec<u16> = "PNG image\0*.png\0BMP image\0*.bmp\0\0"
        .encode_utf16()
        .collect();
    let extension = w("png");
    let mut dialog: OPENFILENAMEW = std::mem::zeroed();
    dialog.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
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
    let path = PathBuf::from(OsString::from_wide(&file[..length]));
    Some(if path.extension().is_none() {
        path.with_extension(if dialog.nFilterIndex == 2 {
            "bmp"
        } else {
            "png"
        })
    } else {
        path
    })
}
unsafe fn save_image(pixels: &[u8], width: i32, height: i32, ask: bool) -> std::io::Result<()> {
    let folder = screenshot_folder()?;
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(std::io::Error::other)?
        .as_millis();
    let Some(path) = choose_path(&folder, &format!("Snipping Lite {timestamp}.png"), ask) else {
        return Ok(());
    };
    let bmp = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("bmp"));
    let encoder = GUID {
        data1: if bmp { 0x557cf400 } else { 0x557cf406 },
        data2: 0x1a04,
        data3: 0x11d3,
        data4: [0x9a, 0x73, 0x00, 0x00, 0xf8, 0x1e, 0xf3, 0x2e],
    };
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
    let name: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let status = GdipSaveImageToFile(image.cast(), name.as_ptr(), &encoder, null());
    GdipDisposeImage(image.cast());
    if status == 0 {
        Ok(())
    } else {
        Err(std::io::Error::other("Could not encode the image"))
    }
}
unsafe fn copy_image(pixels: &[u8], width: i32, height: i32) -> bool {
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
    // Clipboard DIBs are bottom-up while the captured rows are top-down.
    let row = width as usize * 4;
    for (i, line) in pixels.chunks_exact(row).rev().enumerate() {
        std::ptr::copy_nonoverlapping(line.as_ptr(), data.add(header + i * row), row);
    }
    GlobalUnlock(global);
    let mut owned = false;
    if OpenClipboard(state().owner) != 0 {
        EmptyClipboard();
        owned = !SetClipboardData(17, global).is_null();
        CloseClipboard();
    }
    if !owned {
        GlobalFree(global);
    }
    owned
}
unsafe fn finish(rect: RECT, freeform: bool) {
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    if width < 1 || height < 1 {
        return;
    }
    let dc = GetDC(null_mut());
    let mem = CreateCompatibleDC(dc);
    let bitmap = CreateCompatibleBitmap(dc, width, height);
    ReleaseDC(null_mut(), dc);
    if mem.is_null() || bitmap.is_null() {
        if !mem.is_null() {
            DeleteDC(mem);
        }
        if !bitmap.is_null() {
            DeleteObject(bitmap);
        }
        cancel();
        message(state().owner, "Could not create the capture.");
        return;
    }
    let old = SelectObject(mem, bitmap);
    let copied = BitBlt(
        mem,
        0,
        0,
        width,
        height,
        state().screen_dc,
        rect.left,
        rect.top,
        SRCCOPY,
    ) != 0;
    SelectObject(mem, old);
    let mut pixels = if copied {
        read_pixels(mem, bitmap, width, height)
    } else {
        None
    };
    DeleteObject(bitmap);
    if let Some(data) = pixels.as_mut() {
        if freeform && state().points.len() >= 3 {
            let points: Vec<POINT> = state()
                .points
                .iter()
                .map(|p| POINT {
                    x: p.x - rect.left,
                    y: p.y - rect.top,
                })
                .collect();
            if !apply_freeform_mask(mem, data, &points, width, height) {
                pixels = None;
            }
        } else {
            data.as_chunks_mut::<4>()
                .0
                .iter_mut()
                .for_each(|p| p[3] = 255);
        }
    }
    DeleteDC(mem);
    // Close the overlay first so the Save As dialog is not covered by it.
    cancel();
    let Some(pixels) = pixels else {
        message(state().owner, "Could not copy the selected pixels.");
        return;
    };
    let mut errors = Vec::new();
    if state().save && save_image(&pixels, width, height, state().ask).is_err() {
        errors.push("Could not save the image.");
    }
    if state().copy && !copy_image(&pixels, width, height) {
        errors.push("Could not copy the image to the clipboard.");
    }
    if !errors.is_empty() {
        message(state().owner, &errors.join("\n"));
    }
}
unsafe fn cancel() {
    let overlay = state().overlay;
    if !overlay.is_null() {
        DestroyWindow(overlay);
    }
    cleanup();
    if state().return_visible {
        ShowWindow(state().owner, SW_SHOW);
    }
}
unsafe extern "system" fn overlay_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_LBUTTONDOWN => {
            let p = point(lp);
            let (tx, ty) = toolbar();
            if p.y >= ty && p.y < ty + 52 && p.x >= tx && p.x < tx + 520 {
                let index = (p.x - tx) / 104;
                if index == 4 {
                    cancel();
                    return 0;
                }
                state().mode = index;
                InvalidateRect(hwnd, null(), 1);
                if index == MODE_SCREEN {
                    finish(
                        RECT {
                            left: 0,
                            top: 0,
                            right: state().width,
                            bottom: state().height,
                        },
                        false,
                    );
                }
                return 0;
            }
            if state().mode == MODE_WINDOW {
                if let Some(rect) = select_window(p) {
                    finish(rect, false);
                }
                return 0;
            }
            state().start = p;
            state().end = p;
            state().points.clear();
            state().points.push(p);
            state().dragging = true;
            SetCapture(hwnd);
            0
        }
        WM_MOUSEMOVE if state().dragging => {
            let p = point(lp);
            state().end = p;
            if state().mode == MODE_FREE {
                state().points.push(p);
            }
            InvalidateRect(hwnd, null(), 1);
            0
        }
        WM_LBUTTONUP if state().dragging => {
            state().end = point(lp);
            state().dragging = false;
            ReleaseCapture();
            let free = state().mode == MODE_FREE;
            let rect = if free && state().points.len() >= 3 {
                let mut r = RECT {
                    left: state().width,
                    top: state().height,
                    right: 0,
                    bottom: 0,
                };
                for p in &state().points {
                    r.left = r.left.min(p.x);
                    r.top = r.top.min(p.y);
                    r.right = r.right.max(p.x);
                    r.bottom = r.bottom.max(p.y);
                }
                bounds(
                    POINT {
                        x: r.left,
                        y: r.top,
                    },
                    POINT {
                        x: r.right,
                        y: r.bottom,
                    },
                    state().width,
                    state().height,
                )
            } else {
                bounds(state().start, state().end, state().width, state().height)
            };
            if !free || state().points.len() >= 3 {
                finish(rect, free);
            }
            0
        }
        WM_KEYDOWN if wp as u32 == VK_ESCAPE as u32 => {
            cancel();
            0
        }
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let dc = BeginPaint(hwnd, &mut ps);
            let mut rect: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut rect);
            FillRect(dc, &rect, GetStockObject(BLACK_BRUSH) as HBRUSH);
            let (x, y) = toolbar();
            let brush = CreateSolidBrush(0x00333333);
            let bar = RECT {
                left: x,
                top: y,
                right: x + 520,
                bottom: y + 52,
            };
            FillRect(dc, &bar, brush);
            DeleteObject(brush);
            SetBkMode(dc, TRANSPARENT as i32);
            SetTextColor(dc, 0x00ffffff);
            for (i, title) in ["Rectangle", "Freeform", "Window", "Full screen", "Close"]
                .iter()
                .enumerate()
            {
                let mut r = RECT {
                    left: x + (i as i32) * 104,
                    top: y + 8,
                    right: x + (i as i32 + 1) * 104,
                    bottom: y + 44,
                };
                if i as i32 == state().mode {
                    let b = CreateSolidBrush(0x008b5a21);
                    FillRect(dc, &r, b);
                    DeleteObject(b);
                }
                let mut s = w(title);
                DrawTextW(
                    dc,
                    s.as_mut_ptr(),
                    -1,
                    &mut r,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                );
            }
            if state().dragging {
                let pen = CreatePen(PS_SOLID, 3, 0x00ffffff);
                let oldpen = SelectObject(dc, pen);
                let clear = CreateSolidBrush(0x00ff00ff);
                let oldbrush = SelectObject(dc, clear);
                if state().mode == MODE_FREE && state().points.len() > 1 {
                    Polygon(dc, state().points.as_ptr(), state().points.len() as i32);
                } else {
                    Rectangle(
                        dc,
                        state().start.x.min(state().end.x),
                        state().start.y.min(state().end.y),
                        state().start.x.max(state().end.x),
                        state().start.y.max(state().end.y),
                    );
                }
                SelectObject(dc, oldbrush);
                DeleteObject(clear);
                SelectObject(dc, oldpen);
                DeleteObject(pen);
            }
            EndPaint(hwnd, &ps);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
unsafe fn store_settings(hwnd: HWND) {
    if save_settings(state().copy, state().save, state().ask, state().delay).is_err() {
        message(hwnd, "Could not save the settings.");
    }
}
unsafe extern "system" fn main_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
            state().owner = hwnd;
            tray(hwnd, NIM_ADD);
            text(
                hwnd,
                "Capture screenshots with Alt + Shift + S",
                24,
                20,
                430,
            );
            button(hwnd, "New snip", NEW, 24, 58, 150, BS_PUSHBUTTON as u32);
            text(hwnd, "Settings", 24, 110, 430);
            let copy = button(
                hwnd,
                "Automatically copy screenshots",
                COPY,
                24,
                145,
                350,
                BS_AUTOCHECKBOX as u32,
            );
            let save = button(
                hwnd,
                "Automatically save screenshots",
                SAVE,
                24,
                182,
                350,
                BS_AUTOCHECKBOX as u32,
            );
            SendMessageW(copy, BM_SETCHECK, if state().copy { 1 } else { 0 }, 0);
            SendMessageW(save, BM_SETCHECK, if state().save { 1 } else { 0 }, 0);
            let ask = button(
                hwnd,
                "Choose a file name when saving",
                ASK,
                24,
                219,
                350,
                BS_AUTOCHECKBOX as u32,
            );
            SendMessageW(ask, BM_SETCHECK, if state().ask { 1 } else { 0 }, 0);
            text(hwnd, "Capture delay", 24, 262, 130);
            let delay = CreateWindowExW(
                0,
                w("COMBOBOX").as_ptr(),
                null(),
                WS_CHILD | WS_VISIBLE | WS_VSCROLL | CBS_DROPDOWNLIST as u32,
                160,
                258,
                150,
                150,
                hwnd,
                DELAY as HMENU,
                GetModuleHandleW(null()),
                null(),
            );
            for label in ["None", "3 seconds", "5 seconds", "10 seconds"] {
                SendMessageW(delay, CB_ADDSTRING, 0, w(label).as_ptr() as LPARAM);
            }
            let selected = DELAYS.iter().position(|&d| d == state().delay).unwrap_or(0);
            SendMessageW(delay, CB_SETCURSEL, selected, 0);
            text(
                hwnd,
                "Saved images: Pictures\\Screenshots (PNG or BMP)",
                24,
                298,
                430,
            );
            let status = if RegisterHotKey(
                hwnd,
                HOTKEY,
                MOD_ALT | MOD_SHIFT | MOD_NOREPEAT,
                b'S' as u32,
            ) != 0
            {
                "Shortcut: Alt + Shift + S"
            } else {
                "Shortcut unavailable: another application is using it."
            };
            text(hwnd, status, 24, 334, 430);
            0
        }
        WM_COMMAND => {
            match wp & 0xffff {
                NEW => begin_capture(),
                TRAY_SETTINGS => {
                    ShowWindow(hwnd, SW_SHOW);
                    SetForegroundWindow(hwnd);
                }
                TRAY_EXIT => {
                    DestroyWindow(hwnd);
                }
                DELAY if (wp >> 16) as u32 == CBN_SELCHANGE => {
                    let index = SendMessageW(GetDlgItem(hwnd, DELAY as i32), CB_GETCURSEL, 0, 0);
                    if let Some(&delay) = DELAYS.get(index as usize) {
                        state().delay = delay;
                    }
                    store_settings(hwnd);
                }
                COPY | SAVE | ASK => {
                    let copy = SendMessageW(GetDlgItem(hwnd, COPY as i32), BM_GETCHECK, 0, 0) == 1;
                    let save = SendMessageW(GetDlgItem(hwnd, SAVE as i32), BM_GETCHECK, 0, 0) == 1;
                    if !copy && !save {
                        message(hwnd, "Choose at least one destination for screenshots.");
                        SendMessageW(GetDlgItem(hwnd, COPY as i32), BM_SETCHECK, 1, 0);
                        state().copy = true;
                        state().save = false;
                    } else {
                        state().copy = copy;
                        state().save = save;
                    }
                    state().ask =
                        SendMessageW(GetDlgItem(hwnd, ASK as i32), BM_GETCHECK, 0, 0) == 1;
                    store_settings(hwnd);
                }
                _ => {}
            }
            0
        }
        TRAY_MESSAGE => {
            if lp as u32 == WM_LBUTTONUP {
                begin_capture();
            }
            if lp as u32 == WM_RBUTTONUP {
                let menu = CreatePopupMenu();
                AppendMenuW(menu, MF_STRING, NEW, w("New snip").as_ptr());
                AppendMenuW(menu, MF_STRING, TRAY_SETTINGS, w("Settings").as_ptr());
                AppendMenuW(menu, MF_SEPARATOR, 0, null());
                AppendMenuW(menu, MF_STRING, TRAY_EXIT, w("Exit").as_ptr());
                let mut cursor: POINT = std::mem::zeroed();
                GetCursorPos(&mut cursor);
                SetForegroundWindow(hwnd);
                TrackPopupMenu(menu, TPM_RIGHTBUTTON, cursor.x, cursor.y, 0, hwnd, null());
                DestroyMenu(menu);
            }
            0
        }
        WM_CLOSE => {
            ShowWindow(hwnd, SW_HIDE);
            0
        }
        WM_TIMER if wp == DELAY_TIMER => {
            KillTimer(hwnd, DELAY_TIMER);
            state().pending = false;
            capture();
            0
        }
        WM_HOTKEY if wp == HOTKEY as usize => {
            begin_capture();
            0
        }
        WM_DESTROY => {
            tray(hwnd, NIM_DELETE);
            UnregisterHotKey(hwnd, HOTKEY);
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
unsafe fn tray(hwnd: HWND, operation: u32) {
    let mut data: NOTIFYICONDATAW = std::mem::zeroed();
    data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    data.hWnd = hwnd;
    data.uID = 1;
    data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    data.uCallbackMessage = TRAY_MESSAGE;
    data.hIcon = LoadIconW(null_mut(), IDI_APPLICATION);
    let title = w("Snipping Lite");
    data.szTip[..title.len()].copy_from_slice(&title);
    Shell_NotifyIconW(operation, &data);
}
unsafe fn register(name: &str, proc: WNDPROC, brush: HBRUSH) {
    let class_name = w(name);
    let class = WNDCLASSW {
        lpfnWndProc: proc,
        hInstance: GetModuleHandleW(null()),
        lpszClassName: class_name.as_ptr(),
        hCursor: LoadCursorW(null_mut(), IDC_CROSS),
        hbrBackground: brush,
        ..std::mem::zeroed()
    };
    RegisterClassW(&class);
}
fn main() {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let (copy, save, ask, delay) = load_settings();
        let mut app = Box::new(State {
            owner: null_mut(),
            overlay: null_mut(),
            screen: null_mut(),
            screen_dc: null_mut(),
            previous: null_mut(),
            origin: POINT { x: 0, y: 0 },
            width: 0,
            height: 0,
            mode: MODE_RECT,
            start: POINT { x: 0, y: 0 },
            end: POINT { x: 0, y: 0 },
            points: Vec::new(),
            dragging: false,
            return_visible: true,
            copy,
            save,
            ask,
            delay,
            pending: false,
        });
        let mut token = 0usize;
        let input = GdiplusStartupInput {
            GdiplusVersion: 1,
            DebugEventCallback: 0,
            SuppressBackgroundThread: 0,
            SuppressExternalCodecs: 0,
        };
        let mut output: GdiplusStartupOutput = std::mem::zeroed();
        let gdiplus = GdiplusStartup(&mut token, &input, &mut output) == 0;
        STATE = &mut *app;
        register(MAIN, Some(main_proc), (COLOR_WINDOW + 1) as HBRUSH);
        register(
            OVERLAY,
            Some(overlay_proc),
            GetStockObject(BLACK_BRUSH) as HBRUSH,
        );
        let hwnd = CreateWindowExW(
            0,
            w(MAIN).as_ptr(),
            w("Snipping Lite").as_ptr(),
            WS_OVERLAPPEDWINDOW & !WS_THICKFRAME & !WS_MAXIMIZEBOX,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            480,
            430,
            null_mut(),
            null_mut(),
            GetModuleHandleW(null()),
            null(),
        );
        if hwnd.is_null() {
            return;
        }
        ShowWindow(hwnd, SW_SHOW);
        UpdateWindow(hwnd);
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        if gdiplus {
            GdiplusShutdown(token);
        }
        STATE = null_mut();
    }
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
    #[test]
    fn selection_bounds_are_sorted_and_clipped() {
        let r = bounds(POINT { x: 140, y: 40 }, POINT { x: -10, y: 10 }, 100, 100);
        assert_eq!((r.left, r.top, r.right, r.bottom), (0, 10, 100, 40));
    }
}
