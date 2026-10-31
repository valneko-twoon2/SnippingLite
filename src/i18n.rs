//! Translations. The source strings in the code are English and also serve as
//! the lookup keys; a language without a table entry falls back to English.

#[derive(Clone, Copy, PartialEq)]
pub enum Language {
    /// The source strings themselves; selected when `LANGUAGE` is switched back.
    #[allow(dead_code)]
    English,
    Japanese,
}

/// Only Japanese ships for now. To add a language, extend `Language`, add a
/// table like `JAPANESE` and select it in `table`.
pub const LANGUAGE: Language = Language::Japanese;

/// Placeholders such as `{n}` are replaced by the caller with `str::replace`.
const JAPANESE: &[(&str, &str)] = &[
    // Settings window
    (
        "Capture screenshots with Alt + Shift + S",
        "Alt + Shift + S でスクリーンショットを撮影",
    ),
    ("New snip", "新規切り取り"),
    // クイックマークアップのバー
    ("Capture", "キャプチャ"),
    ("Snipping", "領域の切り取り"),
    ("Screen recording", "画面録画"),
    ("Shortcuts", "ショートカット"),
    ("On", "オン"),
    ("Off", "オフ"),
    ("Change", "変更"),
    ("Open folder", "フォルダーを開く"),
    ("Open", "開く"),
    ("Choose a folder", "フォルダーの選択"),
    ("Screenshots", "スクリーンショット"),
    ("Screen Recordings", "画面録画"),
    ("Screenshot folder", "スクリーンショットの保存先"),
    ("Recording folder", "画面録画の保存先"),
    ("Automatically copy screenshots", "自動的に変更にコピー"),
    (
        "Screenshots are copied to the clipboard automatically",
        "スクリーンショットの編集内容はクリップボードに自動的にコピーされます",
    ),
    (
        "Automatically save screenshots",
        "元のスクリーンショットを自動的に保存する",
    ),
    (
        "Choose a file name when saving",
        "保存時にファイル名を指定する",
    ),
    (
        "Keep a history of screenshots",
        "スクリーンショットの履歴を残す",
    ),
    ("History size", "履歴の保存件数"),
    ("{n} screenshots", "{n} 件"),
    ("Appearance", "外観"),
    ("App theme", "アプリ テーマ"),
    ("Use system setting", "システム設定を使用する"),
    ("Light", "ライト"),
    ("Dark", "ダーク"),
    ("Show notifications", "通知を表示する"),
    (
        "Keep the toolbar open after a snip",
        "撮影後もツールバーを表示したままにする",
    ),
    (
        "Close it with the × button. Drag an empty part to move it",
        "× ボタンで閉じます。ボタン以外の部分をドラッグすると移動できます",
    ),
    (
        "Include microphone input in recordings",
        "画面録画の開始時に既定でマイク入力を含める",
    ),
    (
        "Include system audio in recordings",
        "画面録画の開始時に既定でシステム オーディオを含める",
    ),
    (
        "Record only the sound of the window's app",
        "ウィンドウ録画ではそのアプリの音だけを録音する",
    ),
    (
        "Turn off to record all system audio when recording a window",
        "オフにすると、ウィンドウ録画でもシステム全体の音を録音します",
    ),
    ("Automatically copy recordings", "自動的に変更にコピー"),
    (
        "Recordings are copied to the clipboard automatically",
        "画面録画の編集内容は自動的にクリップボードにコピーされます",
    ),
    (
        "Automatically save recordings",
        "元の画面録画を自動的に保存する",
    ),
    (
        "Choose at least one destination for recordings.",
        "画面録画の保存先を少なくとも 1 つ選んでください。",
    ),
    (
        "After a screenshot or recording is saved",
        "スクリーンショットや録画を保存したあとに表示します",
    ),
    ("Countdown before recording", "録画前にカウントダウンする"),
    (
        "Include the mouse cursor in recordings",
        "録画にマウスカーソルを含める",
    ),
    (
        "Unsupported recording frame rate.",
        "録画のフレームレートが無効です。",
    ),
    (
        "Dim and freeze the screen while snipping",
        "切り取り中に画面を暗くして静止する",
    ),
    (
        "Turn off to keep videos and apps running while you select",
        "オフにすると、動画やアプリを動かしたまま範囲を選べます",
    ),
    (
        "Leave out the title bar when recording a window",
        "ウィンドウを録画するときタイトルバーを含めない",
    ),
    (
        "Click a window to record it",
        "クリックでウィンドウを録画できます",
    ),
    ("Open history", "履歴を開く"),
    (
        "Choose at least one destination for screenshots.",
        "保存先を少なくとも 1 つ選んでください。",
    ),
    (
        "Start a snip with Alt + Shift + S",
        "Alt + Shift + S で切り取りを開始する",
    ),
    (
        "Shortcut unavailable: another application is using it.",
        "ショートカットを使用できません: 別のアプリが使用しています。",
    ),
    // History window
    ("Settings", "設定"),
    ("History", "履歴"),
    ("All", "すべて"),
    ("Last 24 hours", "24 時間以内"),
    ("Last 7 days", "7 日以内"),
    ("Last 30 days", "30 日以内"),
    ("Cancel", "キャンセル"),
    ("Delete all", "すべて削除"),
    (
        "Delete every screenshot in the history?",
        "履歴のスクリーンショットをすべて削除しますか？",
    ),
    ("Clear", "クリア"),
    ("No screenshots yet", "スクリーンショットはまだありません"),
    (
        "{total} screenshots, newest first (the oldest beyond {limit} are removed)",
        "{total} 件（新しい順。{limit} 件を超えた古いものは削除されます）",
    ),
    (
        "{shown} of {total} screenshots match",
        "{total} 件中 {shown} 件が一致",
    ),
    (
        "History recording is turned off in Settings.",
        "履歴の記録は設定でオフになっています。",
    ),
    (
        "Search by date, e.g. 2026/10/28",
        "日付で検索（例: 2026/10/28）",
    ),
    (
        "Captures appear here after you take them.",
        "撮影したものがここに表示されます。",
    ),
    (
        "No screenshots match.",
        "一致するスクリーンショットはありません。",
    ),
    (
        "Screenshot {date} {time}",
        "スクリーンショット {date} {time}",
    ),
    ("Could not open the image.", "画像を開けませんでした。"),
    ("Back", "戻る"),
    ("Delete", "削除"),
    ("Show in folder", "フォルダーで表示"),
    ("Save as...", "名前を付けて保存..."),
    ("Copy", "コピー"),
    ("Pen", "ペン"),
    ("Highlighter", "蛍光ペン"),
    ("Crop", "切り抜き"),
    (
        "Add a border to screenshots",
        "各スクリーンショットに境界を追加",
    ),
    ("Border color", "色"),
    ("Border thickness", "太さ"),
    ("About this app", "このアプリについて"),
    (
        "Review third-party software",
        "サードパーティ製ソフトウェアの確認",
    ),
    ("Send feedback", "フィードバックの送信"),
    ("Red", "赤"),
    ("Yellow", "黄"),
    ("Green", "緑"),
    ("Blue", "青"),
    ("Black", "黒"),
    ("Pick color", "色の取得"),
    ("Copied the color.", "色をコピーしました。"),
    ("Could not copy the color.", "色をコピーできませんでした。"),
    ("Copy text", "文字をコピー"),
    ("Recognizing text...", "文字を認識しています..."),
    ("Copied the text.", "文字をコピーしました。"),
    ("Could not copy the text.", "文字をコピーできませんでした。"),
    (
        "No text was found in the image.",
        "画像に文字が見つかりませんでした。",
    ),
    (
        "Add a text recognition language in the Windows settings.",
        "Windows の設定で、文字認識の言語を追加してください。",
    ),
    (
        "The image is too large to recognize text.",
        "画像が大きすぎて文字を認識できません。",
    ),
    (
        "Could not recognize text in the image.",
        "画像の文字を認識できませんでした。",
    ),
    ("Thin", "細"),
    ("Medium", "中"),
    ("Thick", "太"),
    ("Color", "色"),
    ("Apply crop", "切り抜きを適用"),
    ("Undo", "元に戻す"),
    ("Save changes", "変更を保存"),
    (
        "Left and right arrow keys browse, Esc goes back.",
        "左右の矢印キーで前後の画像へ移動、Esc で一覧に戻ります。",
    ),
    (
        "Copied to the clipboard.",
        "クリップボードにコピーしました。",
    ),
    (
        "Could not copy the image.",
        "画像をコピーできませんでした。",
    ),
    ("Saved the image.", "画像を保存しました。"),
    ("Could not save the image.", "画像を保存できませんでした。"),
    ("Saved the changes.", "変更を保存しました。"),
    (
        "Could not save the changes.",
        "変更を保存できませんでした。",
    ),
    ("Rectangle", "四角形"),
    ("Freeform", "フリーフォーム"),
    ("Window", "ウィンドウ"),
    ("Full screen", "全画面表示"),
    ("Markup and share", "マークアップと共有"),
    ("Dismiss notification", "通知を閉じる"),
    // Notification
    (
        "Snip copied to clipboard",
        "スクリーンショットをクリップボードにコピーしました",
    ),
    ("Saved as {name}", "{name} として保存しました"),
    (
        "Saved automatically to the Screenshots folder",
        "スクリーンショット フォルダーに自動的に保存されました。",
    ),
    (
        "Paste it into a document or app",
        "ドキュメントやアプリに貼り付けできます",
    ),
    ("Snip saved", "切り取りを保存しました"),
    ("Recording saved", "録画を保存しました"),
    (
        "Recording saved under another name",
        "録画を別の名前で保存しました",
    ),
    (
        "Recording copied to clipboard",
        "録画をクリップボードにコピーしました",
    ),
    (
        "Could not copy the recording to the clipboard.",
        "録画をクリップボードにコピーできませんでした。",
    ),
    // Tray menu, recording and capture messages
    ("Stop recording", "録画を停止"),
    ("Exit", "終了"),
    ("About", "About"),
    ("Credits", "Credits"),
    ("Version", "バージョン"),
    ("License", "ライセンス"),
    (
        "{n} open source libraries",
        "{n} 個のオープンソース ライブラリ",
    ),
    ("Homepage", "ホームページ"),
    ("Back", "戻る"),
    (
        "Could not open the About page.",
        "About ページを開けませんでした。",
    ),
    ("Stop", "停止"),
    ("Play", "再生"),
    ("Pause", "一時停止"),
    ("Seek", "シーク"),
    ("Volume", "音量"),
    (
        "This video cannot be played.",
        "このビデオは再生できません。",
    ),
    (
        "Could not capture the desktop.",
        "デスクトップを取り込めませんでした。",
    ),
    (
        "Could not create the capture.",
        "切り取りを作成できませんでした。",
    ),
    (
        "Could not copy the selected pixels.",
        "選択範囲のピクセルをコピーできませんでした。",
    ),
    (
        "Could not copy the image to the clipboard.",
        "画像をクリップボードにコピーできませんでした。",
    ),
    (
        "Could not create the recordings folder.",
        "録画の保存フォルダーを作成できませんでした。",
    ),
    (
        "The selected area is too small to record.",
        "選択範囲が小さすぎて録画できません。",
    ),
    ("The recorder crashed.", "録画処理が異常終了しました。"),
    (
        "Could not prepare the screen capture.",
        "画面の取り込みを準備できませんでした。",
    ),
    (
        "Could not start the video encoder",
        "動画エンコーダーを開始できませんでした",
    ),
    (
        "Could not write a video frame",
        "動画のフレームを書き込めませんでした",
    ),
    (
        "Could not finish the video file",
        "動画ファイルを完了できませんでした",
    ),
    (
        "Recording ended early; the video up to that point was saved.",
        "録画が途中で終了しました。ここまでの動画は保存されています。",
    ),
    (
        "Could not capture the selected window.",
        "選択したウィンドウを取得できませんでした。",
    ),
    // File dialog
    ("PNG image", "PNG 画像"),
    ("BMP image", "BMP 画像"),
];

