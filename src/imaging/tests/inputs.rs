//! Pictures and pixel buffers that arrive damaged, empty or with the wrong size.

use super::*;

fn valid_png(name: &str) -> Vec<u8> {
    let path = scratch(&format!("inputs-valid-{name}.png"));
    encode(&path, &sample(), 3, 2).unwrap();
    fs::read(path).unwrap()
}

#[test]
fn pictures_that_cannot_be_read_give_nothing() {
    let valid = valid_png("unreadable");
    let cases: [(&str, Vec<u8>); 5] = [
        ("empty", Vec::new()),
        ("text", b"not a picture at all".to_vec()),
        ("signature only", valid[..8].to_vec()),
        ("header only", valid[..33].to_vec()),
        ("zeros", vec![0; 4096]),
    ];
    for (name, bytes) in cases {
        let path = scratch(&format!("inputs-{}.png", name.replace(' ', "-")));
        fs::write(&path, bytes).unwrap();
        assert!(decode(&path).is_none(), "{name}");
    }
    assert!(decode(&scratch("inputs-missing.png")).is_none(), "missing");
    let folder = scratch("inputs-folder.png");
    fs::create_dir_all(&folder).unwrap();
    assert!(decode(&folder).is_none(), "a folder");
}

#[test]
fn a_cut_off_picture_is_never_returned_with_a_wrong_size() {
    let valid = valid_png("cut");
    for keep in [valid.len() / 2, valid.len() - 1] {
        let path = scratch(&format!("inputs-cut-{keep}.png"));
        fs::write(&path, &valid[..keep]).unwrap();
        if let Some((width, height, pixels)) = decode(&path) {
            assert_eq!(pixels.len(), width as usize * height as usize * 4, "{keep}");
        }
    }
}

/// 2x2 の正しい PNG の IHDR だけを、`width` x `height` の宣言に書き換えたもの。
fn declaring(width: u32, height: u32) -> Vec<u8> {
    use crate::test_support::png::{self, Header, Picture};
    let picture = Picture {
        width: 2,
        height: 2,
        rgba: [10, 20, 30, 255].repeat(4),
        filters: vec![0, 0],
    };
    let mut chunks = png::parse(&picture.png(65535)).unwrap().chunks;
    chunks[0].data = Header::rgba(width, height).bytes();
    chunks[0].seal();
    png::join(&chunks)
}

/// 20000 x 20000 を宣言する 35 バイトの GIF（画素は LZW で 1 つだけ）。
fn tiny_gif() -> Vec<u8> {
    let mut gif = b"GIF89a".to_vec();
    gif.extend(20000u16.to_le_bytes());
    gif.extend(20000u16.to_le_bytes());
    gif.extend([0x80, 0, 0, 0, 0, 0, 255, 255, 255, 0x2c, 0, 0, 0, 0]);
    gif.extend(20000u16.to_le_bytes());
    gif.extend(20000u16.to_le_bytes());
    gif.extend([0, 2, 2, 0x44, 0x01, 0, 0x3b]);
    gif
}

#[test]
fn a_small_file_that_declares_a_huge_picture_is_refused_without_taking_the_memory() {
    // 86 バイトの PNG と 35 バイトの GIF が、20000 x 20000（BGRA で 16 億バイト）を宣言する。宣言を信じると、
    // GDI+ と読み取りがそれぞれ 16 億バイトを取り、履歴のウィンドウが巨大なテクスチャを作ろうとする。
    let png = declaring(20_000, 20_000);
    assert_eq!(png.len(), 86);
    let peak = peak_commit();
    for (name, bytes) in [("png", png), ("gif", tiny_gif())] {
        let path = scratch(&format!("inputs-huge.{name}"));
        fs::write(&path, &bytes).unwrap();
        let started = std::time::Instant::now();
        assert!(decode(&path).is_none(), "{name}");
        assert!(
            started.elapsed() < std::time::Duration::from_secs(1),
            "{name}"
        );
    }
    let grown = peak_commit().saturating_sub(peak);
    assert!(
        grown < 64 << 20,
        "the peak commit grew by {} MB",
        grown >> 20
    );
}

