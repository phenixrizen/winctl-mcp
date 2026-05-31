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