fn table() -> &'static [(&'static str, &'static str)] {
    match LANGUAGE {
        Language::English => &[],
        Language::Japanese => JAPANESE,
    }
}

/// The text in the active language, or the English source when it has no entry.
pub fn tr(text: &'static str) -> &'static str {
    table()
        .iter()
        .find(|(english, _)| *english == text)
        .map_or(text, |(_, translated)| translated)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every `tr("...")` in the sources needs a table entry, or the text would
    /// silently stay English.
    #[test]
    fn every_translated_string_has_an_entry() {
        let sources = [
            include_str!("main.rs"),
            include_str!("screenshot.rs"),
            include_str!("toolbar.rs"),
            include_str!("video.rs"),
            include_str!("config.rs"),
            include_str!("settings_ui.rs"),
            include_str!("settings_ui/app.rs"),
            include_str!("settings_ui/app/sections.rs"),
            include_str!("settings_ui/expander.rs"),
            include_str!("tray.rs"),
            include_str!("markup.rs"),
            include_str!("markup/bar.rs"),
            include_str!("settings_ui/widgets.rs"),
            include_str!("history_ui.rs"),
            include_str!("history_ui/detail.rs"),
            include_str!("history_ui/gallery.rs"),
            include_str!("history_ui/widgets.rs"),
            include_str!("toast.rs"),
            include_str!("record.rs"),
            include_str!("imaging.rs"),
            include_str!("ocr.rs"),
        ];
        let mut found = 0;
        for source in sources {
            for (start, _) in source.match_indices("tr(\"") {
                let before = source[..start].chars().next_back();
                if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                    continue;
                }
                let text = &source[start + 4..];
                let text = &text[..text.find('"').unwrap()];
                assert!(
                    JAPANESE.iter().any(|(english, _)| *english == text),
                    "missing translation: {text}"
                );
                found += 1;
            }
        }
        assert!(found > 50);
    }
}
