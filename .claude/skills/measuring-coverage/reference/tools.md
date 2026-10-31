# エコシステム別のカバレッジとテストランナー

## 目次

- JavaScript と TypeScript
- Python
- Rust
- Go
- Java と Kotlin
- C# と .NET
- カバレッジの落とし穴

## JavaScript と TypeScript

| 必要なもの | 既定 | コマンド |
|---|---|---|
| ランナー（まだ無い場合） | Node 組み込み（Node 20 以上。Node 23.6 以上は `.ts` を直接実行でき、import には `.ts` ファイルを指定する） | `node --test`（`**/*.test.{js,mjs,cjs,ts,mts}` を探す。引数なしで使う: Node 24 では `node --test test/` のようなフォルダー指定が通らない） |
| ランナー（既にある場合） | プロジェクトの vitest、jest、mocha | そのスクリプト |
| カバレッジ（node:test） | 組み込み | `node --test --experimental-test-coverage --test-reporter=spec --test-reporter-destination=stdout --test-reporter=lcov --test-reporter-destination=lcov.info`（出力先のフォルダーは先に作っておく。対象を絞るには `--test-coverage-include='src/**'`。ただし、それでも読み込まれないファイルは出ない） |
| カバレッジ（vitest） | `@vitest/coverage-v8` | `vitest run --coverage --coverage.include='src/**' --coverage.reporter=lcov --coverage.reporter=text` |
| カバレッジ（jest） | 組み込み | `jest --coverage --collectCoverageFrom='src/**/*.{js,ts,tsx}'`（lcov は `coverage/lcov.info`） |
| カバレッジ（上記以外） | c8 | `npx c8 --all --src src -r lcov -r text <test command>` |

## Python

| 必要なもの | 既定 | コマンド |
|---|---|---|
| ランナー | あれば pytest、なければ標準の `unittest` | `python -m pytest -q` / `python -m unittest discover -s tests -t .` |
| カバレッジ | coverage.py | `coverage run --branch --source=<package> -m pytest` の後に `coverage report -m` と `coverage lcov -o lcov.info`（または `coverage xml`） |
| カバレッジ（何も入っていない場合） | 標準の `trace`（行のみ） | `python -m trace --count --missing --coverdir=.trace --module unittest discover -s tests -t .` の後に `coverage-gaps.mjs .trace --expect <package>`。`--ignore-dir` は付けない（カバレッジの落とし穴を参照）。Python 3.12 より前では標準ライブラリで分岐を測れない（3.12 以降は `sys.monitoring` の BRANCH イベント）。その場合は「測定不可」と報告する |

## Rust

| 必要なもの | 既定 | コマンド |
|---|---|---|
| ランナー | cargo | `cargo test`（プロジェクトが使っているなら `cargo nextest run`） |
| カバレッジ | cargo-llvm-cov | `cargo llvm-cov --lcov --output-path lcov.info` と `cargo llvm-cov --summary-only`。分岐には nightly が要る: `cargo +nightly llvm-cov --branch` |

## Go

| 必要なもの | 既定 | コマンド |
|---|---|---|
| ランナー | go test | `go test ./...` |
| カバレッジ | 組み込み（文単位、分岐なし） | `go test -coverprofile=cover.out -covermode=count ./...`、`go tool cover -func=cover.out` |

## Java と Kotlin

| 必要なもの | 既定 | コマンド |
|---|---|---|
| ランナー | JUnit 5 | `mvn test` / `gradle test` |
| カバレッジ | JaCoCo（行と分岐） | プラグインを入れて `mvn verify`（`target/site/jacoco/jacoco.xml`）、`gradle jacocoTestReport` |

## C# と .NET

| 必要なもの | 既定 | コマンド |
|---|---|---|
| ランナー | プロジェクトの xUnit、NUnit、MSTest | `dotnet test` |
| カバレッジ | coverlet | `dotnet test --collect:"XPlat Code Coverage"`（Cobertura は `TestResults/*/coverage.cobertura.xml`） |

## カバレッジの落とし穴

- 多くのツールは、テストが読み込んだファイルだけを数える。Node 組み込みのカバレッジは、
  `--test-coverage-include` を付けても読み込まれないファイルを外す。jest には `collectCoverageFrom`、
  vitest には `coverage.include`、c8 には `--all`、coverage.py には `--source` が要る。
  `scripts/coverage-gaps.mjs <report> --expect src` は、レポートに名前が出ないファイルを挙げる。
- 行カバレッジは、条件のある行を一度実行されれば網羅済みと数える。分岐の列を読むこと。
  `a && b` については条件の組み合わせを読む（case-design.md）。
- テストファイルや生成コードのカバレッジは合計を水増しする。実行からではなく、測定対象から外す。
- Python の `trace --ignore-dir` は、プロジェクトの `__init__.py` を落とす。判定をモジュール名だけで保持するため、
  一つの `__init__` を無視すると、ほかのすべても無視される。付けないこと。coverage-gaps.mjs は `--root` の下の
  モジュールだけを残す。
- 合計に対するカバレッジのゲートは、新しい未テストのファイルを古いテスト済みのファイルの陰に隠す。
  ファイルごとの表を読むこと。
