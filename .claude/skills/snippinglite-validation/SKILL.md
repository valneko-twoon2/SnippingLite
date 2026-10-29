---
name: snippinglite-validation
description: Validate SnippingLite changes before committing.
---

# SnippingLite validation

Run `pwsh -File scripts/validate.ps1` before committing. For capture or UI changes, build the release binary and exercise capture, cancellation, clipboard output, shortcut registration, and multiple monitors where available. Report untested behavior honestly. The end-to-end tests do this for the main flows: `cargo test --manifest-path src/Cargo.toml --config src/.cargo/config.toml --test e2e -- --ignored --test-threads=1` starts the app with temporary settings and folders, covers the primary monitor with a known backdrop and drives it with the mouse and keyboard. Close Snipping Lite and keep your hands off the mouse while they run; run them after capture, toolbar, recording or history window changes. Run `pwsh -File scripts/complete-task.ps1` before reporting completion.

Text alignment: `scripts/check-text-alignment.ps1` runs as part of `validate.ps1` and fails on top or bottom text anchors, `canvas.text` without `DT_VCENTER`, or a removed Japanese font offset. For UI changes also capture the settings and history windows and check by eye that every label is vertically centered against its toggle, button or row; fix the cause (anchor or font offset) instead of adding `// align-ok`.

Coverage: pwsh -File scripts/coverage.ps1 measures line coverage of the application code (95% is the goal and the default threshold) and runs the unit tests together with the ones marked #[ignore]. Those create windows, cover the screen, use the clipboard, show dialogs and menus, and open Explorer, so close Snipping Lite, keep your hands off the mouse and keyboard, and do not run it on a machine that must stay usable. Each of these tests builds the real tray state (	est_support::desk::Desk) and plays the part of the message loop and the mouse; the egui screens are driven without a window (	est_support::screen::Screen). The end-to-end tests and the test helpers are not measured.

File length: `scripts/check-file-length.ps1` runs as part of `validate.ps1` and in the Stop hook, and fails when a `.rs`, `.ps1` or `.cmd` file has more than 500 lines. Claude Code also runs it after each Edit or Write and the hook reports the file at once. Split the file by responsibility (a child module file, or tests split by topic under `tests/`) and keep every test; do not shorten it by deleting tests or comments. Run `cargo test --no-run` after a split, because a moved item may need `pub(super)` or `pub(crate)` and a `use child::*;` in the parent.
