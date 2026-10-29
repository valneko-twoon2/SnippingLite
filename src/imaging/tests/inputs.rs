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
