use crate::AppState;
use winctl_memory::{
    MemoryIdRequest, MemoryListRequest, MemorySearchRequest, MemoryUpdateRequest, RememberRequest,
};

pub fn memory_remember(state: &AppState, request: RememberRequest) -> serde_json::Value {
    tracing::info!(
        kind = %request.kind,
        title = %request.title,
        tags = ?request.tags,
        has_manifest = request.manifest_json.is_some(),
        has_app_identity = request.app_identity_json.is_some(),
        has_target_identity = request.target_identity_json.is_some(),
        "memory.remember requested"
    );
    if !state.policy.memory_mutation_enabled {
        return policy_denied(
            "memory_mutation_disabled",
            "memory.remember is disabled by runtime policy",
        );
    }
    let mut store = state.memory.lock().expect("memory store mutex poisoned");
    match store.remember(request) {
        Ok(item) => {
            tracing::info!(
                memory_id = %item.id,
                kind = %item.kind,
                title = %item.title,
                "memory.remember stored item"
            );
            serde_json::json!({
                "ok": true,
                "item": item,
                "embedding": {
                    "model": winctl_memory::DEFAULT_EMBEDDING_MODEL,
                    "dimension": winctl_memory::DEFAULT_EMBEDDING_DIM
                }
            })
        }
        Err(error) => {
            tracing::warn!(error = %error, "memory.remember failed");
            serde_json::json!({"ok": false, "error": memory_error("memory_remember_failed", error)})
        }
    }
}

pub fn memory_search(state: &AppState, request: MemorySearchRequest) -> serde_json::Value {
    tracing::info!(
        query = ?request.query,
        kind = ?request.kind,
        tags = ?request.tags,
        limit = ?request.limit,
        has_app_identity = request.app_identity_json.is_some(),
        has_target_identity = request.target_identity_json.is_some(),
        "memory.search requested"
    );
    let mut store = state.memory.lock().expect("memory store mutex poisoned");
    match store.search(request) {
        Ok(results) => {
            tracing::info!(result_count = results.len(), "memory.search completed");
            serde_json::json!({
                "ok": true,
                "results": results,
                "embedding": {
                    "model": winctl_memory::DEFAULT_EMBEDDING_MODEL,
                    "dimension": winctl_memory::DEFAULT_EMBEDDING_DIM
                }
            })
        }
        Err(error) => {
            tracing::warn!(error = %error, "memory.search failed");
            serde_json::json!({"ok": false, "error": memory_error("memory_search_failed", error)})
        }
    }
}

pub fn memory_get(state: &AppState, request: MemoryIdRequest) -> serde_json::Value {
    tracing::info!(memory_id = %request.id, "memory.get requested");
    let mut store = state.memory.lock().expect("memory store mutex poisoned");
    match store.get(&request.id) {
        Ok(Some(item)) => {
            tracing::info!(
                memory_id = %item.id,
                kind = %item.kind,
                use_count = item.use_count,
                "memory.get returned item"
            );
            serde_json::json!({"ok": true, "item": item})
        }
        Ok(None) => {
            tracing::warn!(memory_id = %request.id, "memory.get item not found");
            serde_json::json!({
                "ok": false,
                "error": {
                    "code": "memory_item_not_found",
                    "message": format!("memory item {} was not found", request.id)
                }
            })
        }
        Err(error) => {
            tracing::warn!(memory_id = %request.id, error = %error, "memory.get failed");
            serde_json::json!({"ok": false, "error": memory_error("memory_get_failed", error)})
        }
    }
}

