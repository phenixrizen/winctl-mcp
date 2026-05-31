# capture.ocr_region

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: OCR an image region or freshly captured bound window.

## Inputs

- `image_path`: optional image path.
- `bound_id`: optional bound window ID to capture first.
- `x`, `y`, `width`, `height`: optional region metadata.

## Notes

The current provider runs Tesseract directly, saves the cropped region as an artifact, and returns recognized text with word bounding boxes. If Tesseract is not installed or not on `PATH`, the tool fails closed with provider diagnostics.
