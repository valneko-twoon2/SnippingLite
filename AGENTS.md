# SnippingLite agent instructions

SnippingLite is a native Windows screenshot utility. The Rust project lives in `src/`; run Cargo with `--manifest-path src/Cargo.toml` and, except for `fmt`, `--config src/.cargo/config.toml` from the repository root.

Read `.agents/skills/snippinglite-validation/SKILL.md` before implementation. Keep `.claude/skills/` in sync with `.agents/skills/`. Use English for code comments, commits and technical documentation; reply to the user in their language. Keep the English and Japanese READMEs aligned.

Record `git status --short` before work and preserve user changes. Use UTF-8 and CRLF for text except the LF-only Git hook launchers. Do not bypass hooks. Before committing run `pwsh -File scripts/validate.ps1`, stage only task files, and create an English Conventional Commit without attribution trailers. Run `pwsh -File scripts/complete-task.ps1` before reporting completion. Report validation and any remaining user changes honestly.

All UI text must be vertically centered in its row, button, or box: use `*_CENTER` anchors in egui and `DT_VCENTER` in Win32 text, and keep the Japanese font `y_offset_factor` tweak in `install_fonts`. `scripts/check-text-alignment.ps1` (run by `validate.ps1`) rejects top or bottom anchors; mark a justified exception with `// align-ok: <reason>`. After changing UI, look at a screenshot of the window and confirm the text is centered.

Do not panic in Win32 callbacks. Keep debug information and incremental compilation disabled. Preserve the static CRT and control-flow guard release configuration. Run `cargo audit` when dependencies change. Do not add dependencies or tests without a concrete need.

Keep every source file (.rs, .ps1, .cmd) at 500 lines or fewer. scripts/check-file-length.ps1 runs in alidate.ps1 and in the Stop hook, and a Claude Code hook runs it after each edit, so an over-long file blocks the commit and the end of the task. Split the file by responsibility instead of trimming it: move a cohesive group of items or the mod tests block into a child module (oo.rs plus oo/bar.rs), and split long test files by topic into 	ests/<topic>.rs. Do not delete tests or comments to fit. A file that cannot be split sensibly may carry a length-ok: <reason> marker in its first five lines.
