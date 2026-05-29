use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::{
    AppState, ArtifactExportRequest, ClipboardReadRequest, ClipboardWriteRequest,
    FilesystemCopyRequest, FilesystemDeleteRequest, FilesystemListRequest, FilesystemMoveRequest,
    FilesystemReadRequest, FilesystemSearchRequest, NotificationsListRequest,
    ProcessDiagnosticsRequest, RegistryDeleteRequest, RegistryListRequest, RegistryReadRequest,
    RegistryWriteRequest,
};
use winctl::{RegistryHive, RegistryValueKind};

const DEFAULT_MAX_READ_BYTES: usize = 64 * 1024;
const DEFAULT_MAX_ENTRIES: usize = 500;
const DEFAULT_MAX_RESULTS: usize = 100;

pub fn clipboard_read(_state: &AppState, request: ClipboardReadRequest) -> serde_json::Value {
    tracing::info!(
        max_chars = ?request.max_chars,
        "clipboard.read requested"
    );
    match winctl::clipboard_read_text() {
        Ok(mut clipboard) => {
            let truncated = if let (Some(text), Some(max_chars)) =
                (clipboard.text.as_mut(), request.max_chars)
            {
                let original_chars = text.chars().count();
                if original_chars > max_chars {
                    *text = text.chars().take(max_chars).collect();
                    true
                } else {
                    false
                }
            } else {
                false
            };
            serde_json::json!({
                "ok": true,
                "clipboard": clipboard,
                "truncated": truncated,
            })
        }
        Err(error) => {
            tracing::warn!(error_code = %error.code, "clipboard.read failed");
            serde_json::json!({"ok": false, "error": error})
        }
    }
}

pub fn clipboard_write(_state: &AppState, request: ClipboardWriteRequest) -> serde_json::Value {
    tracing::info!(
        chars = request.text.chars().count(),
        "clipboard.write requested"
    );
    if !env_flag("WINCTL_ENABLE_CLIPBOARD_WRITE") {
        return denied(
            "clipboard_write_disabled",
            "clipboard.write requires WINCTL_ENABLE_CLIPBOARD_WRITE=1",
        );
    }
    match winctl::clipboard_write_text(&request.text) {
        Ok(result) => serde_json::json!({"ok": true, "result": result}),
        Err(error) => {
            tracing::warn!(error_code = %error.code, "clipboard.write failed");
            serde_json::json!({"ok": false, "error": error})
        }
    }
}

pub fn filesystem_read(state: &AppState, request: FilesystemReadRequest) -> serde_json::Value {
    tracing::info!(path = %request.path, max_bytes = ?request.max_bytes, "filesystem.read requested");
    let path = match resolve_allowed_existing_path(state, &request.path) {
        Ok(path) => path,
        Err(error) => return error,
    };
    let max_bytes = request
        .max_bytes
        .unwrap_or(DEFAULT_MAX_READ_BYTES)
        .min(1024 * 1024);
    match read_limited(&path, max_bytes) {
        Ok((bytes, truncated)) => {
            let text = String::from_utf8(bytes.clone()).ok();
            serde_json::json!({
                "ok": true,
                "path": path,
                "bytes_read": bytes.len(),
                "truncated": truncated,
                "utf8": text.is_some(),
                "text": text,
                "warnings": if text.is_some() { Vec::<String>::new() } else { vec!["file bytes are not valid UTF-8; text omitted".into()] },
            })
        }
        Err(error) => io_error("filesystem_read_failed", &path, error),
    }
}

