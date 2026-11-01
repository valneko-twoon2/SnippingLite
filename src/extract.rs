//! 文字の抽出: ツールバーの文字のボタンがオンのとき、切り取った範囲の文字を Windows の文字認識で
//! 読み取り、画像の代わりにクリップボードへ文字を置く（Snipping Tool の「テキスト アクション」）。

use super::*;

/// 切り取りを画像ではなく文字として届けるか。録画には使わない。
pub(crate) unsafe fn wanted() -> bool {
    state().extract && !state().video
}

/// 切り取った画像（上から下へ並ぶ BGRA）の文字を読み取り、クリップボードへ置く。読み取れない
/// とき、文字がないとき、置けないときは、その理由を知らせる。
pub(crate) unsafe fn deliver(pixels: &[u8], width: i32, height: i32) {
    let owner = state().owner;
    let result = ocr::spawn(pixels.to_vec(), width, height)
        .recv()
        .unwrap_or(Err(ocr::Error::Failed));
    match result {
        Ok(text) if text.trim().is_empty() => {
            message(owner, tr("No text was found in the image."));
        }
        Ok(text) => {
            if !imaging::copy_text(owner, &text) {
                message(owner, tr("Could not copy the text."));
            }
        }
        Err(error) => message(owner, error.message()),
    }
}

#[cfg(test)]
mod tests;
