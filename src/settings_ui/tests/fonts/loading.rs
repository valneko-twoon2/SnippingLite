//! 本物のフォントの検査と、候補から最初に読めるフェイスを選ぶ読み込み（`install_fonts_from`）。
//! 読めないフェイスを epaint に渡すと、最初のフレームで panic するか、文字を正しく描けない（head が
//! 短いと units_per_em が 0 になって倍率が無限大になり、debug ビルドでは debug_assert で panic する。
//! cmap や hhea が短いと文字の範囲が、maxp が短いと字形の画像が変わる）。読めない候補を飛ばした
//! ときは、次の候補だけを置いたときと同じに描けるはずなので、それと比べる。

use super::*;

#[test]
fn the_system_fonts_the_windows_choose_from_pass_the_check() {
    // 本物のフォントを誤って除くと、日本語や太字が出なくなる。手元にあるものだけを確かめる。
    let folder = fonts_folder();
    let mut checked = 0;
    for &(file, index) in REGULAR_FACES
        .iter()
        .chain(&SEMIBOLD_FACES)
        .chain(&JAPANESE_FACES)
        .chain(&JAPANESE_BOLD_FACES)
    {
        if let Ok(bytes) = std::fs::read(folder.join(file)) {
            assert!(face_fits(&bytes, index), "{file} #{index}");
            checked += 1;
        }
    }
    assert!(checked > 0, "no font of the list was found in {folder:?}");
}

/// `sandbox` の中に、`files`（名前と中身）だけを置いたフォントのフォルダー `name` を作る。
fn fonts_with(sandbox: &Sandbox, name: &str, files: &[(&str, &[u8])]) -> PathBuf {
    let folder = sandbox.path().join(name);
    std::fs::create_dir_all(&folder).unwrap();
    for &(file, bytes) in files {
        std::fs::write(folder.join(file), bytes).unwrap();
    }
    folder
}

/// 「Snipping Lite」のラベルを 1 フレーム描いた結果。文字の並んだ範囲と、字形を描き込んだ
/// フォントの画像（テクスチャの変更）。範囲は文字の対応や寸法が狂うと変わり、画像は輪郭が
/// 描けない（字形の数が 0 と読まれるなど）と変わる。
struct Label {
    rects: Vec<egui::Rect>,
    atlas: egui::TexturesDelta,
}

impl Drop for Label {
    fn drop(&mut self) {
        // 画面に送らないので、テクスチャの変更は捨てる（epaint は扱わずに捨てると debug_assert で止まる）。
        self.atlas.clear();
    }
}

/// `folder` のフォントを読み込み、ラベルを 1 フレーム描く。
fn draw_label(folder: &Path) -> Label {
    let ctx = egui::Context::default();
    assert!(install_fonts_from(&ctx, folder), "{folder:?}");
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(400.0, 200.0),
        )),
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| {
        ui.label("Snipping Lite");
    });
    fn rects(shape: &egui::Shape, out: &mut Vec<egui::Rect>) {
        match shape {
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| rects(s, out)),
            egui::Shape::Text(text) => out.push(text.galley.rect),
            _ => {}
        }
    }
    let mut drawn = Vec::new();
    for clipped in &output.shapes {
        rects(&clipped.shape, &mut drawn);
    }
    // 比べるために取っておく。
    let atlas = std::mem::take(&mut output.textures_delta);
    Label {
        rects: drawn,
        atlas,
    }
}

/// 本物の次の候補（`REGULAR_FACES` の 2 番目）の中身と、それだけを置いたフォルダーで描いたラベル。
fn the_next_face_alone(sandbox: &Sandbox) -> (Vec<u8>, Label) {
    let (next, _) = REGULAR_FACES[1];
    let real = std::fs::read(fonts_folder().join(next)).unwrap();
    let drawn = draw_label(&fonts_with(sandbox, "alone", &[(next, &real)]));
    assert!(
        drawn
            .rects
            .iter()
            .any(|r| r.width() > 0.0 && r.width().is_finite() && r.height() > 0.0),
        "the label is drawn with {next} alone: {:?}",
        drawn.rects
    );
    assert!(!drawn.atlas.set.is_empty(), "the glyphs are drawn");
    (real, drawn)
}

/// 読めない候補を飛ばして次の候補で描いたなら、文字の範囲も字形の画像も、次の候補だけのときと同じ。
fn assert_drawn_like(drawn: &Label, expected: &Label, what: &str) {
    assert_eq!(drawn.rects, expected.rects, "where the text goes, {what}");
    assert!(drawn.atlas == expected.atlas, "the glyphs differ, {what}");
}

#[test]
fn a_cut_off_font_gives_way_to_the_next_one_and_text_still_draws() {
    // 最初の候補は途中で切れた TTC（4 つのフェイスがあると言うが 16 バイトしかない）。
    // そのまま渡すと、epaint は最初のフレームで panic する。
    let sandbox = Sandbox::new();
    let (real, expected) = the_next_face_alone(&sandbox);
    let cut = &collection_of(&[&[4], &[4], &[4], &[4]])[..16];
    let (first, _) = REGULAR_FACES[0];
    let (japanese, _) = JAPANESE_FACES[0];
    let (next, _) = REGULAR_FACES[1];
    let files = [(first, cut), (japanese, cut), (next, real.as_slice())];
    let drawn = draw_label(&fonts_with(&sandbox, "cut", &files));
    assert_drawn_like(&drawn, &expected, "after a cut-off font");
}

#[test]
fn a_real_face_with_a_table_too_short_to_read_gives_way_to_the_next_one() {
    // 最初の候補の名前で、次の候補の本物のフォントの、描くのに要るテーブルの 1 つを、読むのに要る
    // 長さ（`NEEDED`）より 1 バイト短くしたものを置く。
    let sandbox = Sandbox::new();
    let (real, expected) = the_next_face_alone(&sandbox);
    let (first, _) = REGULAR_FACES[0];
    let (next, _) = REGULAR_FACES[1];
    for (tag, least) in NEEDED {
        let name = tag_name(tag);
        let mut short = real.clone();
        let table = table_of(&short, 0, tag);
        set_record(&mut short, 0, table, LENGTH, least - 1);
        let files = [(first, short.as_slice()), (next, real.as_slice())];
        let drawn = draw_label(&fonts_with(&sandbox, &name, &files));
        let what = format!("{first} with {name} of {} bytes", least - 1);
        assert_drawn_like(&drawn, &expected, &what);
    }
}
