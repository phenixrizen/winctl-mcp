# recorder.pause

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Pause or resume the active human recording session without executing desktop input.

## Inputs

- `paused`: true to pause native input capture, false to resume it.
- `reason`: optional audit note attached to the session transition.

## Notes

The recorder is passive and does not require the desktop-control consent gate. Start, pause, resume, and stop transitions are written to the durable control audit log.
