# capture.ocr_region

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: OCR an image region or freshly captured bound window.

## Inputs

- `image_path`: optional image path.
- `bound_id`: optional bound window ID to capture first.
- `x`, `y`, `width`, `height`: optional region metadata.

## Notes

The tool saves the cropped region as an artifact and returns recognized text with word bounding boxes. On Windows it tries the in-box `Windows.Media.Ocr` provider first, so a stock Windows install does not need Tesseract. If Windows OCR is unavailable or fails, it falls back to direct Tesseract TSV parsing when `tesseract` is on `PATH`; if no provider succeeds, the response fails closed with per-provider diagnostics.

Each word includes a `bounds` box and a `center` point already translated out of crop-local pixels. The response `coordinate_space` reports the space those values are in: `screen_pixels` when `bound_id` was given (the box is translated through the screenshot's virtual-desktop region, so `center` is directly usable as `input.click` with `coordinate_space: "screen_pixels"`), or `image_pixels` when a caller-supplied `image_path` was used (no known screen origin). This makes the OCR-fallback path clickable in one step instead of requiring manual offset math. Prefer `uia.find`/`capture.read_text` first; use OCR only for text or controls the UI Automation tree cannot expose.
