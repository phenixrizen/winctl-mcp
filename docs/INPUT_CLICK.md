# input.click

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Click a bound window coordinate after identity revalidation and window-from-point preflight.

## Inputs

- `bound_id`: bound window ID returned by `windows.bind`.
- `x`: X coordinate.
- `y`: Y coordinate.
- `coordinate_space`: `screen_pixels`, `window_pixels`, `client_pixels`, `normalized_window`, or `normalized_client`.
- `button`: optional `left`, `right`, or `middle`; defaults to `left`.
- `fail_if_outside_bound`: optionally fail if preflight resolves outside the bound target.

## Notes

The bound window is revalidated before input is sent.
