//! 縮小後の大きさと、面積平均の色。期待値はすべて手計算から。

use super::*;

/// B、G、R が同じ値 `v` の不透明な画素。
fn gray(v: u8) -> [u8; 4] {
    [v, v, v, 255]
}

#[test]
fn a_picture_within_the_side_keeps_its_size() {
    assert_eq!(fit(2048, 1, 2048), (2048, 1));
    assert_eq!(fit(2048, 2048, 2048), (2048, 2048));
    assert_eq!(fit(60, 40, 2048), (60, 40));
}

#[test]
fn a_larger_picture_fits_the_side_with_its_shape_kept() {
    // 長い辺を 2048 にし、短い辺は同じ比で縮めて四捨五入する。
    // 1 x 2048 / 2049 = 0.9995 -> 1
    assert_eq!(fit(2049, 1, 2048), (2048, 1));
    // 1500 x 2048 / 3000 = 1024
    assert_eq!(fit(3000, 1500, 2048), (2048, 1024));
    // 1000 x 2048 / 3001 = 682.44 -> 682
    assert_eq!(fit(3001, 1000, 2048), (2048, 682));
    // 縦長: 1000 x 16384 / 20000 = 819.2 -> 819
    assert_eq!(fit(1000, 20000, 16384), (819, 16384));
    // 0.5 ちょうどは切り上げる: 3 x 2 / 4 = 1.5 -> 2
    assert_eq!(fit(4, 3, 2), (2, 2));
}

#[test]
fn a_side_is_never_shrunk_below_one_pixel() {
    // 1 x 2048 / 5000 = 0.41 は 0 に丸まるが、1 画素は残す。
    assert_eq!(fit(1, 5000, 2048), (1, 2048));
    assert_eq!(fit(100_000, 1, 2048), (2048, 1));
}

#[test]
fn each_pixel_is_the_rounded_average_of_its_block() {
    // 2 x 2 -> 1 x 1。B: (10 + 20 + 30 + 42) / 4 = 25.5 -> 26、G: (20 + 30 + 40 + 50) / 4 = 35、
    // R: (30 + 40 + 50 + 61) / 4 = 45.25 -> 45。
    let pixels = [
        [10, 20, 30, 255],
        [20, 30, 40, 255],
        [30, 40, 50, 255],
        [42, 50, 61, 255],
    ]
    .concat();
    assert_eq!(shrink(&pixels, 2, 2, 1, 1), [26, 35, 45, 255]);
}

#[test]
fn uneven_blocks_split_by_the_floor_of_the_scaled_position() {
    // 3 -> 2: 元の x は floor(x x 2 / 3) に入る。0 と 1 は 0 に、2 は 1 に。
    // (10 + 20) / 2 = 15、90 はそのまま。
    let row = [gray(10), gray(20), gray(90)].concat();
    assert_eq!(shrink(&row, 3, 1, 2, 1), [gray(15), gray(90)].concat());
    // 縦も同じ: (0 + 100) / 2 = 50、200 はそのまま。
    let column = [gray(0), gray(100), gray(200)].concat();
    assert_eq!(shrink(&column, 1, 3, 1, 2), [gray(50), gray(200)].concat());
}

#[test]
fn transparent_pixels_do_not_darken_the_color_beside_them() {
    // 不透明な赤と、透明な黒。色は不透明度で重み付けするので赤のまま、不透明度は
    // (255 + 0) / 2 = 127.5 -> 128。単純に平均すると R は 128 の暗い赤になる。
    let pixels = [[0, 0, 255, 255], [0, 0, 0, 0]].concat();
    assert_eq!(shrink(&pixels, 2, 1, 1, 1), [0, 0, 255, 128]);
    // すべて透明なら、色も不透明度も 0。
    let clear = [[9, 9, 9, 0], [7, 7, 7, 0]].concat();
    assert_eq!(shrink(&clear, 2, 1, 1, 1), [0, 0, 0, 0]);
}

#[test]
fn the_same_size_leaves_the_pixels_unchanged() {
    let pixels = [[1, 2, 3, 255], [4, 5, 6, 255], [7, 8, 9, 255]].concat();
    assert_eq!(shrink(&pixels, 3, 1, 3, 1), pixels);
}

#[test]
fn halving_a_red_and_cyan_picture_keeps_the_two_halves_apart() {
    // 履歴のテストの画像と同じ、左半分が赤で右半分がシアンの 8 x 2 を 4 x 1 に。
    // 2 x 2 のブロックはどれも片側の色だけを含むので、混ざらない。
    let (red, cyan) = ([0, 0, 255, 255], [255, 255, 0, 255]);
    let row = [red, red, red, red, cyan, cyan, cyan, cyan].concat();
    let pixels = [row.clone(), row].concat();
    assert_eq!(shrink(&pixels, 8, 2, 4, 1), [red, red, cyan, cyan].concat());
}
