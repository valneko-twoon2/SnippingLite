# Snipping Tool settings page

Goal: the settings window looks and reads like the settings page of the Windows 11 Snipping Tool
(Japanese, dark and light), including the settings that depend on its editor window.

Reference: the user's screenshots of Snipping Tool 11.2608.41.0 (dark theme, Japanese). The
installed package has no readable Japanese resource, and this environment cannot operate the
screen, so everything not visible in those screenshots is marked "unknown" below.

## What the reference page contains

| Section | Item | Control |
|---|---|---|
| 領域の切り取り | 自動的に変更にコピー / スクリーンショットの編集内容はクリップボードに自動的にコピーされます | switch |
| | 元のスクリーンショットを自動的に保存する | switch + expander: スクリーンショットの保存先 (path, 変更 link, フォルダーを開く button) |
| | 編集した後でウィンドウを閉じる際に確認する | switch |
| | 複数のウィンドウ / 別のウィンドウで新しいスクリーンショットを開く | switch |
| | 各スクリーンショットに境界を追加 | switch + expander: preview, 色 (swatch), 太さ (slider) |
| | HDR スクリーンショットのカラー コレクター / HDR モニターで撮影したスクリーンショットの色を調整します | switch |
| 画面録画 | 自動的に変更にコピー / 画面録画の編集内容は自動的にクリップボードにコピーされます | switch |
| | 元の画面録画を自動的に保存する | switch + expander: 画面録画の保存先 |
| | 編集した画面録画の保存を依頼する | switch |
| | 画面録画の開始時に既定でマイク入力を含める | switch with a microphone icon |
| | 画面録画の開始時に既定でシステム オーディオを含める | switch with a speaker icon |
| ショートカット | PrintScreen キーを使用して Snipping Tool を開く | link: Windows の設定の変更 |
| 外観 | アプリ テーマ | expander (options unknown) |
| このアプリについて (right column) | version, copyright, license, service terms, privacy, third-party software, フィードバックの送信 | links and a button |

Switch layout: the switch, then the オン/オフ label to its right, then the chevron of an expander.

## Decisions already taken

- Our own settings (capture delay, dim and freeze, notifications, keep the toolbar, history size,
  frame rate, border colors, and so on) stay in their current sections, drawn like the other cards.
- Settings that need an editing window are implemented as functions, not only drawn.

## Stages

1. Widgets: Snipping Tool style card, expander (chevron, divider, nested rows), switch with the
   オン/オフ label to its right, slider, color swatch, link, subtitle text.
2. Wording and order of the shared items, in both sections.
3. Expanders: save folder, border (preview, color picker, thickness slider), app theme.
4. Layout: responsive two columns with the このアプリについて pane; the license and third-party
   links open the About page.
5. Functions: confirm when closing an edited screenshot, open every screenshot in its own window,
   and ask before saving an edited recording.
6. Verify in dark and light against screenshots, run `validate.ps1` and `coverage.ps1`.

## Open questions

- HDR color corrector: capture goes through GDI and has no HDR path. Needs a decision: leave it
  out, or implement tone mapping for HDR monitors.
- 編集した画面録画の保存を依頼する: there is no recording editor, so the meaning would be
  "ask whether to keep the recording when it ends". Needs confirmation.
- The back arrow of the page has nothing to go back to; planned to leave it out.
- The theme options and the rest of the 外観 section are not in the screenshots.
