# capture.video_start

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Start recording a bound window or display to an animated GIF replay artifact.

## Inputs

- `bound_id`: optional bound window to record.
- `display_index`: optional display index to record. Defaults to `0` when `bound_id` is omitted.
- `frame_interval_ms`: optional frame interval, clamped between 100 ms and 5000 ms.
- `max_duration_ms`: optional safety cap, clamped between 500 ms and 30 minutes.
- `max_frame_width`: optional GIF encode width cap. Defaults to `1280`.
- `max_frame_height`: optional GIF encode height cap. Defaults to `720`.
- `output_name`: optional safe artifact base name.

## Notes

Window recordings revalidate the original HWND, PID, and executable identity before each frame. Display recordings capture the selected monitor. Only one video recording can be active per server session.

The artifact is written under the capture directory in `videos/` as a GIF, with source PNG frames retained in a sibling frames directory for diagnostics. Source frames are captured at native size; the GIF is scaled to the configured frame cap so high-DPI or ultrawide displays do not produce oversized replay artifacts.

Frame capture uses the screenshot providers. Windows Graphics Capture is primary; `WINCTL_CAPTURE_DXGI_FALLBACK=1` opts into fallback providers if the primary provider fails: DXGI Desktop Duplication for display capture and visible-window GDI screen blit for window capture.
