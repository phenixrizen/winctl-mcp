use crate::{AppState, SecretDeleteRequest, SecretSetRequest};
use zeroize::Zeroizing;

pub fn secret_set(state: &AppState, request: SecretSetRequest) -> serde_json::Value {
    tracing::info!(
        secret_name = %request.name,
        has_description = request.description.is_some(),
        tags_count = request.tags.len(),
        "secret.set requested"
    );
    if !state.policy.memory_mutation_enabled {
        return policy_denied(
            "memory_mutation_disabled",
            "secret.set is disabled by runtime memory policy",
        );
    }
    let value = Zeroizing::new(request.value);
    let mut store = state.memory.lock().expect("memory store mutex poisoned");
    match store.set_secret(&request.name, &value, request.description, request.tags) {
        Ok(secret) => {
            tracing::info!(secret_name = %secret.name, "secret.set stored metadata");
            serde_json::json!({"ok": true, "secret": secret})
        }
        Err(error) => {
            tracing::warn!(secret_name = %request.name, error = %error, "secret.set failed");
            serde_json::json!({"ok": false, "error": secret_error("secret_set_failed", error)})
        }
    }
}

pub fn secret_list(state: &AppState) -> serde_json::Value {
    tracing::info!("secret.list requested");
    let store = state.memory.lock().expect("memory store mutex poisoned");
    match store.list_secrets() {
        Ok(secrets) => {
            tracing::info!(secret_count = secrets.len(), "secret.list completed");
            serde_json::json!({
                "ok": true,
                "provider": winctl_memory::SECRET_PROVIDER_WINDOWS_DPAPI_USER,
                "secrets": secrets
            })
        }
        Err(error) => {
            tracing::warn!(error = %error, "secret.list failed");
            serde_json::json!({"ok": false, "error": secret_error("secret_list_failed", error)})
        }
    }
}

pub fn secret_delete(state: &AppState, request: SecretDeleteRequest) -> serde_json::Value {
    tracing::info!(secret_name = %request.name, "secret.delete requested");
    if !state.policy.memory_mutation_enabled {
        return policy_denied(
            "memory_mutation_disabled",
            "secret.delete is disabled by runtime memory policy",
        );
    }
    let mut store = state.memory.lock().expect("memory store mutex poisoned");
    match store.delete_secret(&request.name) {
        Ok(deleted) => {
            tracing::info!(secret_name = %request.name, deleted, "secret.delete completed");
            serde_json::json!({"ok": true, "name": request.name, "deleted": deleted})
        }
        Err(error) => {
            tracing::warn!(secret_name = %request.name, error = %error, "secret.delete failed");
            serde_json::json!({"ok": false, "error": secret_error("secret_delete_failed", error)})
        }
    }
}

fn secret_error(code: &'static str, error: winctl_memory::MemoryError) -> serde_json::Value {
    serde_json::json!({
        "code": code,
        "message": error.to_string()
    })
}

fn policy_denied(code: &str, message: &str) -> serde_json::Value {
    tracing::warn!(code = code, "secret request denied by policy");
    serde_json::json!({
        "ok": false,
        "error": {
            "code": code,
            "message": message,
        }
    })
}
