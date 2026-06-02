# capture.compare_baseline

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Compare an actual image against a baseline and write an optional diff artifact.

## Inputs

- `actual_path`: actual image path.
- `baseline_path`: baseline image path.
- `tolerance`: optional per-channel RGB tolerance.
- `max_different_pixels`: optional allowed difference count.
- `diff_path`: optional diff output path.

## Notes

The diff image highlights changed pixels and defaults to the capture directory when `diff_path` is omitted.
