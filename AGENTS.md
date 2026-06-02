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

## Dashboard guidance
- Prefer the existing Vue + Tailwind + daisyUI component stack before adding custom UI.
- Use daisyUI components for common dashboard controls such as menus, collapsible submenus, tabs, buttons, badges, alerts, tables, pagination, cards, modals, and form inputs.
- Add custom dashboard CSS only for layout constraints, product-specific branding, or behavior that the existing component library does not provide.

## Operational guidance
- Include tracing logs for bind/focus/click/type/capture flows.
- Every screenshot response should include virtual desktop coordinates for the captured region.
- Keep public repository docs generic for release signing and deployment.
- Do not publish project-specific Azure/GitHub signing account names, tenant IDs, subscription IDs, client IDs, resource group names, certificate profile names, or similar private setup details in repo docs.
- Store signing configuration in protected GitHub environments/secrets or private operator notes, not in public markdown files.