#[test]
fn a_picture_may_declare_as_many_pixels_as_its_file_could_hold() {
    // 1 バイトの deflate は最大 1032 バイトに広がり、PNG の画素は最小 1 ビットなので、86 バイトの
    // ファイルが持てる画素は 86 x 1032 x 8 = 710,016 個まで。境界の両側を試す（GDI+ は幅の広すぎる
    // 走査線を自分で断るので、幅は控えめにする）。
    let limit = 86 * 1032 * 8;
    for (width, height, readable) in [
        (1_000, 710, true),
        (16, 44_376, true),
        (16, 44_377, false),
        (1_000, 711, false),
    ] {
        let bytes = declaring(width, height);
        assert_eq!(bytes.len(), 86);
        assert_eq!(width as u64 * height as u64 <= limit, readable);
        let path = scratch(&format!("inputs-declared-{width}x{height}.png"));
        fs::write(&path, &bytes).unwrap();
        let read = decode(&path);
        assert_eq!(read.is_some(), readable, "{width}x{height}");
        if let Some((w, h, pixels)) = read {
            // GDI+ は足りない行を埋めて、宣言どおりの大きさで返す。
            assert_eq!((w, h), (width as i32, height as i32));
            assert!(fits(&pixels, w, h));
        }
    }
}

#[test]
fn a_picture_is_read_by_its_content_and_not_by_its_extension() {
    let bmp = scratch("inputs-content.bmp");
    encode(&bmp, &sample(), 3, 2).unwrap();
    let disguised = scratch("inputs-disguised.png");
    fs::copy(&bmp, &disguised).unwrap();
    let (width, height, _) = decode(&disguised).expect("a BMP named .png is still a picture");
    assert_eq!((width, height), (3, 2));
}

#[test]
fn pixels_must_match_the_stated_size_exactly() {
    // Equivalence classes and the edges around each: (width, height, byte count, accepted).
    let cases = [
        (1, 1, 4, true),
        (2, 2, 16, true),
        (2, 2, 15, false),
        (2, 2, 17, false),
        (2, 2, 0, false),
        (0, 2, 0, false),
        (2, 0, 0, false),
        (0, 0, 0, false),
        (-1, 2, 8, false),
        (2, -1, 8, false),
        (i32::MAX, 2, 8, false),
        (i32::MAX, i32::MAX, 4, false),
    ];
    for (width, height, len, accepted) in cases {
        // Only small buffers are allocated; the huge sizes are refused before any is needed.
        let pixels = vec![255u8; len];
        assert_eq!(
            fits(&pixels, width, height),
            accepted,
            "{width}x{height} with {len} bytes"
        );
        let path = scratch(&format!("inputs-size-{width}-{height}-{len}.png"));
        let _ = fs::remove_file(&path);
        let result = encode(&path, &pixels, width, height);
        assert_eq!(
            result.is_ok(),
            accepted,
            "{width}x{height} with {len} bytes"
        );
        if !accepted {
            assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::InvalidInput);
            assert!(!path.exists(), "no file for a refused picture");
        }
    }
}

#[test]
fn encoding_into_a_place_that_cannot_hold_the_file_fails() {
    let pixels = vec![255u8; 16];
    let missing_folder = scratch("inputs-no-such-folder")
        .join("deeper")
        .join("a.png");
    assert!(encode(&missing_folder, &pixels, 2, 2).is_err());
    let folder = scratch("inputs-a-folder.png");
    fs::create_dir_all(&folder).unwrap();
    assert!(encode(&folder, &pixels, 2, 2).is_err());
}

#[test]
fn the_clipboard_copy_refuses_a_picture_that_does_not_fit_instead_of_panicking() {
    let four = [0u8; 4];
    for (pixels, width, height) in [
        (&[][..], 0, 0),
        (&four[..], 0, 1),
        (&four[..], 1, 0),
        (&four[..], 2, 1),
        (&four[..], -1, 1),
    ] {
        assert!(
            !copy_to_clipboard(std::ptr::null_mut(), pixels, width, height),
            "{width}x{height} with {} bytes",
            pixels.len()
        );
    }
}
