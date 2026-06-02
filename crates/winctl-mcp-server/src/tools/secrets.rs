use crate::{AppState, MacroTypeSecretRequest, SecretDeleteRequest, SecretSetRequest};
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

pub fn macro_type_secret(state: &AppState, request: MacroTypeSecretRequest) -> serde_json::Value {
    tracing::info!(
        bound_id = %request.bound_id,
        has_secret_ref = !request.secret_ref.trim().is_empty(),
        "macro.type_secret requested"
    );
    let secret_ref = request.secret_ref.trim().to_owned();
    if secret_ref.is_empty() {
        return serde_json::json!({
            "ok": false,
            "error": {
                "code": "secret_ref_required",
                "message": "macro.type_secret requires a non-empty secret_ref"
            }
        });
    }

    let secret = {
        let mut store = state.memory.lock().expect("memory store mutex poisoned");
        match store.resolve_secret_plaintext(&secret_ref) {
            Ok(Some(secret)) => secret,
            Ok(None) => {
                tracing::warn!("macro.type_secret secret was not found");
                return serde_json::json!({
                    "ok": false,
                    "error": {
                        "code": "secret_not_found",
                        "message": "secret was not found"
                    }
                });
            }
            Err(error) => {
                tracing::warn!(error = %error, "macro.type_secret failed to resolve secret");
                return serde_json::json!({
                    "ok": false,
                    "error": secret_error("secret_resolve_failed", error)
                });
            }
        }
    };

    crate::tools::input::input_type_secret_value(state, request.bound_id, &secret)
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
