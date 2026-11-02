//! Text recognition in a screenshot with the Windows OCR engine. It uses the
//! languages of the user profile; a language is added in the Windows settings.

use crate::i18n::tr;
use std::sync::mpsc::{channel, Receiver};
use windows::{
    Graphics::Imaging::{BitmapAlphaMode, BitmapPixelFormat, SoftwareBitmap},
    Media::Ocr::OcrEngine,
    Storage::Streams::DataWriter,
    Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED},
};

#[derive(Debug, PartialEq)]
pub enum Error {
    /// No language that can be recognized is installed.
    NoLanguage,
    /// The image is larger than the engine accepts.
    TooLarge,
    Failed,
}

impl Error {
    pub fn message(&self) -> &'static str {
        match self {
            Error::NoLanguage => tr("Add a text recognition language in the Windows settings."),
            Error::TooLarge => tr("The image is too large to recognize text."),
            Error::Failed => tr("Could not recognize text in the image."),
        }
    }
}

/// Recognizes the text of top-down BGRA pixels on a background thread.
pub fn spawn(pixels: Vec<u8>, width: i32, height: i32) -> Receiver<Result<String, Error>> {
    let (send, receive) = channel();
    std::thread::spawn(move || {
        // SAFETY: initialized and uninitialized on this thread only.
        let initialized = unsafe { RoInitialize(RO_INIT_MULTITHREADED).is_ok() };
        let _ = send.send(recognize(&pixels, width, height));
        if initialized {
            unsafe { RoUninitialize() };
        }
    });
    receive
}

fn recognize(pixels: &[u8], width: i32, height: i32) -> Result<String, Error> {
    if width <= 0 || height <= 0 || pixels.len() != width as usize * height as usize * 4 {
        return Err(Error::Failed);
    }
    let engine = OcrEngine::TryCreateFromUserProfileLanguages().map_err(|_| Error::NoLanguage)?;
    let limit = OcrEngine::MaxImageDimension().map_err(|_| Error::Failed)? as i32;
    if width > limit || height > limit {
        return Err(Error::TooLarge);
    }
    let failed = |_| Error::Failed;
    let writer = DataWriter::new().map_err(failed)?;
    writer.WriteBytes(pixels).map_err(failed)?;
    let buffer = writer.DetachBuffer().map_err(failed)?;
    // The alpha channel is ignored: a transparent area reads as its color data.
    let bitmap = SoftwareBitmap::CreateCopyWithAlphaFromBuffer(
        &buffer,
        BitmapPixelFormat::Bgra8,
        width,
        height,
        BitmapAlphaMode::Ignore,
    )
    .map_err(failed)?;
    let result = engine
        .RecognizeAsync(&bitmap)
        .map_err(failed)?
        .join()
        .map_err(failed)?;
    let mut lines = Vec::new();
    for line in result.Lines().map_err(failed)? {
        lines.push(line.Text().map_err(failed)?.to_string());
    }
    Ok(join_lines(&lines))
}

/// Characters that Japanese, Chinese and Korean text is written in, which are
/// not separated by spaces.
fn is_wide(c: char) -> bool {
    matches!(c,
        '\u{1100}'..='\u{11FF}'
        | '\u{3000}'..='\u{30FF}'
        | '\u{3130}'..='\u{318F}'
        | '\u{3400}'..='\u{4DBF}'
        | '\u{4E00}'..='\u{9FFF}'
        | '\u{AC00}'..='\u{D7AF}'
        | '\u{F900}'..='\u{FAFF}'
        | '\u{FF00}'..='\u{FFEF}')
}

/// Joins recognized lines. The engine puts a space between East Asian
/// characters, which is removed; spaces next to other text are kept.
pub fn join_lines(lines: &[String]) -> String {
    lines
        .iter()
        .map(|line| tidy(line))
        .collect::<Vec<_>>()
        .join("\r\n")
}

