# Recorder

[Back to tool index](INDEX.md)

The local recorder is a foundation for recording UI automation into auditable macro manifests. It exposes:

- `/recorder`: local read-only recorder UI.
- `/recorder/state`: windows and recording session JSON.
- `recorder.*` MCP tools for session lifecycle and manifest export.

The recorder stores structured MCP tool steps. It does not bypass target identity validation; exported manifests run through the normal macro engine.
