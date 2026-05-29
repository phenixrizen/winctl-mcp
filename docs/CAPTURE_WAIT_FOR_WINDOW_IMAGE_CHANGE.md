# capture.wait_for_window_image_change

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Poll bound-window screenshots until the image bytes change, returning replay-safe capture diagnostics.

## Inputs

- `bound_id`: bound window ID returned by `windows.bind`.
- `timeout_ms`: optional timeout in milliseconds.
- `poll_interval_ms`: optional poll interval in milliseconds.

## Notes

Each screenshot revalidates the bound target before capture. Results include initial and changed screenshot metadata plus timing.
