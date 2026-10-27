# SnippingLite

A native Windows screenshot utility in early development. Run with `cargo run --config src/.cargo/config.toml --manifest-path src/Cargo.toml`. Use **New capture** or **Alt+Shift+S**, drag a rectangle, and paste the resulting bitmap. Press Escape to cancel. The shortcut cannot be registered if another application already owns it.

Currently only rectangular clipboard capture is implemented; the full Snipping Tool interface, settings, other capture modes and image editing are not yet available.
