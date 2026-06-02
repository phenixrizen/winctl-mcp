# build.run

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Run an allowlisted build tool directly and return structured output diagnostics.

## Inputs

- `program`: build executable, such as `cargo`, `dotnet`, `msbuild`, `cmake`, or `ctest`.
- `args`: argv-style arguments.
- `cwd`: optional working directory.
- `timeout_ms`: optional diagnostic timeout budget.

## Notes

The command is not passed through `cmd.exe`, PowerShell, or a shell string.
