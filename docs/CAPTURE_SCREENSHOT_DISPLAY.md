# capture.screenshot_display

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Capture a screenshot of a display by zero-based monitor index and return exact virtual desktop region metadata.

## Inputs

- `display_index`: zero-based monitor index.

## Notes

Display screenshots return exact virtual desktop region metadata for the captured display plus `provider` metadata.

Windows Graphics Capture is the primary provider. When `WINCTL_CAPTURE_DXGI_FALLBACK=1` is set and Windows Graphics Capture fails, the server may use DXGI Desktop Duplication for the selected display. That fallback is opt-in and reflected with `provider: "dxgi_duplication"` plus `fallback_from`.
