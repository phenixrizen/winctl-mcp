# capture.screenshot_window

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Capture a screenshot of a bound window and return exact virtual desktop region metadata.

## Inputs

- `bound_id`: bound window ID returned by `windows.bind`.

## Notes

The bound window is revalidated before capture. Screenshot responses include virtual desktop coordinates for the captured region plus `provider` metadata.

Windows Graphics Capture is the primary provider. When `WINCTL_CAPTURE_DXGI_FALLBACK=1` is set and Windows Graphics Capture fails, the server may use a visible-window GDI screen blit against the revalidated window rectangle. That fallback is opt-in and reflected with `provider: "gdi_screen_blt"` plus `fallback_from`.
