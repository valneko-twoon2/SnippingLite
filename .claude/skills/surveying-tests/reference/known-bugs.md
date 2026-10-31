# 既知の不具合の印の付け方

この作業で直さない不具合をテストが示したときは、テストを残して印を付ける。スイートは緑のまま、不具合は見えたままになる。理由には `known bug:` と何が間違っているかを書く。

| ランナー | 印 |
|---|---|
| node:test | `test('…', { todo: 'known bug: …' }, fn)`（実行され、todo として報告される。実行全体は失敗にならず、不具合を直したあとも失敗にならない。通っている todo テストを探すこと） |
| vitest / jest | `it.fails('…', fn)` / `test.failing('…', fn)`（不具合を直すと実行全体が失敗する。そのときは印を外す） |
| pytest | `@pytest.mark.xfail(strict=True, raises=AssertionError, reason='known bug: …')`（strict: 不具合を直すと失敗する。raises: エラーを不具合と取り違えない） |
| unittest | `@unittest.expectedFailure`（理由を書く欄が無いので docstring に書く。どんな例外でも想定した失敗として扱われる） |
| cargo test | `#[ignore = "known bug: …"]`（想定した失敗の印は無い。無視として表示される） |
| go test | `t.Skip("known bug: …")` |
| JUnit / xUnit | `@Disabled("known bug: …")` / `[Fact(Skip = "known bug: …")]` |

- ほとんどの印は、どんな失敗でも通る。打ち間違いや誤ったインポートも含む。印を外してテストを一度実行し、不具合の検証部分で失敗することを読んで確かめる。
- 仕様どおりの振る舞いだけが通る入力を選ぶ。同じ規則のほかの上限から離す。修正の惜しい誤り（`>=` のつもりで `>`）でも、そのテストは失敗するようにする。
- 性質に印を付けると、その性質のほかのすべての失敗が隠れる。失敗するケースを例として固定してそちらに印を付け、性質は不具合に関わらない入力で動かし続ける。
