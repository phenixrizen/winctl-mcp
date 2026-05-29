# uia.snapshot

[Back to tool index](INDEX.md)

Agent name: `winctl-cmp`

Description: Capture a UI Automation tree for a bound window with element roles, names, automation IDs, bounds, state, hierarchy, and stable element references.

## Inputs

- `bound_id`: bound window ID returned by `windows.bind`.
- `max_depth`: optional tree depth limit; defaults to `8`.
- `max_elements`: optional element count limit; defaults to `2000`.

## Notes

The bound window is revalidated before UI Automation is initialized. COM is initialized on the blocking worker thread used for the snapshot.