pub fn filesystem_list(state: &AppState, request: FilesystemListRequest) -> serde_json::Value {
    tracing::info!(
        path = %request.path,
        recursive = request.recursive,
        max_entries = ?request.max_entries,
        "filesystem.list requested"
    );
    let root = match resolve_allowed_existing_path(state, &request.path) {
        Ok(path) => path,
        Err(error) => return error,
    };
    let max_entries = request.max_entries.unwrap_or(DEFAULT_MAX_ENTRIES).min(5000);
    let mut entries = Vec::new();
    let mut warnings = Vec::new();
    list_entries(
        &root,
        request.recursive,
        max_entries,
        &mut entries,
        &mut warnings,
    );
    serde_json::json!({
        "ok": true,
        "root": root,
        "entries": entries,
        "truncated": entries.len() >= max_entries,
        "warnings": warnings,
    })
}

pub fn filesystem_search(state: &AppState, request: FilesystemSearchRequest) -> serde_json::Value {
    tracing::info!(
        root = %request.root,
        pattern = %request.pattern,
        max_results = ?request.max_results,
        "filesystem.search requested"
    );
    let root = match resolve_allowed_existing_path(state, &request.root) {
        Ok(path) => path,
        Err(error) => return error,
    };
    let max_results = request.max_results.unwrap_or(DEFAULT_MAX_RESULTS).min(1000);
    let max_file_bytes = request
        .max_file_bytes
        .unwrap_or(DEFAULT_MAX_READ_BYTES)
        .min(1024 * 1024);
    let mut results = Vec::new();
    let mut warnings = Vec::new();
    search_entries(
        &root,
        &request.pattern,
        max_results,
        max_file_bytes,
        &mut results,
        &mut warnings,
    );
    serde_json::json!({
        "ok": true,
        "root": root,
        "pattern": request.pattern,
        "results": results,
        "truncated": results.len() >= max_results,
        "warnings": warnings,
    })
}

pub fn filesystem_copy(state: &AppState, request: FilesystemCopyRequest) -> serde_json::Value {
    tracing::info!(from = %request.from, to = %request.to, overwrite = request.overwrite, "filesystem.copy requested");
    if !env_flag("WINCTL_ENABLE_FILESYSTEM_MUTATION") {
        return denied(
            "filesystem_mutation_disabled",
            "filesystem.copy requires WINCTL_ENABLE_FILESYSTEM_MUTATION=1",
        );
    }
    let from = match resolve_allowed_existing_path(state, &request.from) {
        Ok(path) => path,
        Err(error) => return error,
    };
    let to = match resolve_allowed_write_path(state, &request.to) {
        Ok(path) => path,
        Err(error) => return error,
    };
    if to.exists() && !request.overwrite {
        return denied(
            "destination_exists",
            "destination exists and overwrite was false",
        );
    }
    match fs::copy(&from, &to) {
        Ok(bytes) => serde_json::json!({"ok": true, "from": from, "to": to, "bytes": bytes}),
        Err(error) => io_error("filesystem_copy_failed", &to, error),
    }
}

pub fn filesystem_move(state: &AppState, request: FilesystemMoveRequest) -> serde_json::Value {
    tracing::info!(from = %request.from, to = %request.to, overwrite = request.overwrite, "filesystem.move requested");
    if !env_flag("WINCTL_ENABLE_FILESYSTEM_MUTATION") {
        return denied(
            "filesystem_mutation_disabled",
            "filesystem.move requires WINCTL_ENABLE_FILESYSTEM_MUTATION=1",
        );
    }
    let from = match resolve_allowed_existing_path(state, &request.from) {
        Ok(path) => path,
        Err(error) => return error,
    };
    let to = match resolve_allowed_write_path(state, &request.to) {
        Ok(path) => path,
        Err(error) => return error,
    };
    if to.exists() && !request.overwrite {
        return denied(
            "destination_exists",
            "destination exists and overwrite was false",
        );
    }
    if to.exists() {
        if let Err(error) = fs::remove_file(&to) {
            return io_error("filesystem_overwrite_failed", &to, error);
        }
    }
    match fs::rename(&from, &to) {
        Ok(()) => serde_json::json!({"ok": true, "from": from, "to": to}),
        Err(error) => io_error("filesystem_move_failed", &to, error),
    }
}

