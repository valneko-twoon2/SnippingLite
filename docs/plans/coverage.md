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
| 3 | Win32 window procedures driven in-process: toast, overlay, toolbar, screenshot, video, main | open |
| 4 | Close the remaining gaps, document the workflow | open |

Baseline before stage 0: 18% (33% with the desktop tests). After stage 1: see the latest run of the script.

Lines that cannot be reached without breaking Windows (failing device APIs, an unavailable
GDI+, a missing AAC encoder) are listed in the report of the last stage instead of being excluded.
