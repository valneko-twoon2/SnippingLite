# SnippingLite

開発中の Windows ネイティブスクリーンショットツールです。`cargo run --config src/.cargo/config.toml --manifest-path src/Cargo.toml` で起動し、 **New capture** または **Alt+Shift+S** を押して範囲をドラッグすると、画像をクリップボードにコピーします。Escape でキャンセルします。ショートカットが別のアプリで使用中の場合は登録できません。

現在は矩形範囲のクリップボードへのコピーのみ対応しています。Snipping Tool の画面の完全再現、設定、その他の切り取りモード、画像編集は未実装です。