pub fn memory_update(state: &AppState, request: MemoryUpdateRequest) -> serde_json::Value {
    tracing::info!(
        memory_id = %request.id,
        kind_updated = request.kind.is_some(),
        title_updated = request.title.is_some(),
        text_updated = request.text.is_some(),
        manifest_updated = request.manifest_json.is_some(),
        tags_updated = request.tags.is_some(),
        app_identity_updated = request.app_identity_json.is_some(),
        target_identity_updated = request.target_identity_json.is_some(),
        "memory.update requested"
    );
    if !state.policy.memory_mutation_enabled {
        return policy_denied(
            "memory_mutation_disabled",
            "memory.update is disabled by runtime policy",
        );
    }
    let mut store = state.memory.lock().expect("memory store mutex poisoned");
    match store.update(request) {
        Ok(Some(item)) => {
            tracing::info!(memory_id = %item.id, "memory.update stored item");
            serde_json::json!({"ok": true, "item": item})
        }
        Ok(None) => serde_json::json!({
            "ok": false,
            "error": {
                "code": "memory_item_not_found",
                "message": "memory item was not found"
            }
        }),
        Err(error) => {
            tracing::warn!(error = %error, "memory.update failed");
            serde_json::json!({"ok": false, "error": memory_error("memory_update_failed", error)})
        }
    }
}

pub fn memory_delete(state: &AppState, request: MemoryIdRequest) -> serde_json::Value {
    tracing::info!(memory_id = %request.id, "memory.delete requested");
    if !state.policy.memory_mutation_enabled {
        return policy_denied(
            "memory_mutation_disabled",
            "memory.delete is disabled by runtime policy",
        );
    }
    let mut store = state.memory.lock().expect("memory store mutex poisoned");
    match store.delete(&request.id) {
        Ok(deleted) => {
            tracing::info!(memory_id = %request.id, deleted, "memory.delete completed");
            serde_json::json!({"ok": true, "id": request.id, "deleted": deleted})
        }
        Err(error) => {
            tracing::warn!(memory_id = %request.id, error = %error, "memory.delete failed");
            serde_json::json!({"ok": false, "error": memory_error("memory_delete_failed", error)})
        }
    }
}

pub fn memory_list(state: &AppState, request: MemoryListRequest) -> serde_json::Value {
    tracing::info!(
        kind = ?request.kind,
        tags = ?request.tags,
        limit = ?request.limit,
        "memory.list requested"
    );
    let store = state.memory.lock().expect("memory store mutex poisoned");
    match store.list(request) {
        Ok(items) => {
            tracing::info!(item_count = items.len(), "memory.list completed");
            serde_json::json!({"ok": true, "items": items})
        }
        Err(error) => {
            tracing::warn!(error = %error, "memory.list failed");
            serde_json::json!({"ok": false, "error": memory_error("memory_list_failed", error)})
        }
    }
}

pub fn memory_reindex(state: &AppState) -> serde_json::Value {
    tracing::info!("memory.reindex requested");
    if !state.policy.memory_mutation_enabled {
        return policy_denied(
            "memory_mutation_disabled",
            "memory.reindex is disabled by runtime policy",
        );
    }
    let store = state.memory.lock().expect("memory store mutex poisoned");
    match store.reindex() {
        Ok(()) => {
            tracing::info!("memory.reindex completed");
            serde_json::json!({
                "ok": true,
                "schema_version": winctl_memory::MEMORY_SCHEMA_VERSION,
                "embedding": {
                    "model": winctl_memory::DEFAULT_EMBEDDING_MODEL,
                    "dimension": winctl_memory::DEFAULT_EMBEDDING_DIM
                }
            })
        }
        Err(error) => {
            tracing::warn!(error = %error, "memory.reindex failed");
            serde_json::json!({"ok": false, "error": memory_error("memory_reindex_failed", error)})
        }
    }
}

fn memory_error(code: &'static str, error: winctl_memory::MemoryError) -> serde_json::Value {
    serde_json::json!({
        "code": code,
        "message": error.to_string()
    })
}

fn policy_denied(code: &str, message: &str) -> serde_json::Value {
    tracing::warn!(code = code, "memory request denied by policy");
    serde_json::json!({
        "ok": false,
        "error": {
            "code": code,
            "message": message,
        }
    })
}
