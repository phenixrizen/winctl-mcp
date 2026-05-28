# windows.window_from_point

[Back to tool index](INDEX.md)

Agent name: `winctl-cmp`

Description: Resolve a screen point to top-level and child window diagnostics, optionally relative to a bound target.

## Inputs

- `x`: screen X coordinate.
- `y`: screen Y coordinate.
- `bound_id`: optional bound window ID used to report whether the resolved point belongs to the bound target.

## Notes

This is used as a preflight diagnostic for click safety and target validation.
