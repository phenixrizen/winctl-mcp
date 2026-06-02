# input.delay

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Wait for a bounded number of milliseconds and return timing metadata for replay manifests.

## Inputs

- `duration_ms`: requested wait duration in milliseconds.

## Notes

The server caps one delay call at 60 seconds and reports elapsed timing metadata.
