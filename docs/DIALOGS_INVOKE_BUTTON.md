# dialogs.invoke_button

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Invoke an explicit foreground dialog button through UI Automation `InvokePattern`.

## Inputs

- `hwnd`: required dialog HWND, normally from `dialogs.list`.
- `pid`: required owning process ID.
- `button_name`: optional exact button name, such as `OK` or `Cancel`.
- `element_ref`: optional UI Automation element reference from `dialogs.list`.
- `selector`: optional UI Automation selector for a Button or SplitButton element.
- `max_depth`: optional UI Automation snapshot depth.
- `max_elements`: optional UI Automation snapshot element cap.
- `allow_offscreen`: allow invoking an offscreen button when explicitly intended.
- `allow_non_dialog`: allow action against a non-dialog foreground window for diagnostics.

## Notes

The tool revalidates HWND and PID, requires the window to be foreground, and invokes only Button or SplitButton elements. It does not dispatch coordinate fallback clicks.

UAC secure-desktop prompts are not automated. When a secure desktop or `consent.exe` prompt is detected, the tool returns a clear error instead of pretending to handle it.
