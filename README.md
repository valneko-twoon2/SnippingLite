# Snipping Lite

日本語 | [English](README.en.md)

Windows 11 の Snipping Tool に似た、軽量なスクリーンショット・画面録画ツールです。通知領域に常駐し、**Alt+Shift+S** で撮影を始めます。

## 主な機能

- **スクリーンショット**: 矩形、フリーフォーム、ウィンドウ、全画面。クリップボードへのコピーと PNG の保存
- **画面録画**: 矩形かウィンドウを MP4（H.264）で録画。マウスカーソル、マイク、システムの音を選べる
- **クイックマークアップ**: 撮った範囲にペン・蛍光ペンで書き込んでから確定
- **文字の抽出**: 選んだ範囲の文字を読み取り、テキストとしてコピー
- **履歴**: 撮った画像を見返し、書き込み、切り抜き、再コピー、保存ができる
- **マルチモニター**: 並びや拡大率（DPI）が違うモニターにも対応

画面表示は日本語のみです。細かな動作は [docs/behavior.md](docs/behavior.md) にまとめています。

## インストール

PowerShell で次のコマンドを実行します。

```powershell
irm https://github.com/valneko-twoon2/SnippingLite/releases/latest/download/install.ps1 | iex
```

- 管理者権限なしで、現在のユーザー向けに `%LocalAppData%\Programs\Snipping Lite` へインストールします。
- ダウンロードの SHA-256 を確かめ、アプリを起動して、サインインのたびに起動するようにします。
- 同じコマンドをもう一度実行すると更新できます。
- 別のフォルダーやリポジトリを使うときは、`install.ps1` をダウンロードして `-Directory <フォルダー>` や `-Repo <owner/name>` を付けて実行します。

## アンインストール

Windows の「設定」>「アプリ」から削除します。

- 設定と履歴は残ります。消すときは、インストール先の `uninstall.ps1` を `-RemoveData` 付きで実行します。
- 保存したスクリーンショットと録画は削除されません。

## 開発

| 目的 | コマンド |
| --- | --- |
| デバッグ版をビルド（起動はしない） | `scripts/build.cmd` |
| ビルド済みのリリース版を起動 | `pwsh -File scripts/run-latest.ps1` |
| 変更の検証 | `pwsh -File scripts/validate.ps1` |
| リリース用のファイルを `artifacts/` に作る | `pwsh -File scripts/publish.ps1` |

クレートのバージョンと同じタグ（例: `v1.0.0`）を push すると、テストを通したうえで GitHub の Releases に公開されます。
