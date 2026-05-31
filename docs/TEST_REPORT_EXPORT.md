# test.report_export

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Export a test or macro run result as JSON, JUnit XML, or HTML.

## Inputs

- `run_id`: macro/test run ID.
- `format`: `json`, `junit`, or `html`.
- `output_path`: optional artifact path.

## Notes

When no output path is supplied, the report is written under the configured capture directory.
