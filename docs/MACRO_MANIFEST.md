# Macro Manifest

[Back to tool index](INDEX.md)

`winctl.macro.v1` is the versioned JSON manifest format for replayable Windows UI automation.

A manifest moves from authoring to verified replay through these tools:

```mermaid
flowchart LR
    author["author or recorder.stop"] --> validate["macro.validate"]
    validate --> dry["macro.dry_run"]
    dry --> promote["macro.promote (save_to_memory)"]
    promote --> run["macro.run (needs control.arm)"]
    run --> export["macro.export_result"]
```

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

Target-bound tools are resolved through a single replay resolver before dispatch. The resolver accepts these window target intents:

- `{"type":"current"}` resolves to the current bound window established by the manifest bind flow or a previous `windows.bind` step.
- `{"type":"alias","name":"app_main"}` resolves to a named bound-window alias established by a previous `windows.bind` step using the same target.
- `{"type":"launched_process_window","launch_id":"${launch_id}","pid":1234,"hwnd":"0x1234"}` resolves the current bound window only when it still matches the launched process identity.
- `{"type":"bound_window","bound_id":"${bound_id}"}` is supported for legacy manifests; new recorded manifests should prefer `current` or `alias` so runtime-specific handles are not persisted.

For backward compatibility, `args.bound_id` and `${bound_id}` substitutions still work. They are routed through the same revalidation path and produce replay diagnostics that recommend target-based manifests.

Coordinates are fallback metadata only. Coordinate fallback requires monitor, virtual desktop, window/client rect, DPI, scale, screenshot size, resolved target, and preflight validation metadata.

## Validation

The `winctl-macro` crate validates the manifest version, supported tool names, unique step IDs, target identity requirements, alias-before-bind mistakes, assertion safety, and coordinate fallback metadata. Target-bound steps without `target` or `args.bound_id` remain valid for legacy compatibility, but validation emits an `implicit_current_target` warning.
