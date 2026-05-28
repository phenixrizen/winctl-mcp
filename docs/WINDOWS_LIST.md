# windows.list

[Back to tool index](INDEX.md)

Agent name: `winctl-cmp`

Description: List visible and discoverable top-level Windows windows with HWND, PID, executable, class, title, and virtual desktop geometry.

## Inputs

None.

## Notes

This is a diagnostic enumeration tool. Control actions should still use `windows.bind` and later operate through the returned `bound_id`.
