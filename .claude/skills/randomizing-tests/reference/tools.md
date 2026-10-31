# 言語別の性質ベース・ファジングのツール

プロジェクトにすでにあるものを使う。通常のテスト内のシード付きループならライブラリは要らない（randomized.md）。

## JavaScript と TypeScript

| 用途 | 既定 | コマンド |
|---|---|---|
| 性質ベース、モデルベース | fast-check | `fc.assert(fc.property(...), { seed, numRuns })`、モデルベースは `fc.commands` |

## Python

| 用途 | 既定 | コマンド |
|---|---|---|
| 性質ベース、ステートフル | Hypothesis | `@given(...)`、`@settings(derandomize=True)` または `--hypothesis-seed`、`RuleBasedStateMachine` |
| ファジング | 長いカバレッジ誘導の実行には Atheris。テスト群にはシード付きループまたは Hypothesis | |

## Rust

| 用途 | 既定 | コマンド |
|---|---|---|
| 性質ベース、ステートフル | proptest | 固定の `ProptestConfig` を使う `proptest!`、深い実行には `PROPTEST_CASES`、proptest-state-machine |
| ファジング | stable では通常のテスト内のシード付きループ。長い実行には cargo-fuzz（nightly、libFuzzer） | `cargo +nightly fuzz run <target>` |

## Go

| 用途 | 既定 | コマンド |
|---|---|---|
| 性質ベース | `testing/quick` または rapid | |
| ファジング | 標準搭載（Go 1.18 以降） | `func FuzzX(f *testing.F)`、シードは `f.Add`、`go test -fuzz=FuzzX -fuzztime=60s`。見つかった入力は `testdata/fuzz/` に入り、以後の `go test` のたびに走る |

## Java と Kotlin

| 用途 | 既定 | コマンド |
|---|---|---|
| 性質ベース | jqwik | `@Property` |
| ファジング | Jazzer | `@FuzzTest` |

## C# と .NET

| 用途 | 既定 | コマンド |
|---|---|---|
| 性質ベース | FsCheck または CsCheck | |
| ファジング | SharpFuzz | |
