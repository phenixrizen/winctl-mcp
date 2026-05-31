# dialogs.list

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Enumerate the foreground native dialog, UI Automation button candidates, and secure-desktop/UAC status.

## Inputs

- `max_depth`: optional UI Automation snapshot depth for dialog inspection.
- `max_elements`: optional UI Automation snapshot element cap.
- `include_non_foreground`: include non-foreground dialog-like top-level windows.
- `include_non_dialog_foreground`: include the foreground window even when it is not recognized as a native dialog.

## Notes

The tool reports `secure_desktop.active` when the input desktop is unavailable or not the normal `Default` desktop. UAC secure-desktop prompts are reported as not automatable by design.

Dialog buttons are returned as UI Automation element references. Use those exact dialog HWND/PID identities with `dialogs.invoke_button`; do not select dialogs by title alone.
