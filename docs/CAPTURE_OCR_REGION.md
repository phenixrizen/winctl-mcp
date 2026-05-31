# capture.ocr_region

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Return OCR-region diagnostics for an image or bound window.

## Inputs

- `image_path`: optional image path.
- `bound_id`: optional bound window ID to capture first.
- `x`, `y`, `width`, `height`: optional region metadata.

## Notes

OCR provider integration is not enabled in this build; the tool returns provider diagnostics.
