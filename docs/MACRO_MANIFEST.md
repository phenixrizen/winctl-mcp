# Macro Manifest

[Back to tool index](INDEX.md)

`winctl.macro.v1` is the versioned JSON manifest format for replayable Windows UI automation.

## Required Shape

- `version`: must be `winctl.macro.v1`.
- `kind`: procedure kind, such as `test_procedure` or `macro`.
- `title`: human-readable title.
- `description`: audit-oriented explanation.
- `tags`: searchable labels.
- `app_identity`: executable/product identity requirements.
- `launch`: optional `process.launch` tool call.
- `bind`: binding strategy with required executable or expected identity.
- `preconditions`, `steps`, `waits`, `assertions`, `cleanup`: auditable tool steps.
- `artifacts`: screenshot, result, target, UIA, and log capture policy.
- `replay`: source and last-run metadata.

## Targeting Rules

Macro steps should prefer semantic targets: UI Automation element references, names, roles, automation IDs, class names, hierarchy paths, visible text, image checkpoints, and verified bound windows.

Coordinates are fallback metadata only. Coordinate fallback requires monitor, virtual desktop, window/client rect, DPI, scale, screenshot size, resolved target, and preflight validation metadata.

## Validation

The `winctl-macro` crate validates the manifest version, supported tool names, unique step IDs, target identity requirements, assertion safety, and coordinate fallback metadata.
