# assert.visual_match

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Assert that an image, window screenshot, region, or element crop matches a baseline image.

## Inputs

- `actual_path`: optional actual image path.
- `bound_id`: optional bound window ID to capture when `actual_path` is omitted.
- `element_ref` or `selector`: optional UI Automation element crop when `bound_id` is supplied.
- `max_depth`, `max_elements`: optional element-scope limits.
- `x`, `y`, `width`, `height`: optional region crop in image pixels, or within the element crop when element-scoped.
- `baseline_path`: baseline/reference image path.
- `tolerance`: per-channel pixel tolerance.
- `max_different_pixels`: allowed differing-pixel count.
- `diff_path`: optional diff artifact path.
- `negate`: invert the final result; useful for asserting a visual difference.
- `timeout_ms`, `poll_interval_ms`: poll until the assertion passes or times out.

## Notes

Read-only; no armed control session is needed. Element-scoped visual assertions require a bound window and revalidated UIA bounds. Comparison uses `capture.compare_baseline` and returns diff/comparison artifact metadata.
