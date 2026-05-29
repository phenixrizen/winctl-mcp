# macro.run_step

[Back to tool index](INDEX.md)

Agent name: `winctl-cmp`

Description: Execute one macro step by ID for stepwise debugging.

## Inputs

- `manifest`: a `winctl.macro.v1` JSON manifest.
- `step_id`: step ID to execute.
- `context_json`: optional context containing values such as `launch_id`, `pid`, and `bound_id`.

## Notes

Use this for repair loops when a full macro run fails.
