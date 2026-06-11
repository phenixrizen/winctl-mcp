//! Dashboard server-config editor: pure helpers for loading the editable view of
//! the server config, merging dashboard edits into the existing TOML, validating,
//! serializing, and atomically persisting it — plus locating the tray binary and
//! building the `winctl-tray restart` command. No I/O happens except in
//! `write_atomic`/`spawn_restart`; everything else is pure for testability.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::{
    EmbeddingFileConfig, LoggingFileConfig, MacroExecutionFileConfig, PathsFileConfig,
    PolicyFileConfig, SecurityPolicy, ServeConfig, WinctlConfigFile,
};

/// Editable operational sections accepted from the dashboard. Reuses the
/// existing file-config section types so there is one source of truth.
#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct ConfigSaveBody {
    #[serde(default)]
    pub policy: Option<PolicyFileConfig>,
    #[serde(default)]
    pub paths: Option<PathsFileConfig>,
    #[serde(default)]
    pub logging: Option<LoggingFileConfig>,
    #[serde(default)]
    pub embedding: Option<EmbeddingFileConfig>,
    #[serde(default)]
    pub macro_execution: Option<MacroExecutionFileConfig>,
}

/// A single validation problem, surfaced per-field to the UI.
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct FieldError {
    pub field: String,
    pub message: String,
}

/// Overlay the editable sections from `edits` onto a copy of `existing`,
/// leaving `[transport]`, `[auth]`, and any other untouched fields intact.
pub(crate) fn merge_editable(existing: &WinctlConfigFile, edits: &ConfigSaveBody) -> WinctlConfigFile {
    let mut merged = existing.clone();
    if edits.policy.is_some() {
        merged.policy = edits.policy.clone();
    }
    if edits.paths.is_some() {
        merged.paths = edits.paths.clone();
    }
    if edits.logging.is_some() {
        merged.logging = edits.logging.clone();
    }
    if edits.embedding.is_some() {
        merged.embedding = edits.embedding.clone();
    }
    if edits.macro_execution.is_some() {
        merged.macro_execution = edits.macro_execution.clone();
    }
    merged
}

/// Validate a candidate config file. Returns the field errors (empty = valid).
pub(crate) fn validate_editable(file: &WinctlConfigFile) -> Vec<FieldError> {
    let mut errors = Vec::new();

    if let Some(embedding) = &file.embedding {
        if matches!(embedding.dimension, Some(0)) {
            errors.push(FieldError {
                field: "embedding.dimension".to_string(),
                message: "embedding dimension must be greater than zero".to_string(),
            });
        }
    }
    if let Some(paths) = &file.paths {
        if let Some(roots) = &paths.filesystem_roots {
            for (idx, root) in roots.iter().enumerate() {
                if root.as_os_str().is_empty() {
                    errors.push(FieldError {
                        field: format!("paths.filesystem_roots[{idx}]"),
                        message: "filesystem root must not be empty".to_string(),
                    });
                }
            }
        }
    }

    // Whole-file sanity: it must still resolve to a valid ServeConfig
    // (catches an unparsable listen, etc., even though those fields are locked).
    if let Err(error) = ServeConfig::from_file(file) {
        errors.push(FieldError {
            field: "config".to_string(),
            message: format!("{error:#}"),
        });
    }

    errors
}

/// Serialize a config file to pretty TOML.
pub(crate) fn serialize_config(file: &WinctlConfigFile) -> anyhow::Result<String> {
    Ok(toml::to_string_pretty(file)?)
}

/// Atomically replace `path` with `contents` (temp sibling + rename).
pub(crate) fn write_atomic(path: &Path, contents: &str) -> std::io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path) // MoveFileExW with REPLACE_EXISTING on Windows
}

/// Build the four editable operational sections from the live in-memory policy,
/// used for the read-only view when the server has no `--config` file.
pub(crate) fn sections_from_policy(
    policy: &SecurityPolicy,
    capture_dir: &Path,
) -> (
    PolicyFileConfig,
    PathsFileConfig,
    EmbeddingFileConfig,
    MacroExecutionFileConfig,
) {
    let policy_section = PolicyFileConfig {
        enable_filesystem_mutation: Some(policy.enable_filesystem_mutation),
        enable_clipboard_write: Some(policy.enable_clipboard_write),
        enable_registry_mutation: Some(policy.enable_registry_mutation),
        allow_private_network: Some(policy.allow_private_network),
        memory_mutation_enabled: Some(policy.memory_mutation_enabled),
        macro_execution_enabled: Some(policy.macro_execution_enabled),
        macro_destructive_tools_allowed: Some(policy.macro_destructive_tools_allowed),
        max_macro_runtime_ms: policy.max_macro_runtime_ms,
        max_macro_steps: policy.max_macro_steps,
        screenshot_retention_count: policy.screenshot_retention_count,
        tool_allowlist: Some(policy.tool_allowlist.clone()),
        tool_denylist: Some(policy.tool_denylist.clone()),
    };
    let paths_section = PathsFileConfig {
        capture_dir: Some(capture_dir.to_path_buf()),
        artifact_dir: policy.artifact_dir.clone(),
        filesystem_roots: Some(policy.filesystem_roots.clone()),
        memory_db: None,
    };
    let embedding_section = EmbeddingFileConfig {
        model_path: policy.embedding_model_path.clone(),
        dimension: policy.embedding_dimension,
    };
    // `macro_execution_enabled` and `macro_destructive_tools_allowed` appear in
    // both `policy_section` and `macro_section` because the TOML file carries
    // them under both `[policy]` and `[macro_execution]`. `merge_editable`
    // replaces each section atomically, so both stay in sync after a save.
    let macro_section = MacroExecutionFileConfig {
        enabled: Some(policy.macro_execution_enabled),
        allow_destructive_tools: Some(policy.macro_destructive_tools_allowed),
        max_runtime_ms: policy.max_macro_runtime_ms,
        max_steps: policy.max_macro_steps,
    };
    (policy_section, paths_section, embedding_section, macro_section)
}

