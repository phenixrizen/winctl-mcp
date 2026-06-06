# capture.video_stop

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Stop the active video recording and return replay artifact metadata.

## Inputs

- `recording_id`: optional active recording ID from `capture.video_start`.

## Notes

Stopping joins the background capture worker, encodes the captured PNG frames into a GIF, and returns `recording.output_path`, `frames_dir`, `frame_count`, `encoded_width`, `encoded_height`, elapsed time, warnings, target metadata, `capture_providers`, and any `capture_fallbacks`.

Macro and test runs can request run-video capture with the `video` field on `macro.run` or `test.run`; the resulting video artifact is attached to the run result.
