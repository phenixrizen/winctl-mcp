# browser.describe

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Describe one browser target by bound window, PID, or HWND without tab-title selection.

## Inputs

- `bound_id`: optional bound window ID.
- `pid`: optional browser process ID.
- `hwnd`: optional browser window HWND hex string.

## Notes

Use `bound_id` when available so HWND, PID, and executable identity are revalidated before browser metadata is returned.
