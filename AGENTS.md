# SnippingLite エージェント指示

SnippingLite はネイティブの Windows 向けスクリーンショットユーティリティです。Rust プロジェクトは `src/` にあります。Cargo はリポジトリのルートから `--manifest-path src/Cargo.toml` を付けて実行し、`fmt` 以外は `--config src/.cargo/config.toml` も付けます。

実装の前に `.agents/skills/snippinglite-validation/SKILL.md` を読むこと。`.claude/skills/` は `.agents/skills/` と同期を保つこと。コードのコメント、コミットメッセージ、技術文書は日本語で書き、ユーザーへの返答もユーザーの言語（日本語）で行う。英語版と日本語版の README は内容をそろえる（README は現行どおり英語版・日本語版の両方を維持し、日本語化の対象にしない）。

作業前に `git status --short` を記録し、ユーザーの変更を保全すること。テキストは UTF-8 と CRLF を使う（LF 専用の Git フックのランチャーは除く）。フックを回避しない。全体の検証（`pwsh -File scripts/validate.ps1`）は GitHub Actions の CI（`.github/workflows/test.yml`）が push のたびに走らせ、ローカルのコミットや push のフックでは走らせない。コミット前には、変更した範囲に関わるテスト（該当する `cargo test` の絞り込み、変更したスクリプトの実行、`cargo fmt --check`、UI なら目視）を手動で走らせ、結果を報告する。作業対象のファイルだけをステージして、Conventional Commit 形式（種別は `feat:` などの英語、件名と本文は日本語）でコミットする。帰属表記のトレーラー（`Co-Authored-By` など）は付けない。`src/` 以下の変更は、1 つ直すたびにコミットする。未コミットの変更が残っていると、Stop フック（`scripts/agent-stop.ps1`）がタスクの終了を止める。完了を報告する前に `pwsh -File scripts/complete-task.ps1` を実行する。検証結果と残っているユーザーの変更は正直に報告する。

ユーザーが箇条書きなどで複数の依頼をまとめて出したときは、途中で確認のために止まらず、すべてを最後まで進める。確認が必要なのは、取り消しにくい操作や外部に影響する操作（リモートへの push、削除など）で、あらかじめ許可されていない場合だけにする。迷う点は妥当な既定値を選び、最後にその内容を報告する。

すべての UI テキストは、行・ボタン・ボックスの中で上下中央に配置すること。egui では `*_CENTER` アンカー、Win32 のテキストでは `DT_VCENTER` を使い、`install_fonts` の日本語フォント `y_offset_factor` の調整は残す。`scripts/check-text-alignment.ps1`（`validate.ps1` が実行する）は上端・下端のアンカーを拒否する。正当な例外には `// align-ok: <理由>` を付ける。UI を変更したら、ウィンドウのスクリーンショットを見て、テキストが中央にあることを確認する。

Win32 コールバックの中で panic しない。デバッグ情報とインクリメンタルコンパイルは無効のままにする。静的 CRT と control-flow guard のリリース設定を維持する。依存関係を変えたら `cargo audit` を実行する。具体的な必要がないまま依存関係やテストを増やさない。

すべてのソースファイル（.rs、.ps1、.cmd）は 500 行以下に保つ。`scripts/check-file-length.ps1` は `validate.ps1` と Stop フックで実行され、Claude Code のフックも編集のたびに実行するため、長すぎるファイルはコミットとタスクの終了を妨げる。行数を削って収めるのではなく、責務ごとにファイルを分割する。まとまった項目群や `mod tests` ブロックは子モジュール（`foo.rs` と `foo/bar.rs`）へ移し、長いテストファイルは話題ごとに `tests/<topic>.rs` へ分ける。収めるためにテストやコメントを削らない。分割しようがないファイルには、先頭 5 行以内に `length-ok: <理由>` のマーカーを付けてよい。
