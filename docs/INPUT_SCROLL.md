# input.scroll

[Back to tool index](INDEX.md)

Agent name: `winctl-cmp`

Description: Scroll at a bound window coordinate after identity revalidation and point preflight.

## Inputs

- `bound_id`: bound window ID returned by `windows.bind`.
- `x`: X coordinate.
- `y`: Y coordinate.
- `coordinate_space`: `screen_pixels`, `window_pixels`, `client_pixels`, `normalized_window`, or `normalized_client`.
- `delta_x`: optional horizontal wheel delta.
- `delta_y`: optional vertical wheel delta; defaults to `-120`.
- `fail_if_outside_bound`: optionally fail if preflight resolves outside the bound target.

## Notes

Returns resolved point, preflight diagnostics, scroll deltas, timing, and replay metadata.
