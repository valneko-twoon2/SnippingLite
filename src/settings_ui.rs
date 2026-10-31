//! Settings window. It runs in its own process (`--settings`) so the egui and
//! OpenGL memory is released when the window closes and the tray process stays small.

use eframe::egui::{
    self, epaint::text::VariationCoords, pos2, Align, Align2, Color32, CornerRadius, FontData,
    FontDefinitions, FontFamily, FontId, FontTweak, Layout, RichText, Sense, Stroke, StrokeKind,
    Theme, UiBuilder, Vec2,
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{AllowSetForegroundWindow, PostMessageW, ASFW_ANY, WM_COMMAND},
};

use crate::i18n::tr;
use crate::{
    config::{self, HISTORY_LIMITS},
    imaging, ui, w, MAIN, SETTINGS_TITLE, SETTINGS_WINDOW, TRAY_HISTORY, TRAY_RELOAD,
};

mod app;
mod expander;
mod icons;
mod widgets;
use app::*;
use expander::*;
use icons::*;
use widgets::*;

const SEMIBOLD: &str = "semibold";
/// Downward shift of Japanese glyphs as a fraction of the font size.
/// Yu Gothic UI は行の寸法が Segoe UI と同じで、ずらさなくても WinUI と同じ高さに並ぶ。
/// 游ゴシックのように行の上側が広いフォントに替えるときは、ここで下げて上下中央にそろえる。
const JAPANESE_Y_OFFSET: f32 = 0.0;
const WIDTH: f32 = 696.0;
const HEIGHT: f32 = 760.0;
const MARGIN: f32 = 56.0;
const ROW: f32 = 64.0;

pub(crate) struct Palette {
    /// Translucent colors let the Mica backdrop show through.
    pub(crate) mica: bool,
    pub(crate) bg: Color32,
    pub(crate) card: Color32,
    pub(crate) stroke: Color32,
    pub(crate) text: Color32,
    pub(crate) secondary: Color32,
    pub(crate) field: Color32,
    pub(crate) accent: Color32,
    pub(crate) on_accent: Color32,
    pub(crate) warning: Color32,
}

impl Palette {
    pub(crate) fn new(dark: bool, mica: bool) -> Palette {
        let mut p = Palette::opaque(dark);
        p.mica = mica;
        if mica {
            p.bg = Color32::TRANSPARENT;
            p.card = if dark {
                Color32::from_rgba_unmultiplied(255, 255, 255, 13)
            } else {
                Color32::from_rgba_unmultiplied(255, 255, 255, 179)
            };
            p.stroke = if dark {
                Color32::from_rgba_unmultiplied(0, 0, 0, 56)
            } else {
                Color32::from_rgba_unmultiplied(0, 0, 0, 15)
            };
        }
        p
    }

    fn opaque(dark: bool) -> Palette {
        if dark {
            Palette {
                mica: false,
                bg: Color32::from_rgb(0x20, 0x20, 0x20),
                card: Color32::from_rgb(0x2b, 0x2b, 0x2b),
                stroke: Color32::from_rgb(0x1d, 0x1d, 0x1d),
                text: Color32::WHITE,
                secondary: Color32::from_rgb(0xc5, 0xc5, 0xc5),
                field: Color32::from_rgb(0x33, 0x33, 0x33),
                accent: Color32::from_rgb(0x4c, 0xc2, 0xff),
                on_accent: Color32::BLACK,
                warning: Color32::from_rgb(0xff, 0x99, 0xa4),
            }
        } else {
            Palette {
                mica: false,
                bg: Color32::from_rgb(0xf3, 0xf3, 0xf3),
                card: Color32::from_rgb(0xfb, 0xfb, 0xfb),
                stroke: Color32::from_rgb(0xe5, 0xe5, 0xe5),
                text: Color32::from_rgb(0x1a, 0x1a, 0x1a),
                secondary: Color32::from_rgb(0x61, 0x61, 0x61),
                field: Color32::WHITE,
                accent: Color32::from_rgb(0x00, 0x67, 0xc0),
                on_accent: Color32::WHITE,
                warning: Color32::from_rgb(0xc4, 0x2b, 0x1c),
            }
        }
    }
}

