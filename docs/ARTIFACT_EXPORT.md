# artifact.export

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Export a captured artifact to the capture export directory or an allowlisted destination.

## Inputs

- `source_path`: source artifact path.
- `destination_path`: optional destination path.

## Policy

When `destination_path` is omitted, the artifact is copied under the server capture directory's `exports` folder. Explicit destinations must be under an allowlisted filesystem root.
