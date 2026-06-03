# assert.process

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Assert process lifecycle, responsiveness, resource ceilings, and crash-cleanliness.

## Inputs

- `pid` or `launch_id`: process target. `launch_id` must be tracked by this server session.
- `expect`: `present` or `absent`.
- `negate`: invert the final result.
- `timeout_ms`, `poll_interval_ms`: poll until the assertion passes or times out.
- `running`, `exited`: lifecycle predicates.
- `exit_code`: expected exit code when a provider can prove it. Current live-PID assertions cannot recover historical exit codes after handles are gone; if unavailable, the assertion fails closed with diagnostics.
- `responsive`: assert any current top-level window for the PID is not hung.
- `max_working_set_bytes`, `max_handle_count`, `max_gdi_objects`, `max_user_objects`: resource ceilings from `process.metrics`.
- `crash_free_since_unix_ms`: Unix millisecond marker captured before the risky action.

## Notes

Read-only; no armed control session is needed. Crash-free checks call `diagnostics.crash_report` and fail if WER dumps or Application Event Log errors are found at or after the marker. Required provider failures return `ok: false` with diagnostics, not a silent pass.
