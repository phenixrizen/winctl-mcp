# assert.pixel_color

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Sample an image, bound-window screenshot, or element-scoped screenshot pixel and optionally assert expected RGB.

## Inputs

- `image_path`: optional image path.
- `bound_id`: optional bound window ID to capture first.
- `element_ref` or `selector`: optional UI Automation element scope when `bound_id` is supplied; `x`/`y` become relative to the element bounds.
- `max_depth`, `max_elements`: optional element-scope limits.
- `x`, `y`: image/window pixel coordinates, or element-relative coordinates when scoped.
- `expected_rgb`: optional RGB triplet.
- `tolerance`: optional per-channel tolerance.
- `negate`: invert the final result.
- `timeout_ms`, `poll_interval_ms`: poll until the assertion passes or times out.

## Notes

Read-only; no armed control session is needed. If `expected_rgb` is omitted, the sampled color is returned without failing. Element-scoped assertions require a bound window; image paths cannot be paired with UIA scope.
