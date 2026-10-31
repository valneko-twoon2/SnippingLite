use super::*;
use crate::test_support::clipboard_text;
use windows_sys::Win32::{
    Graphics::Gdi::{BITMAPV5HEADER, BI_BITFIELDS},
    System::{
        DataExchange::{CloseClipboard, GetClipboardData, OpenClipboard},
        Memory::{GlobalLock, GlobalUnlock},
    },
};

const SCRATCH: &str = "snipping-lite-imaging-";

/// このプロセスのテストが画像を書くフォルダーの中のパス。テストは並んで走って同じフォルダーを
/// 使うので、テストごとには消さない。代わりに初めて使うとき、もう動いていないプロセスが残した
/// フォルダーを消す（以前は実行のたびに 1 つずつ残っていた）。
fn scratch(name: &str) -> PathBuf {
    static TIDY: std::sync::Once = std::sync::Once::new();
    let temp = std::env::temp_dir();
    TIDY.call_once(|| remove_stale_scratch(&temp));
    let folder = temp.join(format!("{SCRATCH}{}", std::process::id()));
    fs::create_dir_all(&folder).unwrap();
    folder.join(name)
}

fn remove_stale_scratch(temp: &Path) {
    let Ok(entries) = fs::read_dir(temp) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let owner = name
            .to_str()
            .and_then(|name| name.strip_prefix(SCRATCH))
            .and_then(|id| id.parse::<u32>().ok());
        if owner.is_some_and(|id| !crate::staging::alive(id)) {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

/// A 3x2 image whose pixels all differ, including every alpha class.
fn sample() -> Vec<u8> {
    vec![
        0, 0, 255, 255, 0, 255, 0, 255, 255, 0, 0, 255, // opaque red, green, blue
        10, 20, 30, 128, 40, 50, 60, 1, 255, 255, 255, 0, // partial, nearly clear, clear
    ]
}

/// このプロセスがこれまでにコミットしたメモリーの最大値（バイト）。テストは並んで走るので、
/// 増え方は大きな余裕をもって比べる。
fn peak_commit() -> usize {
    /// PROCESS_MEMORY_COUNTERS（psapi.h）。
    #[repr(C)]
    #[derive(Default)]
    struct Counters {
        size: u32,
        page_faults: u32,
        peak_working_set: usize,
        working_set: usize,
        quota_peak_paged_pool: usize,
        quota_paged_pool: usize,
        quota_peak_non_paged_pool: usize,
        quota_non_paged_pool: usize,
        commit: usize,
        peak_commit: usize,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn K32GetProcessMemoryInfo(process: isize, counters: *mut Counters, size: u32) -> i32;
    }
    let mut counters = Counters {
        size: std::mem::size_of::<Counters>() as u32,
        ..Default::default()
    };
    // SAFETY: -1 is the pseudo handle of this process, and the structure is sized as declared.
    let ok = unsafe { K32GetProcessMemoryInfo(-1, &mut counters, counters.size) };
    assert_ne!(ok, 0, "the memory counters are readable");
    counters.peak_commit
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

/// クリップボードの CF_DIB（8）を、上から下の順の BGRA の画素として読む。ビットフィールドの
/// マスクがあれば読み飛ばす。32 ビットでなければ `None`。
fn clipboard_dib() -> Option<(i32, i32, Vec<u8>)> {
    use windows_sys::Win32::Graphics::Gdi::BITMAPINFOHEADER;
    // SAFETY: the clipboard is opened and closed here, and the data is copied out while locked.
    unsafe {
        if OpenClipboard(null_mut()) == 0 {
            return None;
        }
        let global = GetClipboardData(8);
        let data = if global.is_null() {
            null_mut()
        } else {
            GlobalLock(global).cast::<u8>()
        };
        let mut image = None;
        if !data.is_null() {
            let header = std::ptr::read_unaligned(data.cast::<BITMAPINFOHEADER>());
            let masks = if header.biSize == 40 && header.biCompression == BI_BITFIELDS {
                12
            } else {
                0
            };
            let (width, height) = (header.biWidth, header.biHeight);
            // 下から上の順の 32 ビットのときだけ、行に余りがなく、この大きさで読める。
            if header.biBitCount == 32 && width > 0 && height > 0 {
                let size = width as usize * height as usize * 4;
                let start = data.add(header.biSize as usize + masks);
                let bottom_up = std::slice::from_raw_parts(start, size);
                let rows: Vec<u8> = bottom_up
                    .chunks_exact(width as usize * 4)
                    .rev()
                    .flatten()
                    .copied()
                    .collect();
                image = Some((width, height, rows));
            }
            GlobalUnlock(global);
        }
        CloseClipboard();
        image
    }
}

#[test]
#[ignore = "Replaces the clipboard content"]
fn an_application_without_transparency_sees_the_image_on_white() {
    assert!(copy_to_clipboard(null_mut(), &sample(), 3, 2));
    // 透過を読むアプリは CF_DIBV5 から元のままの画素を受け取る。
    let (_, _, v5) = clipboard_image().unwrap();
    assert_eq!(v5[..12], sample()[12..]);
    // 透過を読まないアプリの読む CF_DIB は、白の上に重ねた不透明な画素になる。期待値は
    // (c * a + 255 * (255 - a)) / 255 の四捨五入を手で計算したもの。
    // a = 128: 10 → 132.0、20 → 137.0、30 → 142.1。a = 1: 40 → 254.2、50 → 254.2、60 → 254.2。
    let (width, height, pixels) = clipboard_dib().expect("a 32-bit CF_DIB");
    assert_eq!((width, height), (3, 2));
    #[rustfmt::skip]
    let expected = [
        0, 0, 255, 255, 0, 255, 0, 255, 255, 0, 0, 255,
        132, 137, 142, 255, 254, 254, 254, 255, 255, 255, 255, 255,
    ];
    assert_eq!(pixels, expected);
    // CF_BITMAP だけを読むアプリには、Windows が作る DDB を渡す。その色も白の上に重ねたもの。
    let colors: Vec<[u8; 3]> = clipboard_bitmap()
        .expect("a CF_BITMAP")
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| [p[0], p[1], p[2]])
        .collect();
    let wanted: Vec<[u8; 3]> = expected
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| [p[0], p[1], p[2]])
        .collect();
    assert_eq!(colors, wanted);
}

/// クリップボードの CF_BITMAP（2）を、上から下の順の 32 ビットの画素として読む。
fn clipboard_bitmap() -> Option<Vec<u8>> {
    use windows_sys::Win32::Graphics::Gdi::{
        GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, DIB_RGB_COLORS,
    };
    // SAFETY: the clipboard is opened and closed here; the bitmap belongs to the clipboard and is
    // only read while it is open.
    unsafe {
        if OpenClipboard(null_mut()) == 0 {
            return None;
        }
        let bitmap = GetClipboardData(2);
        let mut pixels = None;
        let mut size: BITMAP = std::mem::zeroed();
        if !bitmap.is_null()
            && GetObjectW(
                bitmap,
                std::mem::size_of::<BITMAP>() as i32,
                (&mut size as *mut BITMAP).cast(),
            ) != 0
        {
            let mut info: BITMAPINFO = std::mem::zeroed();
            info.bmiHeader.biSize = std::mem::size_of_val(&info.bmiHeader) as u32;
            info.bmiHeader.biWidth = size.bmWidth;
            info.bmiHeader.biHeight = -size.bmHeight;
            info.bmiHeader.biPlanes = 1;
            info.bmiHeader.biBitCount = 32;
            let mut out = vec![0u8; size.bmWidth as usize * size.bmHeight as usize * 4];
            let dc = GetDC(null_mut());
            let lines = GetDIBits(
                dc,
                bitmap,
                0,
                size.bmHeight as u32,
                out.as_mut_ptr().cast(),
                &mut info,
                DIB_RGB_COLORS,
            );
            ReleaseDC(null_mut(), dc);
            pixels = (lines == size.bmHeight).then_some(out);
        }
        CloseClipboard();
        pixels
    }
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

#[test]
#[ignore = "Replaces the clipboard content"]
fn a_file_goes_on_the_clipboard_as_a_file_list() {
    // 日本語やスペースを含むパスも、そのまま貼り付けられる。
    for name in ["clip.mp4", "画面録画 1.mp4"] {
        let path = scratch(name);
        assert!(copy_file(null_mut(), &path), "{name}");
        assert_eq!(crate::test_support::clipboard_files(), vec![path]);
    }
}

mod crash;
mod differential;
mod fuzz;
mod inputs;