pub fn run(hotkey_ok: bool) {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(SETTINGS_TITLE)
            .with_inner_size([WIDTH, HEIGHT])
            .with_resizable(false)
            .with_maximize_button(false)
            .with_icon(egui::IconData {
                rgba: ui::icon_rgba(64),
                width: 64,
                height: 64,
            }),
        ..Default::default()
    };
    let _ = eframe::run_native(
        SETTINGS_TITLE,
        options,
        Box::new(move |cc| {
            if !install_fonts(&cc.egui_ctx) {
                return Err("No usable system font was found.".into());
            }
            // Mica は壁紙の色でウィンドウがアクティブなときに青みを帯びる。
            // Snipping Tool は中立な灰色のままなので、不透明な背景を使う。
            let mica = false;
            apply_theme(&cc.egui_ctx, config::load_settings().theme);
            install_visuals(&cc.egui_ctx, mica);
            Ok(Box::new(SettingsApp::new(hotkey_ok, mica)))
        }),
    );
}

/// 英数字のフォントの候補（ファイル, フェイスの番号）。前にあるものほど優先する。
const REGULAR_FACES: [(&str, u32); 5] = [
    ("SegUIVar.ttf", 0),
    ("segoeui.ttf", 0),
    ("tahoma.ttf", 0),
    ("arial.ttf", 0),
    ("micross.ttf", 0),
];
/// 見出しの Semibold の候補。可変フォントでは太さの軸で Semibold にする。
const SEMIBOLD_FACES: [(&str, u32); 2] = [("SegUIVar.ttf", 0), ("seguisb.ttf", 0)];
/// Yu Gothic UI は YuGothM.ttc などの 2 番目以降のフェイスにある。かながプロポーショナル幅で、
/// 行の寸法も Segoe UI と同じなので、英数字と混ぜても WinUI と同じ位置に並ぶ。
/// 游ゴシック（全角幅のかな）や古い Windows のメイリオは使わない。
const JAPANESE_FACES: [(&str, u32); 3] =
    [("YuGothM.ttc", 1), ("meiryo.ttc", 2), ("msgothic.ttc", 1)];
const JAPANESE_BOLD_FACES: [(&str, u32); 2] = [("YuGothB.ttc", 2), ("meiryob.ttc", 2)];

