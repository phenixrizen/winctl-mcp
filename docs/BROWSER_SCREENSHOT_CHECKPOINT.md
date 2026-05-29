# browser.screenshot_checkpoint

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Capture a screenshot checkpoint for a revalidated bound browser window.

## Inputs

- `bound_id`: bound window ID returned by `windows.bind`.

## Notes

This tool first verifies that the bound target is a recognized browser window, then captures a normal bound-window screenshot with virtual desktop coordinates.
