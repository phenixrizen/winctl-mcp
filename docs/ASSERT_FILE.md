# assert.file

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Assert allowlisted filesystem path presence, content predicates, size, and SHA-256.

## Inputs

- `path`: file path under configured filesystem roots, capture directory, or temp directory.
- `expect`: `present` or `absent`.
- `negate`: invert the final result.
- `timeout_ms`, `poll_interval_ms`: poll until the assertion passes or times out.
- `content`, `content_contains`, `content_regex`: UTF-8 content predicates. Compared content is redacted in results.
- `size_bytes`, `min_size_bytes`, `max_size_bytes`: size predicates.
- `sha256`: expected SHA-256 hex digest.

## Notes

Read-only; no armed control session is needed and filesystem mutation policy is not used. Results never echo file content; they include metadata, UTF-8 status, and hash data only.
