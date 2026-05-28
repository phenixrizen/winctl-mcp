# windows.find

[Back to tool index](INDEX.md)

Agent name: `winctl-cmp`

Description: Find windows matching a selector and return scored diagnostics without binding or controlling them.

## Inputs

Accepts a `WindowSelector` with fields such as `id`, `hwnd`, `pid`, `process_name`, executable-path filters, title filters, class filters, and visibility/minimized/cloaked constraints.

## Notes

This tool is for discovery and diagnostics. Title-only matching is not trusted for control actions.
