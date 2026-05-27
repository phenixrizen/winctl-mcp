# AGENTS.md

## Scope
This file applies to the entire repository.

## Project intent
- Build a reliable Windows MCP replacement in Rust, with strict window identity validation.
- Prefer correctness and explicit diagnostics over fuzzy heuristics for control actions.

## Naming and layout
- Repository: `winctl-mcp`
- Core library crate: `winctl`
- MCP server binary crate: `winctl-mcp-server`

## Development environment
- Primary day-to-day development host is Ubuntu/WSL.
- Target runtime is Windows.
- Use cross-compilation and reproducible build commands from the `Makefile`.

## Coding guidance
- Keep low-level Win32 logic in `crates/winctl`.
- Keep MCP tool wiring and orchestration in `crates/winctl-mcp-server`.
- Bound-window operations must validate stable identity (HWND + PID + executable) before action.
- Title-only matching must never be trusted for control actions.

## Operational guidance
- Include tracing logs for bind/focus/click/type/capture flows.
- Every screenshot response should include virtual desktop coordinates for the captured region.
