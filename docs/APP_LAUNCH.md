# app.launch

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Launch an executable, protocol handler, packaged app, or Start Menu app target without shell command concatenation.

## Inputs

- `mode`: `executable`, `protocol`, `packaged_app`, or `start_menu`.
- `target`: executable path, protocol URI, package app ID, or Start Menu app target.
- `args`: optional argv-style arguments.
- `cwd`: optional working directory.
- `wait_for_window`: wait for a visible top-level window.
- `timeout_ms`: optional window wait timeout.
- `allow_child_process_windows`: include child-process windows as candidates.

## Notes

Executable launches use the direct process path. Protocol, packaged app, and Start Menu launches use Windows shell app activation and do not concatenate arbitrary shell commands.
