---
name: snippinglite-validation
description: Validate SnippingLite changes before committing.
---

# SnippingLite validation

Run `pwsh -File scripts/validate.ps1` before committing. For capture or UI changes, build the release binary and exercise capture, cancellation, clipboard output, shortcut registration, and multiple monitors where available. Report untested behavior honestly. Run `pwsh -File scripts/complete-task.ps1` before reporting completion.

Text alignment: `scripts/check-text-alignment.ps1` runs as part of `validate.ps1` and fails on top or bottom text anchors, `canvas.text` without `DT_VCENTER`, or a removed Japanese font offset. For UI changes also capture the settings and history windows and check by eye that every label is vertically centered against its toggle, button or row; fix the cause (anchor or font offset) instead of adding `// align-ok`.