pub fn filesystem_delete(state: &AppState, request: FilesystemDeleteRequest) -> serde_json::Value {
    tracing::info!(path = %request.path, recursive = request.recursive, "filesystem.delete requested");
    if !env_flag("WINCTL_ENABLE_FILESYSTEM_MUTATION") {
        return denied(
            "filesystem_mutation_disabled",
            "filesystem.delete requires WINCTL_ENABLE_FILESYSTEM_MUTATION=1",
        );
    }
    let path = match resolve_allowed_existing_path(state, &request.path) {
        Ok(path) => path,
        Err(error) => return error,
    };
    let result = if path.is_dir() {
        if request.recursive {
            fs::remove_dir_all(&path)
        } else {
            fs::remove_dir(&path)
        }
    } else {
        fs::remove_file(&path)
    };
    match result {
        Ok(()) => serde_json::json!({"ok": true, "path": path, "deleted": true}),
        Err(error) => io_error("filesystem_delete_failed", &path, error),
    }
}

pub fn artifact_export(state: &AppState, request: ArtifactExportRequest) -> serde_json::Value {
    tracing::info!(
        source_path = %request.source_path,
        destination_path = ?request.destination_path,
        "artifact.export requested"
    );
    let source = match resolve_allowed_existing_path(state, &request.source_path) {
        Ok(path) => path,
        Err(error) => return error,
    };
    let destination = match request.destination_path {
        Some(path) => match resolve_allowed_write_path(state, &path) {
            Ok(path) => path,
            Err(error) => return error,
        },
        None => {
            let export_dir = state.capture_dir.join("exports");
            if let Err(error) = fs::create_dir_all(&export_dir) {
                return io_error("artifact_export_dir_failed", &export_dir, error);
            }
            export_dir.join(source.file_name().unwrap_or_default())
        }
    };
    match fs::copy(&source, &destination) {
        Ok(bytes) => serde_json::json!({
            "ok": true,
            "source_path": source,
            "destination_path": destination,
            "bytes": bytes,
        }),
        Err(error) => io_error("artifact_export_failed", &destination, error),
    }
}

pub fn registry_list(_state: &AppState, request: RegistryListRequest) -> serde_json::Value {
    tracing::info!(hive = ?request.hive, path = %request.path, "registry.list requested");
    match winctl::registry_list(request.hive, &request.path, request.include_values) {
        Ok(listing) => serde_json::json!({"ok": true, "listing": listing}),
        Err(error) => serde_json::json!({"ok": false, "error": error}),
    }
}

pub fn registry_read(_state: &AppState, request: RegistryReadRequest) -> serde_json::Value {
    tracing::info!(hive = ?request.hive, path = %request.path, name = ?request.name, "registry.read requested");
    match winctl::registry_read(request.hive, &request.path, request.name.as_deref()) {
        Ok(value) => serde_json::json!({"ok": true, "value": value}),
        Err(error) => serde_json::json!({"ok": false, "error": error}),
    }
}

pub fn registry_write(_state: &AppState, request: RegistryWriteRequest) -> serde_json::Value {
    tracing::info!(hive = ?request.hive, path = %request.path, name = ?request.name, kind = ?request.kind, "registry.write requested");
    if !env_flag("WINCTL_ENABLE_REGISTRY_MUTATION") {
        return denied(
            "registry_mutation_disabled",
            "registry.write requires WINCTL_ENABLE_REGISTRY_MUTATION=1",
        );
    }
    match winctl::registry_write(
        request.hive,
        &request.path,
        request.name.as_deref(),
        request.kind,
        &request.data,
    ) {
        Ok(value) => serde_json::json!({"ok": true, "value": value}),
        Err(error) => serde_json::json!({"ok": false, "error": error}),
    }
}

