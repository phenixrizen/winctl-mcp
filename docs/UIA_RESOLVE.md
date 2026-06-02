# uia.resolve

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Revalidate a UI Automation element reference path against the current bound window snapshot.

## Inputs

- `bound_id`: bound window ID returned by `windows.bind`.
- `element_ref`: stable element reference returned by `uia.snapshot` or `uia.find`.
- `max_depth`: optional tree depth limit; defaults to `8`.
- `max_elements`: optional element count limit; defaults to `2000`.

## Notes

The element reference is only considered valid if it can be found after revalidating the owning bound window identity and taking a fresh UI Automation snapshot.
