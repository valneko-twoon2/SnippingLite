# Snipping Lite

[日本語](README.md) | English

A lightweight screenshot and screen recording tool for Windows, modeled on the Windows 11 Snipping Tool. It stays in the notification area; press **Alt+Shift+S** to start a snip.

## Features

- **Screenshots**: rectangle, freeform, window, or full screen. Copy to the clipboard and save as PNG
- **Screen recording**: record a rectangle or a window to MP4 (H.264), with optional mouse cursor, microphone, and system audio
- **Quick markup**: draw on the snip with a pen or highlighter before finishing it
- **Text extraction**: read the text in the selected area and copy it as text
- **History**: browse past screenshots, draw on them, crop, copy again, or save
- **Multiple monitors**: any arrangement and different display scales (DPI)

The interface is in Japanese only. Detailed behavior is described (in Japanese) in [docs/behavior.md](docs/behavior.md).

## Install

Run this command in PowerShell:

```powershell
irm https://github.com/valneko-twoon2/SnippingLite/releases/latest/download/install.ps1 | iex
```

- Installs for the current user (no administrator rights) to `%LocalAppData%\Programs\Snipping Lite`.
- Checks the SHA-256 of the download, starts the app, and starts it at every sign-in.
- Run the same command again to update.
- To use another folder or repository, download `install.ps1` and run it with `-Directory <folder>` or `-Repo <owner/name>`.

## Uninstall

Remove it from Windows Settings > Apps.

- Your settings and history are kept. To remove them, run `uninstall.ps1` from the install folder with `-RemoveData`.
- Saved screenshots and recordings are never deleted.

## Development

| Purpose | Command |
| --- | --- |
| Build the debug binary (does not start it) | `scripts/build.cmd` |
| Start the release build that was already built | `pwsh -File scripts/run-latest.ps1` |
| Validate changes | `pwsh -File scripts/validate.ps1` |
| Build the release files into `artifacts/` | `pwsh -File scripts/publish.ps1` |

Pushing a tag that matches the crate version (for example `v1.0.0`) runs the tests and then publishes the release on GitHub Releases.
