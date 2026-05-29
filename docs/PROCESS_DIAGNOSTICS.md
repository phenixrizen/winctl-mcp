# process.diagnostics

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Return additional process diagnostics with optional windows and child-process metadata.

## Inputs

- `pid`: process ID.
- `include_windows`: include current top-level windows owned by the process.
- `include_children`: include child process metadata.

## Notes

This is a diagnostic read-only tool. Use it to distinguish app processes, terminals, browser children, and WebView helper processes before binding.
