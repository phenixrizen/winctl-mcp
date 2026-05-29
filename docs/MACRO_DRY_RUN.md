# macro.dry_run

[Back to tool index](INDEX.md)

Agent name: `winctl-cmp`

Description: Build a dry-run plan for a macro manifest without performing mutating UI actions.

## Inputs

- `manifest`: a `winctl.macro.v1` JSON manifest.

## Notes

The plan marks mutating steps, bound-window requirements, artifact-producing steps, categories, and target strategies.
