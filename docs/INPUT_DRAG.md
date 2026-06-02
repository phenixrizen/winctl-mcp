# input.drag

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Drag between two bound window coordinates after identity revalidation and point preflight.

## Inputs

- `bound_id`: bound window ID returned by `windows.bind`.
- `start_x` / `start_y`: start coordinate.
- `end_x` / `end_y`: end coordinate.
- `coordinate_space`: `screen_pixels`, `window_pixels`, `client_pixels`, `normalized_window`, or `normalized_client`.
- `button`: optional `left`, `right`, or `middle`; defaults to `left`.
- `duration_ms`: optional drag duration.
- `fail_if_outside_bound`: optionally fail if either point resolves outside the bound target.

## Notes

Both start and end points are resolved and preflighted separately.
