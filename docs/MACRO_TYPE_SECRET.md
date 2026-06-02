# macro.type_secret

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Resolve a named encrypted secret server-side and type it into a bound window without exposing plaintext.

## Inputs

- `bound_id`: stable bound-window id returned by `windows.bind`.
- `secret_ref`: name of a secret stored with `secret.set`.

## Notes

This tool requires an armed control session. It decrypts the secret only inside the server at dispatch time, types it through the same revalidated input path used by keyboard tools, and returns no plaintext, ciphertext, secret length, or typed-count metadata.
