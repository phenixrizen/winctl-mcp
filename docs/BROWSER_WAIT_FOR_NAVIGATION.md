# browser.wait_for_navigation

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Wait for a bound browser window title transition while revalidating browser PID/HWND/executable identity.

## Inputs

- `bound_id`: bound window ID returned by `windows.bind`.
- `title_contains`: optional title substring that must appear.
- `title_not_contains`: optional title substring that must no longer appear.
- `timeout_ms`: optional timeout in milliseconds.
- `poll_interval_ms`: optional polling interval in milliseconds.

## Notes

This is a browser-aware wait, not a title-only bind. Every poll revalidates the existing bound browser window identity.
