# Snipping Lite

Snipping Lite is a native Windows screenshot utility. Start the release build with `pwsh -File scripts/run-latest.ps1` or run `scripts/build.cmd` for a debug build. Keep it running to use **Alt+Shift+S**. Closing the settings window leaves it in the notification area; use the tray menu to reopen settings or exit.

Select rectangle, freeform, window, or full-screen capture in the toolbar. Press Escape to cancel. Settings can automatically copy captures to the clipboard and/or save PNG files to `Pictures\Screenshots`. Enable "Choose a file name when saving" to pick the name and PNG or BMP format in a Save As dialog. A capture delay of 3, 5, or 10 seconds hides the window before the toolbar appears. At least one destination must be enabled. A shortcut conflict is shown in the settings window.

The screenshot toolbar and settings are still in development and do not yet reproduce every Snipping Tool detail. Image annotation is not implemented. The outside of a freeform capture is transparent in PNG files and in applications that read transparency from the clipboard, and white elsewhere (including BMP files).
