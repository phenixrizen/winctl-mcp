# windows.wait_for_state

[Back to tool index](INDEX.md)

Agent name: `winctl-cmp`

Description: Wait for a bound window to satisfy state, foreground, title, or class conditions after identity revalidation.

## Inputs

- `bound_id`: bound window ID returned by `windows.bind`.
- `timeout_ms`: optional timeout in milliseconds.
- `poll_interval_ms`: optional poll interval in milliseconds.
- `visible`: optional expected visibility.
- `foreground`: optional expected foreground state.
- `minimized`: optional expected minimized state.
- `cloaked`: optional expected cloaked state.
- `title_contains`: optional bound-window title substring.
- `class_name_contains`: optional class-name substring.

## Notes

The tool never selects by title alone; every poll revalidates the original bound window identity.
