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

## Commit and versioning conventions
- Every commit MUST follow Conventional Commits: a `type:` prefix on a single-line subject.
- Types: `feat`, `fix`, `perf`, `refactor`, `docs`, `test`, `chore`, `ci`, `build`, `style`, `revert`.
- Do not use a parenthetical scope (write `feat:`, not `feat(mcp):`).
- Do not add a `Co-Authored-By:` trailer. Keep the subject to one line; no body unless asked.
- Breaking changes: add `!` after the type (`feat!:`) or include a `BREAKING CHANGE:` footer.
- CI auto-computes the release version from these commits in `.github/workflows/release.yml`
  via `scripts/compute-release-version.sh`. The bump is the highest-precedence matching type
  since the last `vX.Y.Z` tag:
  - breaking (`!` / `BREAKING CHANGE:`) -> major
  - `feat` -> minor
  - `fix`, `perf`, `revert` -> patch
  - `docs`, `chore`, `ci`, `test`, `style`, `refactor`, `build` -> no release
- Choose the type for its release effect: use `ci:` / `chore:` / `docs:` for tooling and docs so
  they do not cut a release; reserve `fix:` / `feat:` for user-facing behavior changes.
- Tags `vX.Y.Z` are the version source of truth. Do NOT bump `version` in `Cargo.toml`; it stays
  at the workspace default and releases inject `WINCTL_BUILD_VERSION` at build time.

## Operational guidance
- Include tracing logs for bind/focus/click/type/capture flows.
- Every screenshot response should include virtual desktop coordinates for the captured region.
- Keep public repository docs generic for release signing and deployment.
- Do not publish project-specific Azure/GitHub signing account names, tenant IDs, subscription IDs, client IDs, resource group names, certificate profile names, or similar private setup details in repo docs.
- Store signing configuration in protected GitHub environments/secrets or private operator notes, not in public markdown files.
