# windows.bind

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Bind one strict window target by stable identity before any control action.

## Inputs

Accepts a `WindowSelector`. Prefer strong selectors such as `hwnd`, `pid`, `id`, `process_name`, or executable-path filters.

## Notes

The returned `bound_id` records HWND, PID, process name, executable path, title at bind time, selector, and bind timestamp. Bound-window actions revalidate identity before they run.
