# MiniLM Embeddings

[Back to tool index](INDEX.md)

The memory schema is built around MiniLM-compatible 384-dimensional embeddings. The default metadata is:

```text
embedding_model = winctl-local-minilm-compatible-384
embedding_dim = 384
```

## Model Location

Place a MiniLM-compatible ONNX model here:

```text
%LOCALAPPDATA%\winctl-mcp\models\minilm.onnx
```

Or configure a different path:

```toml
[embedding]
model_path = "C:/Users/nater/AppData/Local/winctl-mcp/models/minilm.onnx"
dimension = 384
```

## Current Runtime Behavior

The database, config, and diagnostics record the model path and embedding dimension. The current embedding implementation uses a deterministic local 384-dimensional fallback so memory search works before ONNX runtime integration is added.

When ONNX embedding support is enabled, keep these rules:

- Use a 384-dimensional MiniLM-compatible model unless the database schema is migrated.
- Record the exact model name/path in `memory_metadata`.
- Reindex the memory database after changing models.
- Keep embeddings local; do not call external embedding services from the memory store.
