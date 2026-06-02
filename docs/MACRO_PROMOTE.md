# macro.promote

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Promote an approved macro manifest into the session registry and optionally explicit memory storage.

## Inputs

- `manifest`: a valid `winctl.macro.v1` JSON manifest.
- `remember`: store the macro in the local memory database. Defaults to `true`.

## Notes

Promotion is explicit. The server does not silently save exploratory actions as reusable macros.
