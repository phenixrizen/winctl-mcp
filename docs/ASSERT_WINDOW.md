# assert.window

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Assert read-only predicates for a bound window or discovery selector.

## Inputs

- `bound_id`: optional bound window ID. Preferred for strict known targets; HWND, PID, and executable identity are revalidated.
- `selector`: optional `WindowSelector` used only for read-only discovery when `bound_id` is omitted.
- `expect`: `present` or `absent`.
- `negate`: invert the final result.
- `timeout_ms`, `poll_interval_ms`: poll until the assertion passes or times out.
- `foreground`, `state`: foreground and `minimized`/`maximized`/`normal` checks.
- `title`, `title_contains`, `title_regex`: title predicates.
- `class_name`, `class_name_contains`, `class_name_regex`: class predicates.
- `x`, `y`, `width`, `height`, `bounds_tolerance`: virtual desktop geometry checks.
- `responsive`: assert not hung when the native provider is available.

## Notes

Read-only; no armed control session is needed. Missing `bound_id`/`selector`, stale bindings, unavailable required providers, and assertion misses return `ok: false`.
