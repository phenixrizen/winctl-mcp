# input.mouse_move

[Back to tool index](INDEX.md)

Agent name: `winctl-cmp`

Description: Move the mouse to a bound window coordinate after identity revalidation and point preflight.

## Inputs

- `bound_id`: bound window ID returned by `windows.bind`.
- `x`: X coordinate.
- `y`: Y coordinate.
- `coordinate_space`: `screen_pixels`, `window_pixels`, `client_pixels`, `normalized_window`, or `normalized_client`.
- `fail_if_outside_bound`: optionally fail if preflight resolves outside the bound target.

## Notes

Returns resolved screen position, window bounds, monitor/DPI metadata, preflight diagnostics, and timing metadata for replay.