/// Windows のフォントのフォルダー。
fn fonts_folder() -> PathBuf {
    std::env::var_os("WINDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
        .join("Fonts")
}

/// Snipping Tool（WinUI）と同じ組み合わせのフォントを読み込む。英数字は Segoe UI Variable
/// （Windows 10 では Segoe UI）、日本語は Yu Gothic UI で描く。egui ships no fonts here,
/// so returns false if none is found.
pub(crate) fn install_fonts(ctx: &egui::Context) -> bool {
    install_fonts_from(ctx, &fonts_folder())
}

/// `folder` の中のフォントで `install_fonts` をする。テストが用意したフォルダーを渡せるよう分けてある。
fn install_fonts_from(ctx: &egui::Context, folder: &Path) -> bool {
    let mut fonts = FontDefinitions::empty();
    // (ファイル, フェイスの番号) の候補のうち、最初に読めたものを登録する。epaint は読めない
    // フォントで最初のフレームに panic するので、中身がファイルに収まっているフェイスだけを使う。
    let mut load = |faces: &[(&str, u32)], name: &str, tweak: FontTweak| {
        let (bytes, index) = faces.iter().find_map(|&(file, index)| {
            let bytes = fs::read(folder.join(file)).ok()?;
            face_fits(&bytes, index).then_some((bytes, index))
        })?;
        let mut data = FontData::from_owned(bytes).tweak(tweak.clone());
        data.index = index;
        fonts.font_data.insert(name.to_owned(), Arc::new(data));
        Some(name.to_owned())
    };
    let regular = load(&REGULAR_FACES, "regular", FontTweak::default());
    let semibold = load(
        &SEMIBOLD_FACES,
        "semibold",
        FontTweak {
            coords: VariationCoords::new([(b"wght", 600.0)]),
            ..FontTweak::default()
        },
    );
    let centered = || FontTweak {
        y_offset_factor: JAPANESE_Y_OFFSET,
        ..FontTweak::default()
    };
    let japanese = load(&JAPANESE_FACES, "japanese", centered());
    let japanese_bold = load(&JAPANESE_BOLD_FACES, "japanese_bold", centered());
    let Some(regular) = regular else {
        return false;
    };
    let body: Vec<String> = Some(regular.clone())
        .into_iter()
        .chain(japanese.iter().cloned())
        .collect();
    fonts
        .families
        .insert(FontFamily::Proportional, body.clone());
    fonts.families.insert(FontFamily::Monospace, body);
    fonts.families.insert(
        FontFamily::Name(SEMIBOLD.into()),
        semibold
            .into_iter()
            .chain(japanese_bold.or(japanese))
            .chain(Some(regular))
            .collect(),
    );
    ctx.set_fonts(fonts);
    true
}

/// フォントファイルに入っているフェイスの数。TrueType Collection（.ttc）は複数持つ。
fn face_count(bytes: &[u8]) -> u32 {
    match bytes.get(..12) {
        Some(header) if header.starts_with(b"ttcf") => {
            u32::from_be_bytes([header[8], header[9], header[10], header[11]])
        }
        Some(_) => 1,
        None => 0,
    }
}

/// 文字を描くのに要るテーブル。epaint（skrifa）はこれらが欠けても panic しないが、文字の対応
/// （cmap）がなければどの文字も見つからず、寸法（head、hhea、hmtx、maxp）がなければ文字の幅や
/// 大きさ、字形の数が 0 になり、文字が欠ける。
const DRAWING_TABLES: [&[u8; 4]; 5] = [b"cmap", b"head", b"hhea", b"hmtx", b"maxp"];
/// 字形の輪郭のテーブルの組。TrueType の glyf と loca の両方、CFF、CFF2 のどれか 1 組がそろえば
/// よい（skrifa はこの 3 つから輪郭を読む。フェイスの印とは結びつけない）。
const OUTLINE_TABLES: [&[&[u8; 4]]; 3] = [&[b"glyf", b"loca"], &[b"CFF "], &[b"CFF2"]];

/// フェイス `index` を epaint に渡してよいか。epaint は読めないフォントで最初のフレームに panic
/// するので、ヘッダーの数（`face_count`）を信じずに、途中で切れたファイルや壊れたファイルを
/// 渡す前に除く。TTC は位置の表がすべてファイルに収まり（読み手は表を丸ごと読み、収まらなければ
/// 空として扱う）、その番号の位置にフェイスの始まりがある。フェイスは TrueType か CFF の印で
/// 始まり、テーブルの一覧と、一覧が示す各テーブル（位置と長さ）がファイルに収まる。そのうえで、
/// 描くのに要るテーブル（`DRAWING_TABLES` と `OUTLINE_TABLES` の 1 組）がそろっていないフェイスも、
/// 文字が欠けるので壊れたものと同じに扱い、次の候補へ進む。
fn face_fits(bytes: &[u8], index: u32) -> bool {
    face_tables_fit(bytes, index).is_some()
}

fn face_tables_fit(bytes: &[u8], index: u32) -> Option<()> {
    // `at` から `size` バイトのビッグエンディアンの数。ファイルの外なら `None`。
    let number = |at: usize, size: usize| -> Option<usize> {
        let field = bytes.get(at..at.checked_add(size)?)?;
        Some(
            field
                .iter()
                .fold(0, |value, &byte| value << 8 | byte as usize),
        )
    };
    let count = face_count(bytes);
    if index >= count {
        return None;
    }
    let start = if bytes.starts_with(b"ttcf") {
        let table_end = (count as usize).checked_mul(4)?.checked_add(12)?;
        if table_end > bytes.len() {
            return None;
        }
        number(12 + 4 * index as usize, 4)?
    } else {
        0
    };
    // TrueType（0x00010000 と 'true'）と CFF（'OTTO'）の印。
    let version = bytes.get(start..start.checked_add(4)?)?;
    let marks: [&[u8]; 3] = [&[0, 1, 0, 0], b"true", b"OTTO"];
    if !marks.contains(&version) {
        return None;
    }
    let tables = number(start.checked_add(4)?, 2)?;
    // 一覧の各行の（タグ, 位置, 長さ）。
    let mut records = Vec::with_capacity(tables);
    for table in 0..tables {
        let record = start.checked_add(12 + 16 * table)?;
        let tag = bytes.get(record..record.checked_add(4)?)?;
        let offset = number(record.checked_add(8)?, 4)?;
        let length = number(record.checked_add(12)?, 4)?;
        if offset.checked_add(length)? > bytes.len() {
            return None;
        }
        records.push((tag, offset, length));
    }
    // 読み手（read-fonts）は同じタグの最初の行を使い、位置が 0 の行は無いものとして扱う。
    // 長さが 0 のテーブルも中身を読めないので、無いものとみなす。
    let has = |name: &[u8; 4]| {
        records
            .iter()
            .find(|(tag, _, _)| *tag == name.as_slice())
            .is_some_and(|&(_, offset, length)| offset != 0 && length != 0)
    };
    let drawn = DRAWING_TABLES.iter().all(|name| has(name));
    let outlined = OUTLINE_TABLES
        .iter()
        .any(|set| set.iter().all(|name| has(name)));
    (drawn && outlined).then_some(())
}

/// Applies the chosen app theme to `ui::theme()` and to the egui context.
pub(crate) fn apply_theme(ctx: &egui::Context, choice: config::AppTheme) {
    ui::set_theme_choice(choice);
    ctx.set_theme(match choice {
        config::AppTheme::System => egui::ThemePreference::System,
        config::AppTheme::Light => egui::ThemePreference::Light,
        config::AppTheme::Dark => egui::ThemePreference::Dark,
    });
}

pub(crate) fn install_visuals(ctx: &egui::Context, mica: bool) {
    for theme in [Theme::Dark, Theme::Light] {
        let dark = theme == Theme::Dark;
        let p = Palette::new(dark, mica);
        let mut v = if dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        v.panel_fill = p.bg;
        v.window_fill = p.card;
        v.extreme_bg_color = p.field;
        v.override_text_color = Some(p.text);
        v.window_corner_radius = CornerRadius::same(8);
        v.menu_corner_radius = CornerRadius::same(8);
        for widget in [
            &mut v.widgets.noninteractive,
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
            &mut v.widgets.open,
        ] {
            widget.corner_radius = CornerRadius::same(4);
        }
        v.widgets.inactive.bg_fill = p.field;
        v.widgets.inactive.weak_bg_fill = p.field;
        v.widgets.hovered.bg_fill = if dark {
            Color32::from_rgb(0x3a, 0x3a, 0x3a)
        } else {
            Color32::from_rgb(0xf6, 0xf6, 0xf6)
        };
        v.widgets.hovered.weak_bg_fill = v.widgets.hovered.bg_fill;
        v.widgets.inactive.bg_stroke = Stroke::new(1.0, p.stroke);
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, p.stroke);
        v.selection.bg_fill = p.accent;
        v.selection.stroke = Stroke::new(1.0, p.on_accent);
        ctx.set_visuals_of(theme, v);
    }
    ctx.all_styles_mut(|style| {
        style.text_styles.insert(
            egui::TextStyle::Body,
            FontId::new(14.0, FontFamily::Proportional),
        );
        style.text_styles.insert(
            egui::TextStyle::Button,
            FontId::new(14.0, FontFamily::Proportional),
        );
        style.text_styles.insert(
            egui::TextStyle::Small,
            FontId::new(12.0, FontFamily::Proportional),
        );
        style.interaction.tooltip_delay = 0.4;
    });
}

pub(crate) fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
}

#[cfg(test)]
mod tests;
