# 言語ごとのミューテーションツール

その言語の標準ツールを優先する。何も入っていないときは `scripts/mutate.mjs` がどの言語でも代わりになる（quality.md）。

## JavaScript と TypeScript

| 用途 | 既定 | コマンド |
|---|---|---|
| ミューテーションテスト | StrykerJS | `npx stryker run`（`npx stryker init` が設定を書く）。無いときは `scripts/mutate.mjs` |

## Python

| 用途 | 既定 | コマンド |
|---|---|---|
| ミューテーションテスト | mutmut | `mutmut run` のあとで結果を見る。無いときは `scripts/mutate.mjs --check "python -m py_compile <file>"` |

## Rust

| 用途 | 既定 | コマンド |
|---|---|---|
| ミューテーションテスト | cargo-mutants | 数えるには `cargo mutants --list -f src/x.rs`、実行は `cargo mutants -f src/x.rs`（結果は `mutants.out/`。cargo のオプションは `--` の後に置く: `cargo mutants -f src/x.rs -- --offline`。テストが複数の統合テストに分かれていると最初の失敗で止まるので、新しいテストだけの得点は `--cargo-test-arg=--no-fail-fast` を付ける） |

## Go

| 用途 | 既定 | コマンド |
|---|---|---|
| ミューテーションテスト | gremlins | `gremlins unleash`。無いときは `scripts/mutate.mjs --check "go vet ./..."` |

## Java と Kotlin

| 用途 | 既定 | コマンド |
|---|---|---|
| ミューテーションテスト | PIT | `mvn org.pitest:pitest-maven:mutationCoverage`、Gradle の `pitest` プラグイン |

## C# と .NET

| 用途 | 既定 | コマンド |
|---|---|---|
| ミューテーションテスト | Stryker.NET | `dotnet stryker` |
