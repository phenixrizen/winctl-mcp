# test.run

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Run a winctl test manifest through the macro execution engine.

## Inputs

- `manifest`: `winctl.test.v1` manifest.
- `max_steps`: optional step limit.
- `video`: optional run-video capture settings, using the same fields as `capture.video_start`.

## Notes

This uses the same execution path as `macro.run`. When `video` is provided, the GIF replay artifact is attached to the run result.