/// Name of the tray binary as built by the workspace (sibling of the server exe).
#[cfg(windows)]
const TRAY_BINARY: &str = "winctl-tray.exe";
#[cfg(not(windows))]
const TRAY_BINARY: &str = "winctl-tray";

/// Return the tray binary path inside `dir` if it exists. Pure (no `current_exe`),
/// so it can be tested deterministically.
fn tray_binary_in(dir: &Path) -> Option<PathBuf> {
    let candidate = dir.join(TRAY_BINARY);
    candidate.is_file().then_some(candidate)
}

/// Locate the tray binary next to the running server executable.
pub(crate) fn find_tray_binary() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    tray_binary_in(exe.parent()?)
}

/// Build (but do not spawn) `winctl-tray restart --config <config_file>`.
pub(crate) fn build_restart_command(tray: &Path, config_file: &Path) -> std::process::Command {
    let mut command = std::process::Command::new(tray);
    command.arg("restart").arg("--config").arg(config_file);
    command
}

/// Spawn the tray restart command fully detached so it survives this process
/// being killed by the very restart it triggers.
pub(crate) fn spawn_restart(tray: &Path, config_file: &Path) -> anyhow::Result<()> {
    use anyhow::Context as _;
    use std::process::Stdio;

    let mut command = build_restart_command(tray, config_file);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    detach(&mut command);
    command
        .spawn()
        .with_context(|| format!("failed to spawn {}", tray.display()))?;
    Ok(())
}

#[cfg(windows)]
fn detach(command: &mut std::process::Command) {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    command.creation_flags(DETACHED_PROCESS);
}

#[cfg(not(windows))]
fn detach(_command: &mut std::process::Command) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_file() -> WinctlConfigFile {
        let text = "\
[transport]
mode = \"http\"
listen = \"127.0.0.1:8765\"

[auth]
token = \"keep-me\"

[policy]
enable_filesystem_mutation = false
";
        toml::from_str(text).expect("base file parses")
    }

    #[test]
    fn merge_preserves_transport_and_auth_and_replaces_policy() {
        let existing = base_file();
        let edits = ConfigSaveBody {
            policy: Some(PolicyFileConfig {
                enable_filesystem_mutation: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        };
        let merged = merge_editable(&existing, &edits);
        assert_eq!(
            merged.transport.as_ref().unwrap().listen.as_deref(),
            Some("127.0.0.1:8765")
        );
        assert_eq!(merged.auth.as_ref().unwrap().token.as_deref(), Some("keep-me"));
        assert_eq!(
            merged.policy.as_ref().unwrap().enable_filesystem_mutation,
            Some(true)
        );
    }

    #[test]
    fn serialize_round_trips_and_keeps_locked_sections() {
        let existing = base_file();
        let edits = ConfigSaveBody {
            policy: Some(PolicyFileConfig {
                enable_registry_mutation: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        };
        let merged = merge_editable(&existing, &edits);
        let text = serialize_config(&merged).expect("serialize");
        assert!(text.contains("[auth]"));
        assert!(text.contains("keep-me"));
        let reparsed: WinctlConfigFile = toml::from_str(&text).expect("reparse");
        assert_eq!(
            reparsed.policy.unwrap().enable_registry_mutation,
            Some(true)
        );
    }

    #[test]
    fn validate_rejects_zero_dimension() {
        let mut file = base_file();
        file.embedding = Some(EmbeddingFileConfig {
            model_path: None,
            dimension: Some(0),
        });
        let errors = validate_editable(&file);
        assert!(errors.iter().any(|e| e.field == "embedding.dimension"));
    }

    #[test]
    fn validate_accepts_clean_file() {
        assert!(validate_editable(&base_file()).is_empty());
    }

    #[test]
    fn write_atomic_replaces_destination() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("winctl-config-editor-test-{}.toml", std::process::id()));
        write_atomic(&path, "[policy]\nenable_filesystem_mutation = true\n").expect("first write");
        write_atomic(&path, "[policy]\nenable_filesystem_mutation = false\n").expect("second write");
        let read_back = std::fs::read_to_string(&path).expect("read back");
        assert!(read_back.contains("enable_filesystem_mutation = false"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn restart_command_targets_tray_restart_with_config() {
        let tray = Path::new("/opt/winctl/winctl-tray");
        let config = Path::new("/etc/winctl/config.toml");
        let cmd = build_restart_command(tray, config);
        let prog = cmd.get_program().to_string_lossy().to_string();
        let args: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert!(prog.ends_with("winctl-tray"));
        assert_eq!(args, vec!["restart", "--config", "/etc/winctl/config.toml"]);
    }

    #[test]
    fn tray_binary_in_returns_none_when_absent() {
        let dir = std::env::temp_dir().join(format!("winctl-tray-none-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        assert!(tray_binary_in(&dir).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tray_binary_in_finds_sibling() {
        let dir = std::env::temp_dir().join(format!("winctl-tray-some-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let tray = dir.join(TRAY_BINARY);
        std::fs::write(&tray, b"#!/bin/sh\n").expect("write dummy tray");
        assert_eq!(tray_binary_in(&dir), Some(tray));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
