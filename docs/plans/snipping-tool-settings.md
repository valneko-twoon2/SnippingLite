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

## Decisions (later)

- The HDR color corrector is left out: capture goes through GDI and has no HDR path.
- The back arrow is left out; the page has nothing to go back to.
- Theme options are Light, Dark and Use system setting. After 外観 only the license items remain.
- Stages 1-4 are done (4dcfa38). The recording editor is in scope (see below), which also gives the
  recording-side settings a meaning.

## Recording editor

Goal: after a recording ends, a window like the Snipping Tool's opens on it: video with play/pause,
elapsed/total time, seek slider, volume, trim, save, copy, and "more" (open folder, delete).

Shape: a separate process, `snippinglite --player <file>`, like `--history`, drawn with egui so it
shares the palette and the theme. History lists recordings too and opens them in the same window.

Engine, with no new dependency (only `windows` crate feature flags):

- Playback: WinRT `MediaPlayer` in frame-server mode. It decodes, plays the audio, seeks and sets the
  volume; each frame is copied to a D3D11 texture and read back into an egui texture.
- Trim and save: WinRT `MediaComposition` (`MediaClip` trimmed from start/end, rendered to MP4).
- Copy: put the file on the clipboard as a file drop (`CF_HDROP`), as Explorer does.

Stages:

1. Spike: open a recorded MP4 with `MediaPlayer`, get a frame, seek, read the duration. A test on a
   file made by the recorder proves it. Decide here whether the engine holds; if not, fall back to
   Media Foundation `IMFSourceReader` + WASAPI.
2. Player window: frame, play/pause, time, seek, volume; opened by `--player` and by the history.
3. Trim: start/end handles on the seek bar; save as a new file, or replace.
4. Save, copy, more (open folder, delete); open the window when a recording ends (setting-driven).
5. Settings that depend on it: copy a recording automatically, keep the original recording
   (with the recording-folder expander), ask before saving an edited recording.
6. Same for the screenshot editor: confirm on closing an edited screenshot, open each screenshot in
   its own window.
7. Verify with screenshots, `validate.ps1`, `coverage.ps1`.