fn tidy(line: &str) -> String {
    let chars: Vec<char> = line.trim().chars().collect();
    let mut out = String::with_capacity(line.len());
    for (i, &c) in chars.iter().enumerate() {
        if c == ' ' {
            // Several spaces are looked through, so "あ  い" joins as well.
            let before = out.chars().next_back();
            let after = chars[i..].iter().find(|&&n| n != ' ');
            if before.is_some_and(is_wide) && after.is_some_and(|&n| is_wide(n)) {
                continue;
            }
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &[&str]) -> String {
        join_lines(&text.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>())
    }

    #[test]
    fn spaces_between_east_asian_characters_are_removed() {
        assert_eq!(lines(&["日 本 語 の 文 字"]), "日本語の文字");
        assert_eq!(lines(&["あ  い"]), "あい");
        assert_eq!(lines(&["カ タ カ ナ ， 全 角"]), "カタカナ，全角");
        assert_eq!(lines(&["한 국 어"]), "한국어");
    }

    #[test]
    fn spaces_next_to_other_text_stay() {
        assert_eq!(lines(&["Hello World"]), "Hello World");
        assert_eq!(lines(&["Windows 11 の 設定"]), "Windows 11 の設定");
        assert_eq!(lines(&["日本 and 語"]), "日本 and 語");
        assert_eq!(lines(&["a b 日 c"]), "a b 日 c");
    }

    #[test]
    fn lines_are_trimmed_and_joined_with_crlf() {
        assert_eq!(lines(&["  one ", "two"]), "one\r\ntwo");
        assert_eq!(lines(&["x"]), "x");
        // Boundaries: nothing, an empty line and only spaces.
        assert_eq!(lines(&[]), "");
        assert_eq!(lines(&[""]), "");
        assert_eq!(lines(&["   "]), "");
        assert_eq!(lines(&["a", "", "b"]), "a\r\n\r\nb");
        // A space at the very start or end of a line never joins with a missing neighbor.
        assert_eq!(lines(&[" 日"]), "日");
    }

    #[test]
    fn wide_character_ranges_have_exact_edges() {
        for c in [
            '\u{3000}', '\u{30FF}', '\u{4E00}', '\u{9FFF}', '\u{FF00}', '\u{FFEF}',
        ] {
            assert!(is_wide(c), "{c:?}");
        }
        for c in [
            '\u{2FFF}', '\u{3100}', '\u{4DC0}', '\u{4DFF}', '\u{A000}', '\u{FFF0}', 'A', ' ', '1',
        ] {
            assert!(!is_wide(c), "{c:?}");
        }
    }

    #[test]
    fn invalid_images_fail_without_calling_the_engine() {
        for (len, width, height) in [
            (0, 0, 0),
            (16, 0, 4),
            (16, 4, 0),
            (16, -2, -2),
            (12, 2, 2),
            (20, 2, 2),
        ] {
            assert_eq!(recognize(&vec![0; len], width, height), Err(Error::Failed));
        }
    }

    /// Black text on white as BGRA pixels, drawn the way the app draws its own text.
    fn render(text: &str) -> Vec<u8> {
        use windows_sys::Win32::Graphics::Gdi::{DT_LEFT, DT_SINGLELINE, DT_VCENTER, FW_NORMAL};
        // SAFETY: the canvas and the font are local GDI objects.
        unsafe {
            let mut canvas = crate::ui::Canvas::new(700, 120).expect("Create a canvas");
            canvas.pixels().fill(0xffffffff);
            let font = crate::ui::Font::new(crate::ui::TEXT_FONT, 48.0, FW_NORMAL as i32);
            canvas.text(
                text,
                &font,
                crate::ui::rect(10.0, 0.0, 690.0, 120.0),
                DT_LEFT | DT_VCENTER | DT_SINGLELINE,
                0xff000000,
            );
            canvas
                .pixels()
                .iter()
                .flat_map(|pixel| (*pixel | 0xff000000).to_le_bytes())
                .collect()
        }
    }

    #[test]
    #[ignore = "Requires a Windows text recognition language"]
    fn reads_rendered_text() {
        let text = spawn(render("Hello World 123"), 700, 120)
            .recv()
            .unwrap()
            .expect("Recognize");
        // The engine may read the letter l of Hello as the digit 1, so only other words are checked.
        assert!(text.contains("World"), "{text:?}");
        assert!(text.contains("123"), "{text:?}");
    }

    /// このユーザーの文字認識の言語が日本語か。アプリはユーザーの言語から文字認識の機能を作るので、
    /// 日本語を読むには、Windows の言語の一覧で日本語が先頭にある必要がある。英語のユーザー（GitHub
    /// Actions のランナー）では、日本語の文字は読めない。
    fn recognizes_japanese() -> bool {
        std::thread::spawn(|| {
            // SAFETY: a plain WinRT apartment for this thread.
            unsafe {
                let _ = RoInitialize(RO_INIT_MULTITHREADED);
                let japanese = OcrEngine::TryCreateFromUserProfileLanguages()
                    .and_then(|engine| engine.RecognizerLanguage())
                    .and_then(|language| language.LanguageTag())
                    .is_ok_and(|tag| tag.to_string().starts_with("ja"));
                RoUninitialize();
                japanese
            }
        })
        .join()
        .unwrap_or(false)
    }

    #[test]
    #[ignore = "Requires the Japanese Windows text recognition language"]
    fn reads_rendered_japanese_without_spaces_between_characters() {
        if !recognizes_japanese() {
            eprintln!("skipped: the text recognition language of this user is not Japanese");
            return;
        }
        let text = spawn(render("日本語の文字認識"), 700, 120)
            .recv()
            .unwrap()
            .expect("Recognize");
        assert!(text.contains("日本語"), "{text:?}");
        assert!(!text.contains(' '), "{text:?}");
    }
    #[test]
    #[ignore = "Requires a Windows text recognition language"]
    fn a_blank_image_has_no_text() {
        let text = spawn(vec![255; 200 * 100 * 4], 200, 100).recv().unwrap();
        assert_eq!(text.map(|t| t.trim().to_owned()), Ok(String::new()));
    }

    #[test]
    #[ignore = "Requires a Windows text recognition language"]
    fn an_image_beyond_the_engine_limit_is_refused() {
        // One pixel wider than the largest image the engine accepts.
        let limit = OcrEngine::MaxImageDimension().unwrap() as i32;
        let width = limit + 1;
        let result = spawn(vec![255; width as usize * 4], width, 1)
            .recv()
            .unwrap();
        assert_eq!(result, Err(Error::TooLarge));
    }
}
