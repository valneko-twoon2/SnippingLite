# Snipping Lite

Snipping Lite is a native Windows screenshot utility. Start the release build with `pwsh -File scripts/run-latest.ps1` or run `scripts/build.cmd` for a debug build. Keep it running to use **Alt+Shift+S**. Closing the settings window leaves it in the notification area; use the tray menu to reopen settings or exit.

Select rectangle, freeform, window, or full-screen capture in the toolbar. Press Escape to cancel. Settings can automatically copy captures to the clipboard and/or save BMP files to `Pictures\Screenshots`. At least one destination must be enabled. A shortcut conflict is shown in the settings window.

The screenshot toolbar and settings are still in development and do not yet reproduce every Snipping Tool detail. Image annotation, manual Save As, capture delay, and transparent freeform output are not implemented; the outside of a freeform capture is white.
