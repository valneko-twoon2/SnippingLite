---
name: snippinglite-validation
description: Validate SnippingLite changes before committing.
---

# SnippingLite validation

Run `pwsh -File scripts/validate.ps1` before committing. For capture or UI changes, build the release binary and exercise capture, cancellation, clipboard output, shortcut registration, and multiple monitors where available. Report untested behavior honestly. Run `pwsh -File scripts/complete-task.ps1` before reporting completion.
