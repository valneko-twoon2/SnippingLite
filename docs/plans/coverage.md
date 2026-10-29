# Test coverage plan

Goal: 95% line coverage of the application code, measured by `pwsh -File scripts/coverage.ps1`.
The script runs the unit tests together with the ones marked `#[ignore]` (they need a desktop,
the clipboard or an audio device) and fails below the threshold. The end-to-end tests
(`src/tests/e2e.rs`) and the test helpers (`test_support`) are not measured.

| Stage | Scope | State |
|---|---|---|
| 0 | `scripts/coverage.ps1`, shared test sandbox (`src/test_support.rs`) | done |
| 1 | Pure logic and file I/O: config, history, imaging, ui, ui/canvas, window_capture, record | done |
| 2 | egui screens run without a window: settings_ui, history_ui | done |
| 3 | Win32 window procedures driven in-process: toast, overlay, toolbar, screenshot, video, main | done |
| 4 | Close the remaining gaps, document the workflow | done |

Baseline before stage 0: 18% (33% with the desktop tests). After stage 3: 96.3% of 8,370 lines (311 missed).

What is still not covered (about 311 lines): the entry points that open a real eframe window\n(main, settings_ui::run, history_ui::run), branches that need a failing Windows API (device\nand GDI+ errors, a missing AAC encoder), and the per-process audio loopback, which is checked by\n
ecords_system_audio_and_only_the_chosen_app. That test needs a sound output that records\nper-process audio, so scripts/coverage.ps1 skips it; run it by hand on a machine that has one.\nA failing test makes the coverage data of the whole run unusable, which is why the script\nretries a failed run once.
