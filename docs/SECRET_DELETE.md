# secret.delete

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Delete one encrypted secret by name.

## Inputs

- `name`: secret name to delete.

## Notes

Deletion requires memory mutation policy to allow writes. The deleted secret value is not returned before or after deletion.
