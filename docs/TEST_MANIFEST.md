# Test Manifest

[Back to tool index](INDEX.md)

`winctl.test.v1` is a JSON UI test manifest aligned with `winctl.macro.v1`. The first version wraps a macro manifest so app launch, binding strategy, preconditions, actions, waits, assertions, screenshots, cleanup, artifact policy, and replay metadata stay in one auditable structure.

## Shape

```json
{
  "version": "winctl.test.v1",
  "kind": "test_procedure",
  "title": "Betty settings smoke test",
  "description": "Launch Betty and verify the settings UI.",
  "tags": ["betty", "smoke"],
  "macro_manifest": {
    "version": "winctl.macro.v1",
    "kind": "test_procedure",
    "title": "Betty settings smoke test",
    "description": "Launch Betty and verify the settings UI.",
    "steps": []
  },
  "artifact_paths": [],
  "diagnostics": {}
}
```

Test execution uses the macro engine, so target identity validation, policy checks, memory use metadata, and replay artifacts follow the same path as `macro.run`.