pub fn registry_delete(_state: &AppState, request: RegistryDeleteRequest) -> serde_json::Value {
    tracing::info!(hive = ?request.hive, path = %request.path, name = ?request.name, "registry.delete requested");
    if !env_flag("WINCTL_ENABLE_REGISTRY_MUTATION") {
        return denied(
            "registry_mutation_disabled",
            "registry.delete requires WINCTL_ENABLE_REGISTRY_MUTATION=1",
        );
    }
    match winctl::registry_delete(request.hive, &request.path, request.name.as_deref()) {
        Ok(()) => serde_json::json!({"ok": true, "deleted": true}),
        Err(error) => serde_json::json!({"ok": false, "error": error}),
    }
}

pub fn notifications_list(
    _state: &AppState,
    request: NotificationsListRequest,
) -> serde_json::Value {
    tracing::info!(max_items = ?request.max_items, "notifications.list requested");
    serde_json::json!({
        "ok": true,
        "notifications": [],
        "warnings": [
            "Windows notification inspection requires a dedicated notification provider; no provider is enabled in this build"
        ],
        "provider_enabled": false
    })
}

pub fn process_diagnostics(
    state: &AppState,
    request: ProcessDiagnosticsRequest,
) -> serde_json::Value {
    tracing::info!(pid = request.pid, "process.diagnostics requested");
    let process = match winctl::describe_process(request.pid) {
        Ok(process) => process,
        Err(error) => return serde_json::json!({"ok": false, "error": error}),
    };
    let windows: Vec<_> = if request.include_windows {
        winctl::list_windows()
            .into_iter()
            .filter(|window| window.pid == request.pid)
            .collect()
    } else {
        Vec::new()
    };
    let children = if request.include_children {
        winctl::child_processes(request.pid).unwrap_or_default()
    } else {
        Vec::new()
    };
    let tracked = state.tracked_by_pid(request.pid);
    serde_json::json!({
        "ok": process.is_some(),
        "process": process,
        "tracked_by_server": tracked.is_some(),
        "launch_id": tracked.map(|tracked| tracked.launch_id),
        "windows": windows,
        "children": children,
        "warnings": [],
    })
}

fn resolve_allowed_existing_path(
    state: &AppState,
    path: &str,
) -> Result<PathBuf, serde_json::Value> {
    let path = PathBuf::from(path);
    let canonical = fs::canonicalize(&path)
        .map_err(|error| io_error("filesystem_path_unavailable", &path, error))?;
    ensure_allowed(state, &canonical)?;
    Ok(canonical)
}

fn resolve_allowed_write_path(state: &AppState, path: &str) -> Result<PathBuf, serde_json::Value> {
    let path = PathBuf::from(path);
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let parent = fs::canonicalize(parent)
        .map_err(|error| io_error("filesystem_parent_unavailable", parent, error))?;
    ensure_allowed(state, &parent)?;
    Ok(parent.join(path.file_name().unwrap_or_default()))
}

fn ensure_allowed(state: &AppState, path: &Path) -> Result<(), serde_json::Value> {
    let roots = allowed_roots(state);
    if roots.iter().any(|root| path.starts_with(root)) {
        Ok(())
    } else {
        Err(serde_json::json!({
            "ok": false,
            "error": {
                "code": "filesystem_path_not_allowed",
                "message": format!("path {} is outside configured filesystem roots", path.display()),
                "allowed_roots": roots,
            }
        }))
    }
}

