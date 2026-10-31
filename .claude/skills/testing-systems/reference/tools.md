# エコシステム別の E2E、スナップショット、監査のツール

プロジェクトにすでにあるものを使う。

## JavaScript と TypeScript

| 用途 | 既定 | コマンド |
|---|---|---|
| ブラウザーの E2E、モンキーテスト、見た目、複数ブラウザー | Playwright | `npx playwright test --project=<name>`、`toHaveScreenshot`、WebKit と Firefox のプロジェクト |
| 監査 | npm | `npm audit --omit=dev`（出荷されるもの）、ツール類は `npm audit` のみ |

## Python

| 用途 | 既定 | コマンド |
|---|---|---|
| 監査 | pip-audit | `pip-audit` |

## Rust

| 用途 | 既定 | コマンド |
|---|---|---|
| スナップショット | insta | `cargo insta review` |
| 監査 | cargo-audit、cargo-deny | `cargo audit`、`cargo deny check` |

## Go

| 用途 | 既定 | コマンド |
|---|---|---|
| 監査 | govulncheck | `govulncheck ./...` |

## C# と .NET

| 用途 | 既定 | コマンド |
|---|---|---|
| 監査 | 組み込み | `dotnet list package --vulnerable` |
