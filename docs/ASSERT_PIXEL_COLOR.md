# assert.pixel_color

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Sample an image or bound-window screenshot pixel and optionally assert expected RGB.

## Inputs

- `image_path`: optional image path.
- `bound_id`: optional bound window ID to capture first.
- `x`, `y`: image pixel coordinates.
- `expected_rgb`: optional RGB triplet.
- `tolerance`: optional per-channel tolerance.

## Notes

If `expected_rgb` is omitted, the sampled color is returned without failing.