fn allowed_roots(state: &AppState) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = std::env::var("WINCTL_FS_ROOTS")
        .ok()
        .into_iter()
        .flat_map(|value| {
            value
                .split(';')
                .map(str::trim)
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .filter(|value| !value.is_empty())
        .filter_map(|value| fs::canonicalize(value).ok())
        .collect();
    if let Ok(path) = fs::canonicalize(state.capture_dir.as_ref()) {
        roots.push(path);
    }
    if let Ok(path) = fs::canonicalize(std::env::temp_dir()) {
        roots.push(path);
    }
    roots.sort();
    roots.dedup();
    roots
}

fn read_limited(path: &Path, max_bytes: usize) -> std::io::Result<(Vec<u8>, bool)> {
    let mut file = fs::File::open(path)?;
    let mut bytes = Vec::with_capacity(max_bytes.min(8192));
    let mut handle = file.by_ref().take((max_bytes + 1) as u64);
    handle.read_to_end(&mut bytes)?;
    let truncated = bytes.len() > max_bytes;
    bytes.truncate(max_bytes);
    Ok((bytes, truncated))
}

fn list_entries(
    root: &Path,
    recursive: bool,
    max_entries: usize,
    entries: &mut Vec<serde_json::Value>,
    warnings: &mut Vec<String>,
) {
    if entries.len() >= max_entries {
        return;
    }
    let read_dir = match fs::read_dir(root) {
        Ok(read_dir) => read_dir,
        Err(error) => {
            warnings.push(format!("failed to list {}: {error}", root.display()));
            return;
        }
    };
    for entry in read_dir.flatten() {
        if entries.len() >= max_entries {
            break;
        }
        let path = entry.path();
        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(error) => {
                warnings.push(format!(
                    "failed to read metadata for {}: {error}",
                    path.display()
                ));
                continue;
            }
        };
        let is_dir = metadata.is_dir();
        entries.push(serde_json::json!({
            "path": path,
            "file_name": entry.file_name().to_string_lossy(),
            "is_dir": is_dir,
            "is_file": metadata.is_file(),
            "len": metadata.len(),
        }));
        if recursive && is_dir {
            list_entries(&path, recursive, max_entries, entries, warnings);
        }
    }
}

fn search_entries(
    root: &Path,
    pattern: &str,
    max_results: usize,
    max_file_bytes: usize,
    results: &mut Vec<serde_json::Value>,
    warnings: &mut Vec<String>,
) {
    if results.len() >= max_results {
        return;
    }
    let read_dir = match fs::read_dir(root) {
        Ok(read_dir) => read_dir,
        Err(error) => {
            warnings.push(format!("failed to list {}: {error}", root.display()));
            return;
        }
    };
    for entry in read_dir.flatten() {
        if results.len() >= max_results {
            break;
        }
        let path = entry.path();
        if path.is_dir() {
            search_entries(
                &path,
                pattern,
                max_results,
                max_file_bytes,
                results,
                warnings,
            );
            continue;
        }
        let file_name_matches = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.contains(pattern))
            .unwrap_or(false);
        let content_match = read_limited(&path, max_file_bytes)
            .ok()
            .and_then(|(bytes, _)| String::from_utf8(bytes).ok())
            .and_then(|text| {
                text.lines()
                    .enumerate()
                    .find(|(_, line)| line.contains(pattern))
                    .map(|(index, line)| serde_json::json!({"line": index + 1, "text": line}))
            });
        if file_name_matches || content_match.is_some() {
            results.push(serde_json::json!({
                "path": path,
                "file_name_matches": file_name_matches,
                "content_match": content_match,
            }));
        }
    }
}

fn io_error(code: &str, path: &Path, error: std::io::Error) -> serde_json::Value {
    serde_json::json!({
        "ok": false,
        "error": {
            "code": code,
            "message": format!("{code} for {}: {error}", path.display()),
        }
    })
}

fn denied(code: &str, message: &str) -> serde_json::Value {
    tracing::warn!(code = code, "system utility request denied by policy");
    serde_json::json!({
        "ok": false,
        "error": {
            "code": code,
            "message": message,
        }
    })
}

fn env_flag(name: &str) -> bool {
    std::env::var(name)
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

#[allow(dead_code)]
fn _keep_schema_types(_: RegistryHive, _: RegistryValueKind) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_allowed_roots_include_capture_dir() {
        let state = AppState::with_capture_dir(std::env::temp_dir());
        let roots = allowed_roots(&state);
        assert!(roots.iter().any(|root| root == &std::env::temp_dir()));
    }
}
