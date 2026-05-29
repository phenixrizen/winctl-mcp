# Memory Backup

[Back to tool index](INDEX.md)

The memory database is a local SQLite database. Default path:

```text
%LOCALAPPDATA%\winctl-mcp\memory\memory.sqlite
```

## Backup

Stop the server or ensure no memory writes are running, then copy:

```powershell
$base = "$env:LOCALAPPDATA\winctl-mcp"
$src = "$base\memory"
$dst = "$base\backups\memory-$(Get-Date -Format yyyyMMdd-HHmmss)"
New-Item -ItemType Directory -Force -Path $dst | Out-Null
Copy-Item "$src\memory.sqlite*" $dst -Force
```

Copy `memory.sqlite`, `memory.sqlite-wal`, and `memory.sqlite-shm` when sidecar files exist.

## Restore

Stop the server, copy the backup files back into the memory directory, then start the server and run:

```text
memory.reindex
```

Reindexing rebuilds FTS and vector indexes from the stored memory items.

## Migration Direction

The database records `schema_version`, `migration_version`, `embedding_model`, and `embedding_dim` in `memory_metadata`. Future migrations should be additive, logged, and followed by `memory.reindex` when search indexes or embedding settings change.
