# uia.find

[Back to tool index](INDEX.md)

Agent name: `winctl-cmp`

Description: Find UI Automation elements in a fresh bound-window snapshot by semantic selector fields.

## Inputs

- `bound_id`: bound window ID returned by `windows.bind`.
- `selector`: semantic element selector.
- `max_depth`: optional tree depth limit; defaults to `8`.
- `max_elements`: optional element count limit; defaults to `2000`.

## Selector Fields

- `element_ref`: stable element reference returned by `uia.snapshot`.
- `name`: UI Automation element name.
- `role`: role/control type, such as `Button`, `Pane`, or `Text`.
- `automation_id`: UI Automation automation ID.
- `class_name`: UI Automation class name.
- `text_contains`: substring match across name, automation ID, and class name.
- `include_offscreen`: include offscreen elements; defaults to `false`.

## Notes

If the selector returns multiple candidates, refine it before using the result in a replay manifest.
