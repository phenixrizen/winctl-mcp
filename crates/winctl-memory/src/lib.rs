use std::collections::HashMap;
use std::path::PathBuf;

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;
use zeroize::Zeroizing;

pub const MEMORY_SCHEMA_VERSION: i64 = 2;
pub const DEFAULT_EMBEDDING_DIM: usize = 384;
pub const DEFAULT_EMBEDDING_MODEL: &str = "winctl-local-minilm-compatible-384";
pub const SECRET_PROVIDER_WINDOWS_DPAPI_USER: &str = "windows_dpapi_user";

#[derive(Debug, Error)]
pub enum MemoryError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, MemoryError>;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MemoryItem {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub text: String,
    pub manifest_json: Option<Value>,
    pub tags: Vec<String>,
    pub app_identity_json: Option<Value>,
    pub target_identity_json: Option<Value>,
    pub embedding_model: Option<String>,
    pub embedding_dim: Option<i64>,
    pub schema_version: i64,
    pub created_at: String,
    pub updated_at: String,
    pub last_used_at: Option<String>,
    pub use_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MemorySearchResult {
    pub item: MemoryItem,
    pub score: f64,
    pub vector_score: f64,
    pub keyword_score: f64,
    pub tag_score: f64,
    pub identity_score: f64,
    pub usefulness_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RememberRequest {
    pub kind: String,
    pub title: String,
    pub text: String,
    pub manifest_json: Option<Value>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub app_identity_json: Option<Value>,
    pub target_identity_json: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MemoryUpdateRequest {
    pub id: String,
    pub kind: Option<String>,
    pub title: Option<String>,
    pub text: Option<String>,
    pub manifest_json: Option<Value>,
    pub tags: Option<Vec<String>>,
    pub app_identity_json: Option<Value>,
    pub target_identity_json: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct MemorySearchRequest {
    pub query: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub kind: Option<String>,
    pub app_identity_json: Option<Value>,
    pub target_identity_json: Option<Value>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct MemoryListRequest {
    pub kind: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MemoryIdRequest {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SecretMetadata {
    pub name: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub provider: String,
    pub schema_version: i64,
    pub created_at: String,
    pub updated_at: String,
    pub last_used_at: Option<String>,
    pub use_count: i64,
}

pub struct MemoryStore {
    conn: Connection,
    embedding_model: String,
    embedding_dim: usize,
}

impl MemoryStore {
    pub fn open_default() -> Result<Self> {
        Self::open(default_memory_db_path())
    }

    pub fn open(path: PathBuf) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                MemoryError::Other(format!(
                    "failed to create memory database directory {}: {error}",
                    parent.display()
                ))
            })?;
        }
        register_sqlite_vec();
        let conn = Connection::open(path)?;
        Self::from_connection(conn)
    }

    pub fn open_in_memory() -> Result<Self> {
        register_sqlite_vec();
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(conn: Connection) -> Result<Self> {
        let store = Self {
            conn,
            embedding_model: DEFAULT_EMBEDDING_MODEL.into(),
            embedding_dim: DEFAULT_EMBEDDING_DIM,
        };
        store.migrate()?;
        Ok(store)
    }

    pub fn remember(&mut self, request: RememberRequest) -> Result<MemoryItem> {
        let id = Uuid::new_v4().to_string();
        let now = now();
        let embedding_text = embedding_text(
            &request.title,
            &request.text,
            &request.tags,
            &request.manifest_json,
            &request.app_identity_json,
            &request.target_identity_json,
        );
        let tags_json = serde_json::to_string(&request.tags)?;
        let manifest_json = opt_json_to_string(&request.manifest_json)?;
        let app_identity_json = opt_json_to_string(&request.app_identity_json)?;
        let target_identity_json = opt_json_to_string(&request.target_identity_json)?;

        self.conn.execute(
            "INSERT INTO memory_items (
                id, kind, title, text, manifest_json, tags_json, app_identity_json,
                target_identity_json, embedding_model, embedding_dim, schema_version,
                created_at, updated_at, use_count
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, 0)",
            params![
                id,
                request.kind,
                request.title,
                request.text,
                manifest_json,
                tags_json,
                app_identity_json,
                target_identity_json,
                self.embedding_model,
                self.embedding_dim as i64,
                MEMORY_SCHEMA_VERSION,
                now,
                now,
            ],
        )?;
        let rowid = self
            .rowid_for_id(&id)?
            .ok_or_else(|| MemoryError::Other("inserted memory item rowid was not found".into()))?;
        self.upsert_vector(rowid, &embedding_text)?;
        self.reindex()?;
        self.get_without_touch(&id)?
            .ok_or_else(|| MemoryError::Other("inserted memory item could not be read".into()))
    }

    pub fn get(&mut self, id: &str) -> Result<Option<MemoryItem>> {
        self.record_use(id)
    }

    pub fn record_use(&mut self, id: &str) -> Result<Option<MemoryItem>> {
        let item = self.get_without_touch(id)?;
        if item.is_some() {
            self.touch(id)?;
        }
        self.get_without_touch(id)
    }

    pub fn get_without_touch(&self, id: &str) -> Result<Option<MemoryItem>> {
        self.conn
            .query_row(
                "SELECT id, kind, title, text, manifest_json, tags_json, app_identity_json,
                    target_identity_json, embedding_model, embedding_dim, schema_version,
                    created_at, updated_at, last_used_at, use_count
                 FROM memory_items WHERE id = ?1",
                [id],
                row_to_item,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn update(&mut self, request: MemoryUpdateRequest) -> Result<Option<MemoryItem>> {
        let Some(mut item) = self.get_without_touch(&request.id)? else {
            return Ok(None);
        };
        if let Some(kind) = request.kind {
            item.kind = kind;
        }
        if let Some(title) = request.title {
            item.title = title;
        }
        if let Some(text) = request.text {
            item.text = text;
        }
        if request.manifest_json.is_some() {
            item.manifest_json = request.manifest_json;
        }
        if let Some(tags) = request.tags {
            item.tags = tags;
        }
        if request.app_identity_json.is_some() {
            item.app_identity_json = request.app_identity_json;
        }
        if request.target_identity_json.is_some() {
            item.target_identity_json = request.target_identity_json;
        }
        item.updated_at = now();

        self.conn.execute(
            "UPDATE memory_items
             SET kind = ?2, title = ?3, text = ?4, manifest_json = ?5, tags_json = ?6,
                 app_identity_json = ?7, target_identity_json = ?8, updated_at = ?9
             WHERE id = ?1",
            params![
                item.id,
                item.kind,
                item.title,
                item.text,
                opt_json_to_string(&item.manifest_json)?,
                serde_json::to_string(&item.tags)?,
                opt_json_to_string(&item.app_identity_json)?,
                opt_json_to_string(&item.target_identity_json)?,
                item.updated_at,
            ],
        )?;
        if let Some(rowid) = self.rowid_for_id(&request.id)? {
            let embedding_text = item_embedding_text(&item);
            self.upsert_vector(rowid, &embedding_text)?;
        }
        self.reindex()?;
        self.get_without_touch(&request.id)
    }

    pub fn delete(&mut self, id: &str) -> Result<bool> {
        let rowid = self.rowid_for_id(id)?;
        let deleted = self
            .conn
            .execute("DELETE FROM memory_items WHERE id = ?1", [id])?
            > 0;
        if deleted {
            if let Some(rowid) = rowid {
                self.delete_vector(rowid)?;
            }
            self.reindex()?;
        }
        Ok(deleted)
    }

    pub fn list(&self, request: MemoryListRequest) -> Result<Vec<MemoryItem>> {
        let limit = request.limit.unwrap_or(100).min(1000);
        let mut stmt = self.conn.prepare(
            "SELECT id, kind, title, text, manifest_json, tags_json, app_identity_json,
                target_identity_json, embedding_model, embedding_dim, schema_version,
                created_at, updated_at, last_used_at, use_count
             FROM memory_items
             ORDER BY updated_at DESC
             LIMIT ?1",
        )?;
        let items = stmt
            .query_map([limit as i64], row_to_item)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(items
            .into_iter()
            .filter(|item| filters_match(item, request.kind.as_deref(), &request.tags))
            .collect())
    }

    pub fn search(&mut self, request: MemorySearchRequest) -> Result<Vec<MemorySearchResult>> {
        let limit = request.limit.unwrap_or(20).min(100);
        let query = request.query.clone().unwrap_or_default();
        let mut candidates: HashMap<String, SearchCandidate> = HashMap::new();
        if query.trim().is_empty() {
            for item in self.list(MemoryListRequest {
                kind: request.kind.clone(),
                tags: request.tags.clone(),
                limit: Some(limit * 4),
            })? {
                upsert_candidate(&mut candidates, item, 0.0, 0.0);
            }
        } else {
            for (item, vector_score) in self.search_vec(&query, limit * 4)? {
                upsert_candidate(&mut candidates, item, vector_score, 0.0);
            }
            if let Some(fts_query) = fts_query(&query) {
                for (item, keyword_score) in self.search_fts(&fts_query, limit * 4)? {
                    upsert_candidate(&mut candidates, item, 0.0, keyword_score);
                }
            }
        }

        let mut results: Vec<_> = candidates
            .into_values()
            .filter(|candidate| {
                filters_match(&candidate.item, request.kind.as_deref(), &request.tags)
            })
            .map(|candidate| {
                let item = candidate.item;
                let tag_score = tag_score(&item.tags, &request.tags);
                let identity_score = identity_score(
                    &item.app_identity_json,
                    &request.app_identity_json,
                    &item.target_identity_json,
                    &request.target_identity_json,
                );
                let usefulness_score = usefulness_score(item.use_count);
                let score = 0.50 * candidate.vector_score
                    + 0.25 * candidate.keyword_score
                    + 0.10 * tag_score
                    + 0.10 * identity_score
                    + 0.05 * usefulness_score;
                MemorySearchResult {
                    item,
                    score,
                    vector_score: candidate.vector_score,
                    keyword_score: candidate.keyword_score,
                    tag_score,
                    identity_score,
                    usefulness_score,
                }
            })
            .collect();
        results.sort_by(|a, b| b.score.total_cmp(&a.score));
        results.truncate(limit);
        Ok(results)
    }

    pub fn reindex(&self) -> Result<()> {
        self.conn.execute(
            "INSERT INTO memory_items_fts(memory_items_fts) VALUES('rebuild')",
            [],
        )?;
        self.conn.execute("DELETE FROM memory_items_vec", [])?;
        let mut stmt = self.conn.prepare(
            "SELECT rowid, id, kind, title, text, manifest_json, tags_json, app_identity_json,
                target_identity_json, embedding_model, embedding_dim, schema_version,
                created_at, updated_at, last_used_at, use_count
             FROM memory_items",
        )?;
        let rows = stmt.query_map([], |row| {
            let rowid: i64 = row.get(0)?;
            let item = row_to_item_offset(row, 1)?;
            Ok((rowid, item))
        })?;
        for row in rows {
            let (rowid, item) = row?;
            self.upsert_vector(rowid, &item_embedding_text(&item))?;
        }
        Ok(())
    }

    pub fn set_secret(
        &mut self,
        name: &str,
        plaintext: &str,
        description: Option<String>,
        tags: Vec<String>,
    ) -> Result<SecretMetadata> {
        let name = normalize_secret_name(name)?;
        let now = now();
        let tags_json = serde_json::to_string(&tags)?;
        let ciphertext = protect_secret(plaintext)?;
        self.conn.execute(
            "INSERT INTO secrets (
                name, ciphertext, description, tags_json, provider, schema_version,
                created_at, updated_at, use_count
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0)
            ON CONFLICT(name) DO UPDATE SET
                ciphertext = excluded.ciphertext,
                description = excluded.description,
                tags_json = excluded.tags_json,
                provider = excluded.provider,
                schema_version = excluded.schema_version,
                updated_at = excluded.updated_at",
            params![
                name,
                ciphertext,
                description,
                tags_json,
                SECRET_PROVIDER_WINDOWS_DPAPI_USER,
                MEMORY_SCHEMA_VERSION,
                now,
                now,
            ],
        )?;
        self.secret_metadata(&name)?
            .ok_or_else(|| MemoryError::Other("stored secret metadata could not be read".into()))
    }

    pub fn list_secrets(&self) -> Result<Vec<SecretMetadata>> {
        let mut stmt = self.conn.prepare(
            "SELECT name, description, tags_json, provider, schema_version,
                    created_at, updated_at, last_used_at, use_count
             FROM secrets
             ORDER BY updated_at DESC, name ASC",
        )?;
        let rows = stmt.query_map([], row_to_secret_metadata)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn delete_secret(&mut self, name: &str) -> Result<bool> {
        let name = normalize_secret_name(name)?;
        Ok(self
            .conn
            .execute("DELETE FROM secrets WHERE name = ?1", [name])?
            > 0)
    }

    pub fn resolve_secret_plaintext(&mut self, name: &str) -> Result<Option<Zeroizing<String>>> {
        let name = normalize_secret_name(name)?;
        let row = self
            .conn
            .query_row(
                "SELECT ciphertext, provider FROM secrets WHERE name = ?1",
                [&name],
                |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        let Some((ciphertext, provider)) = row else {
            return Ok(None);
        };
        if provider != SECRET_PROVIDER_WINDOWS_DPAPI_USER {
            return Err(MemoryError::Other(format!(
                "unsupported secret provider {provider}"
            )));
        }
        let plaintext = unprotect_secret(&ciphertext)?;
        self.conn.execute(
            "UPDATE secrets
             SET last_used_at = ?2, use_count = use_count + 1
             WHERE name = ?1",
            params![name, now()],
        )?;
        Ok(Some(Zeroizing::new(plaintext)))
    }

    fn secret_metadata(&self, name: &str) -> Result<Option<SecretMetadata>> {
        self.conn
            .query_row(
                "SELECT name, description, tags_json, provider, schema_version,
                    created_at, updated_at, last_used_at, use_count
                 FROM secrets WHERE name = ?1",
                [name],
                row_to_secret_metadata,
            )
            .optional()
            .map_err(Into::into)
    }

    fn search_vec(&self, query: &str, limit: usize) -> Result<Vec<(MemoryItem, f64)>> {
        let embedding_json = vector_json(&self.embed_text(query))?;
        let mut stmt = self.conn.prepare(
            "SELECT memory_items.id, memory_items.kind, memory_items.title, memory_items.text,
                    memory_items.manifest_json, memory_items.tags_json,
                    memory_items.app_identity_json, memory_items.target_identity_json,
                    memory_items.embedding_model, memory_items.embedding_dim,
                    memory_items.schema_version, memory_items.created_at,
                    memory_items.updated_at, memory_items.last_used_at, memory_items.use_count,
                    memory_items_vec.distance
             FROM memory_items_vec
             JOIN memory_items ON memory_items_vec.rowid = memory_items.rowid
             WHERE memory_items_vec.embedding MATCH vec_f32(?1) AND k = ?2
             ORDER BY memory_items_vec.distance
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![embedding_json, limit as i64], |row| {
            let item = row_to_item(row)?;
            let distance: f64 = row.get(15)?;
            let vector_score = 1.0 / (1.0 + distance.max(0.0));
            Ok((item, vector_score))
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    fn search_fts(&self, query: &str, limit: usize) -> Result<Vec<(MemoryItem, f64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT memory_items.id, memory_items.kind, memory_items.title, memory_items.text,
                    memory_items.manifest_json, memory_items.tags_json,
                    memory_items.app_identity_json, memory_items.target_identity_json,
                    memory_items.embedding_model, memory_items.embedding_dim,
                    memory_items.schema_version, memory_items.created_at,
                    memory_items.updated_at, memory_items.last_used_at, memory_items.use_count,
                    bm25(memory_items_fts) AS rank
             FROM memory_items_fts
             JOIN memory_items ON memory_items_fts.rowid = memory_items.rowid
             WHERE memory_items_fts MATCH ?1
             ORDER BY rank
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![query, limit as i64], |row| {
            let item = row_to_item(row)?;
            let rank: f64 = row.get(15)?;
            let keyword_score = 1.0 / (1.0 + rank.abs());
            Ok((item, keyword_score))
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    fn rowid_for_id(&self, id: &str) -> Result<Option<i64>> {
        self.conn
            .query_row(
                "SELECT rowid FROM memory_items WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    fn upsert_vector(&self, rowid: i64, text: &str) -> Result<()> {
        let embedding_json = vector_json(&self.embed_text(text))?;
        self.conn.execute(
            "INSERT OR REPLACE INTO memory_items_vec(rowid, embedding) VALUES (?1, vec_f32(?2))",
            params![rowid, embedding_json],
        )?;
        Ok(())
    }

    fn delete_vector(&self, rowid: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM memory_items_vec WHERE rowid = ?1", [rowid])?;
        Ok(())
    }

    fn embed_text(&self, text: &str) -> Vec<f32> {
        hashed_minilm_compatible_embedding(text, self.embedding_dim)
    }

    fn touch(&self, id: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE memory_items
             SET last_used_at = ?2, use_count = use_count + 1
             WHERE id = ?1",
            params![id, now()],
        )?;
        Ok(())
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS memory_metadata (
              key TEXT PRIMARY KEY,
              value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS memory_items (
              id TEXT PRIMARY KEY,
              kind TEXT NOT NULL,
              title TEXT NOT NULL,
              text TEXT NOT NULL,
              manifest_json TEXT,
              tags_json TEXT NOT NULL DEFAULT '[]',
              app_identity_json TEXT,
              target_identity_json TEXT,
              embedding_model TEXT,
              embedding_dim INTEGER,
              schema_version INTEGER NOT NULL,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL,
              last_used_at TEXT,
              use_count INTEGER NOT NULL DEFAULT 0
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS memory_items_fts USING fts5(
              title,
              text,
              content='memory_items',
              content_rowid='rowid'
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS memory_items_vec USING vec0(
              embedding float[384]
            );

            CREATE TABLE IF NOT EXISTS secrets (
              name TEXT PRIMARY KEY,
              ciphertext BLOB NOT NULL,
              description TEXT,
              tags_json TEXT NOT NULL DEFAULT '[]',
              provider TEXT NOT NULL,
              schema_version INTEGER NOT NULL,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL,
              last_used_at TEXT,
              use_count INTEGER NOT NULL DEFAULT 0
            );
            ",
        )?;
        self.conn.execute(
            "INSERT OR REPLACE INTO memory_metadata(key, value) VALUES
             ('schema_version', ?1),
             ('migration_version', ?1),
             ('embedding_model', ?2),
             ('embedding_dim', ?3)",
            params![
                MEMORY_SCHEMA_VERSION.to_string(),
                self.embedding_model,
                self.embedding_dim.to_string()
            ],
        )?;
        self.reindex()?;
        Ok(())
    }
}

pub fn default_memory_db_path() -> PathBuf {
    if let Some(path) = std::env::var_os("WINCTL_MEMORY_DB") {
        if !path.is_empty() {
            return PathBuf::from(path);
        }
    }
    #[cfg(windows)]
    {
        if let Some(path) = std::env::var_os("LOCALAPPDATA") {
            if !path.is_empty() {
                return PathBuf::from(path).join("winctl-mcp").join("memory.sqlite");
            }
        }
    }
    std::env::temp_dir()
        .join("winctl-mcp")
        .join("memory.sqlite")
}

pub fn register_sqlite_vec() {
    unsafe {
        rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(
            sqlite_vec::sqlite3_vec_init as *const (),
        )));
    }
}

fn row_to_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryItem> {
    row_to_item_offset(row, 0)
}

fn row_to_item_offset(row: &rusqlite::Row<'_>, offset: usize) -> rusqlite::Result<MemoryItem> {
    let manifest_json: Option<String> = row.get(offset + 4)?;
    let tags_json: String = row.get(offset + 5)?;
    let app_identity_json: Option<String> = row.get(offset + 6)?;
    let target_identity_json: Option<String> = row.get(offset + 7)?;
    Ok(MemoryItem {
        id: row.get(offset)?,
        kind: row.get(offset + 1)?,
        title: row.get(offset + 2)?,
        text: row.get(offset + 3)?,
        manifest_json: parse_opt_json(manifest_json)?,
        tags: serde_json::from_str(&tags_json).unwrap_or_default(),
        app_identity_json: parse_opt_json(app_identity_json)?,
        target_identity_json: parse_opt_json(target_identity_json)?,
        embedding_model: row.get(offset + 8)?,
        embedding_dim: row.get(offset + 9)?,
        schema_version: row.get(offset + 10)?,
        created_at: row.get(offset + 11)?,
        updated_at: row.get(offset + 12)?,
        last_used_at: row.get(offset + 13)?,
        use_count: row.get(offset + 14)?,
    })
}

fn row_to_secret_metadata(row: &rusqlite::Row<'_>) -> rusqlite::Result<SecretMetadata> {
    let tags_json: String = row.get(2)?;
    Ok(SecretMetadata {
        name: row.get(0)?,
        description: row.get(1)?,
        tags: serde_json::from_str(&tags_json).unwrap_or_default(),
        provider: row.get(3)?,
        schema_version: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
        last_used_at: row.get(7)?,
        use_count: row.get(8)?,
    })
}

fn parse_opt_json(value: Option<String>) -> rusqlite::Result<Option<Value>> {
    value
        .map(|value| {
            serde_json::from_str(&value).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })
        })
        .transpose()
}

fn opt_json_to_string(value: &Option<Value>) -> Result<Option<String>> {
    value
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(Into::into)
}

fn normalize_secret_name(name: &str) -> Result<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(MemoryError::Other("secret name must not be empty".into()));
    }
    if trimmed.chars().count() > 256 {
        return Err(MemoryError::Other(
            "secret name must be 256 characters or fewer".into(),
        ));
    }
    Ok(trimmed.to_owned())
}

#[cfg(windows)]
fn protect_secret(plaintext: &str) -> Result<Vec<u8>> {
    use windows::core::w;
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{
        CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    let bytes = plaintext.as_bytes();
    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes.len() as u32,
        pbData: bytes.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptProtectData(
            &input,
            w!("winctl-mcp secret"),
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
        .map_err(|error| MemoryError::Other(format!("DPAPI protect failed: {error}")))?;
        let protected = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        let _ = LocalFree(Some(HLOCAL(output.pbData as _)));
        Ok(protected)
    }
}

#[cfg(not(windows))]
fn protect_secret(_plaintext: &str) -> Result<Vec<u8>> {
    Err(MemoryError::Other(
        "DPAPI secrets are only available on Windows".into(),
    ))
}

#[cfg(windows)]
fn unprotect_secret(ciphertext: &[u8]) -> Result<String> {
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{
        CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    let input = CRYPT_INTEGER_BLOB {
        cbData: ciphertext.len() as u32,
        pbData: ciphertext.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptUnprotectData(
            &input,
            None,
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
        .map_err(|error| MemoryError::Other(format!("DPAPI unprotect failed: {error}")))?;
        let plaintext = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        let _ = LocalFree(Some(HLOCAL(output.pbData as _)));
        String::from_utf8(plaintext)
            .map_err(|error| MemoryError::Other(format!("secret was not UTF-8: {error}")))
    }
}

#[cfg(not(windows))]
fn unprotect_secret(_ciphertext: &[u8]) -> Result<String> {
    Err(MemoryError::Other(
        "DPAPI secrets are only available on Windows".into(),
    ))
}

#[derive(Debug)]
struct SearchCandidate {
    item: MemoryItem,
    vector_score: f64,
    keyword_score: f64,
}

fn upsert_candidate(
    candidates: &mut HashMap<String, SearchCandidate>,
    item: MemoryItem,
    vector_score: f64,
    keyword_score: f64,
) {
    candidates
        .entry(item.id.clone())
        .and_modify(|candidate| {
            candidate.vector_score = candidate.vector_score.max(vector_score);
            candidate.keyword_score = candidate.keyword_score.max(keyword_score);
        })
        .or_insert(SearchCandidate {
            item,
            vector_score,
            keyword_score,
        });
}

fn filters_match(item: &MemoryItem, kind: Option<&str>, tags: &[String]) -> bool {
    kind.map(|kind| item.kind == kind).unwrap_or(true)
        && tags.iter().all(|tag| {
            item.tags
                .iter()
                .any(|item_tag| item_tag.eq_ignore_ascii_case(tag))
        })
}

fn fts_query(query: &str) -> Option<String> {
    let terms: Vec<_> = query
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|term| !term.is_empty())
        .map(|term| format!("{term}*"))
        .collect();
    (!terms.is_empty()).then(|| terms.join(" OR "))
}

fn tag_score(item_tags: &[String], wanted_tags: &[String]) -> f64 {
    if wanted_tags.is_empty() {
        return 0.0;
    }
    let matched = wanted_tags
        .iter()
        .filter(|tag| {
            item_tags
                .iter()
                .any(|item_tag| item_tag.eq_ignore_ascii_case(tag))
        })
        .count();
    matched as f64 / wanted_tags.len() as f64
}

fn identity_score(
    app_identity: &Option<Value>,
    wanted_app_identity: &Option<Value>,
    target_identity: &Option<Value>,
    wanted_target_identity: &Option<Value>,
) -> f64 {
    let mut score = 0.0;
    if wanted_app_identity.is_some() && app_identity == wanted_app_identity {
        score += 0.5;
    }
    if wanted_target_identity.is_some() && target_identity == wanted_target_identity {
        score += 0.5;
    }
    score
}

fn usefulness_score(use_count: i64) -> f64 {
    (use_count as f64 / 10.0).min(1.0)
}

fn embedding_text(
    title: &str,
    text: &str,
    tags: &[String],
    manifest_json: &Option<Value>,
    app_identity_json: &Option<Value>,
    target_identity_json: &Option<Value>,
) -> String {
    let mut combined = String::new();
    combined.push_str(title);
    combined.push('\n');
    combined.push_str(text);
    if !tags.is_empty() {
        combined.push_str("\ntags: ");
        combined.push_str(&tags.join(" "));
    }
    if let Some(value) = manifest_json {
        combined.push_str("\nmanifest: ");
        combined.push_str(&value.to_string());
    }
    if let Some(value) = app_identity_json {
        combined.push_str("\napp: ");
        combined.push_str(&value.to_string());
    }
    if let Some(value) = target_identity_json {
        combined.push_str("\ntarget: ");
        combined.push_str(&value.to_string());
    }
    combined
}

fn item_embedding_text(item: &MemoryItem) -> String {
    embedding_text(
        &item.title,
        &item.text,
        &item.tags,
        &item.manifest_json,
        &item.app_identity_json,
        &item.target_identity_json,
    )
}

fn hashed_minilm_compatible_embedding(text: &str, dimension: usize) -> Vec<f32> {
    let mut embedding = vec![0.0; dimension];
    for token in text
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_ascii_lowercase())
    {
        let hash = fnv1a64(token.as_bytes());
        let index = (hash as usize) % dimension;
        let sign = if hash & (1 << 63) == 0 { 1.0 } else { -1.0 };
        let weight = 1.0 + (token.len().min(16) as f32 / 16.0);
        embedding[index] += sign * weight;
    }
    let norm = embedding
        .iter()
        .map(|value| value * value)
        .sum::<f32>()
        .sqrt();
    if norm > 0.0 {
        for value in &mut embedding {
            *value /= norm;
        }
    }
    embedding
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn vector_json(vector: &[f32]) -> Result<String> {
    serde_json::to_string(vector).map_err(Into::into)
}

fn now() -> String {
    Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remember_get_and_search_round_trip() {
        let mut store = MemoryStore::open_in_memory().unwrap();
        let item = store
            .remember(RememberRequest {
                kind: "test_procedure".into(),
                title: "Betty settings smoke test".into(),
                text: "Launch Betty and open Settings".into(),
                manifest_json: Some(serde_json::json!({"version": "winctl.macro.v1"})),
                tags: vec!["betty".into(), "settings".into()],
                app_identity_json: Some(serde_json::json!({"executable_name": "Betty.exe"})),
                target_identity_json: None,
            })
            .unwrap();

        let fetched = store.get(&item.id).unwrap().unwrap();
        assert_eq!(fetched.title, "Betty settings smoke test");
        assert_eq!(fetched.use_count, 1);

        let results = store
            .search(MemorySearchRequest {
                query: Some("settings".into()),
                tags: vec!["betty".into()],
                limit: Some(10),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].item.id, item.id);
    }

    #[test]
    fn delete_removes_item() {
        let mut store = MemoryStore::open_in_memory().unwrap();
        let item = store
            .remember(RememberRequest {
                kind: "troubleshooting_note".into(),
                title: "Note".into(),
                text: "Text".into(),
                manifest_json: None,
                tags: vec![],
                app_identity_json: None,
                target_identity_json: None,
            })
            .unwrap();

        assert!(store.delete(&item.id).unwrap());
        assert!(store.get(&item.id).unwrap().is_none());
    }

    #[cfg(windows)]
    #[test]
    fn dpapi_secret_round_trip_and_list_omits_plaintext() {
        let mut store = MemoryStore::open_in_memory().unwrap();
        let plaintext = "correct horse battery staple";
        let metadata = store
            .set_secret(
                "betty/login",
                plaintext,
                Some("Betty login password".into()),
                vec!["betty".into(), "login".into()],
            )
            .unwrap();

        assert_eq!(metadata.name, "betty/login");
        assert_eq!(metadata.provider, SECRET_PROVIDER_WINDOWS_DPAPI_USER);
        let listed = store.list_secrets().unwrap();
        assert_eq!(listed.len(), 1);
        let listed_json = serde_json::to_string(&listed).unwrap();
        assert!(!listed_json.contains(plaintext));
        assert!(!listed_json.contains("ciphertext"));

        let resolved = store
            .resolve_secret_plaintext("betty/login")
            .unwrap()
            .unwrap();
        assert_eq!(&*resolved, plaintext);
        drop(resolved);

        let listed = store.list_secrets().unwrap();
        assert_eq!(listed[0].use_count, 1);
        assert!(listed[0].last_used_at.is_some());
        assert!(store.delete_secret("betty/login").unwrap());
        assert!(store.list_secrets().unwrap().is_empty());
    }
}
