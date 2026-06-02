# secret.set

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Store or replace one encrypted secret by name using Windows current-user DPAPI.

## Inputs

- `name`: stable secret name referenced later by macro replay.
- `value`: plaintext secret value. It is encrypted immediately and is never returned.
- `description`: optional metadata shown in `secret.list`.
- `tags`: optional metadata tags.

## Notes

This tool returns metadata only. It does not return plaintext or ciphertext. There is intentionally no `secret.get` tool; replay resolves a secret internally only at the instant it is typed.
