# capture.screenshot_window

[Back to tool index](INDEX.md)

Agent name: `winctl-cmp`

Description: Capture a screenshot of a bound window and return exact virtual desktop region metadata.

## Inputs

- `bound_id`: bound window ID returned by `windows.bind`.

## Notes

The bound window is revalidated before capture. Screenshot responses include virtual desktop coordinates for the captured region.
