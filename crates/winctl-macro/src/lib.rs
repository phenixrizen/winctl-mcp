use std::collections::HashSet;

use chrono::Utc;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

pub const MACRO_MANIFEST_VERSION: &str = "winctl.macro.v1";

#[derive(Debug, Error)]
pub enum MacroError {
    #[error("macro manifest validation failed")]
    Validation,
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct MacroManifest {
    pub version: String,
    pub kind: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub app_identity: Option<AppIdentity>,
    #[serde(default)]
    pub launch: Option<ToolCall>,
    #[serde(default)]
    pub bind: Option<BindingStrategy>,
    #[serde(default)]
    pub preconditions: Vec<MacroStep>,
    #[serde(default)]
    pub steps: Vec<MacroStep>,
    #[serde(default)]
    pub waits: Vec<MacroStep>,
    #[serde(default)]
    pub assertions: Vec<MacroStep>,
    #[serde(default)]
    pub cleanup: Vec<MacroStep>,
    #[serde(default)]
    pub artifacts: ArtifactPolicy,
    #[serde(default)]
    pub replay: ReplayMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct AppIdentity {
    pub executable_name: Option<String>,
    pub executable_path: Option<String>,
    pub product_name: Option<String>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub extra: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct ToolCall {
    pub tool: String,
    #[serde(default)]
    pub args: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct BindingStrategy {
    pub strategy: String,
    pub required_executable: Option<String>,
    #[serde(default)]
    pub expected_identity_json: Option<Value>,
    #[serde(default)]
    pub allow_child_process_windows: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct MacroStep {
    pub id: String,
    pub tool: String,
    #[serde(default)]
    pub args: Value,
    #[serde(default)]
    pub target: Option<MacroTarget>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    #[serde(default = "default_required")]
    pub required: bool,
    #[serde(default)]
    pub continue_on_failure: bool,
    #[serde(default)]
    pub coordinate_fallback: Option<CoordinateFallbackMetadata>,
    #[serde(default)]
    pub audit: StepAudit,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MacroTarget {
    BoundWindow {
        bound_id: Option<String>,
    },
    LaunchedProcessWindow {
        launch_id: Option<String>,
        pid: Option<u32>,
        hwnd: Option<String>,
    },
    UiElement(UiElementTarget),
    ImageCheckpoint {
        checkpoint_id: String,
        path: Option<String>,
        checksum_sha256: Option<String>,
    },
    TextCheckpoint {
        text: String,
    },
    Coordinates {
        x: i32,
        y: i32,
        coordinate_space: CoordinateSpace,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct UiElementTarget {
    pub element_ref: Option<String>,
    pub name: Option<String>,
    pub role: Option<String>,
    pub control_type: Option<String>,
    pub automation_id: Option<String>,
    pub class_name: Option<String>,
    pub hierarchy_path: Option<Vec<usize>>,
    pub visible_text: Option<String>,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CoordinateSpace {
    WindowClient,
    WindowVirtualDesktop,
    VirtualDesktop,
    Screenshot,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct CoordinateFallbackMetadata {
    pub monitor_id: Option<String>,
    pub virtual_desktop_x: i32,
    pub virtual_desktop_y: i32,
    pub window_rect: Rect,
    pub client_rect: Option<Rect>,
    pub dpi: Option<u32>,
    pub scale_factor: Option<f64>,
    pub screenshot_size: Option<Size>,
    #[serde(default)]
    pub original_resolved_target: Value,
    #[serde(default)]
    pub preflight_validation: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Default)]
pub struct StepAudit {
    #[serde(default)]
    pub expected_identity_json: Option<Value>,
    #[serde(default)]
    pub resolved_target_json: Option<Value>,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct ArtifactPolicy {
    #[serde(default)]
    pub screenshots: bool,
    #[serde(default)]
    pub step_results: bool,
    #[serde(default)]
    pub resolved_targets: bool,
    #[serde(default)]
    pub uia_snapshots: bool,
    #[serde(default)]
    pub logs: bool,
    #[serde(default)]
    pub retention_days: Option<u32>,
}

impl Default for ArtifactPolicy {
    fn default() -> Self {
        Self {
            screenshots: true,
            step_results: true,
            resolved_targets: true,
            uia_snapshots: false,
            logs: true,
            retention_days: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Default)]
pub struct ReplayMetadata {
    pub created_at: Option<String>,
    pub last_run_at: Option<String>,
    pub last_success_at: Option<String>,
    pub version_note: Option<String>,
    #[serde(default)]
    pub source_memory_id: Option<String>,
    #[serde(default)]
    pub extra: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct MacroValidationReport {
    pub valid: bool,
    pub issues: Vec<MacroValidationIssue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct MacroValidationIssue {
    pub severity: MacroValidationSeverity,
    pub code: String,
    pub message: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MacroValidationSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct MacroPlan {
    pub valid: bool,
    pub generated_at: String,
    pub manifest_version: String,
    pub title: String,
    pub steps: Vec<MacroPlanStep>,
    pub issues: Vec<MacroValidationIssue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct MacroPlanStep {
    pub id: String,
    pub section: MacroSection,
    pub tool: String,
    pub mutates_ui: bool,
    pub requires_bound_window: bool,
    pub produces_artifact: bool,
    pub category: String,
    pub target_strategy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MacroSection {
    Launch,
    Bind,
    Precondition,
    Step,
    Wait,
    Assertion,
    Cleanup,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct MacroExecutionResult {
    pub manifest_title: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub status: MacroExecutionStatus,
    #[serde(default)]
    pub step_results: Vec<MacroStepResult>,
    #[serde(default)]
    pub artifacts: Vec<MacroArtifact>,
    #[serde(default)]
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MacroExecutionStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    Aborted,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct MacroStepResult {
    pub step_id: String,
    pub tool: String,
    pub status: MacroStepStatus,
    pub started_at: String,
    pub finished_at: Option<String>,
    #[serde(default)]
    pub output: Value,
    #[serde(default)]
    pub diagnostics: Vec<String>,
    #[serde(default)]
    pub artifacts: Vec<MacroArtifact>,
    #[serde(default)]
    pub error: Option<MacroStepError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MacroStepStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct MacroStepError {
    pub kind: MacroStepErrorKind,
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub diagnostics: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MacroStepErrorKind {
    ValidationFailure,
    TargetIdentityFailure,
    WaitTimeout,
    AssertionFailure,
    PolicyDenial,
    ToolExecutionFailure,
    UserAbort,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct MacroArtifact {
    pub kind: String,
    pub path: Option<String>,
    pub metadata: Value,
}

#[derive(Debug, Clone, Copy)]
pub struct ToolDescriptor {
    pub name: &'static str,
    pub category: &'static str,
    pub mutates_ui: bool,
    pub requires_bound_window: bool,
    pub produces_artifact: bool,
}

pub fn validate_manifest(manifest: &MacroManifest) -> MacroValidationReport {
    let mut issues = Vec::new();
    if manifest.version != MACRO_MANIFEST_VERSION {
        issues.push(error(
            "unsupported_version",
            format!(
                "manifest version must be {MACRO_MANIFEST_VERSION}, got {}",
                manifest.version
            ),
            "/version",
        ));
    }
    if manifest.kind.trim().is_empty() {
        issues.push(error("missing_kind", "manifest kind is required", "/kind"));
    }
    if manifest.title.trim().is_empty() {
        issues.push(error(
            "missing_title",
            "manifest title is required",
            "/title",
        ));
    }
    if manifest.description.trim().is_empty() {
        issues.push(warning(
            "missing_description",
            "manifest description is recommended for auditability",
            "/description",
        ));
    }

    validate_optional_tool_call(
        manifest.launch.as_ref(),
        MacroSection::Launch,
        "/launch",
        &mut issues,
    );

    if manifest.steps.is_empty()
        && manifest.preconditions.is_empty()
        && manifest.waits.is_empty()
        && manifest.assertions.is_empty()
        && manifest.cleanup.is_empty()
    {
        issues.push(error(
            "missing_steps",
            "manifest must include at least one step, wait, assertion, precondition, or cleanup action",
            "/steps",
        ));
    }

    let mut ids = HashSet::new();
    validate_steps(
        manifest,
        &manifest.preconditions,
        MacroSection::Precondition,
        "/preconditions",
        &mut ids,
        &mut issues,
    );
    validate_steps(
        manifest,
        &manifest.steps,
        MacroSection::Step,
        "/steps",
        &mut ids,
        &mut issues,
    );
    validate_steps(
        manifest,
        &manifest.waits,
        MacroSection::Wait,
        "/waits",
        &mut ids,
        &mut issues,
    );
    validate_steps(
        manifest,
        &manifest.assertions,
        MacroSection::Assertion,
        "/assertions",
        &mut ids,
        &mut issues,
    );
    validate_steps(
        manifest,
        &manifest.cleanup,
        MacroSection::Cleanup,
        "/cleanup",
        &mut ids,
        &mut issues,
    );

    let valid = !issues
        .iter()
        .any(|issue| issue.severity == MacroValidationSeverity::Error);
    MacroValidationReport { valid, issues }
}

pub fn dry_run_plan(manifest: &MacroManifest) -> Result<MacroPlan, MacroValidationReport> {
    let report = validate_manifest(manifest);
    let steps = plan_steps(manifest);
    let plan = MacroPlan {
        valid: report.valid,
        generated_at: Utc::now().to_rfc3339(),
        manifest_version: manifest.version.clone(),
        title: manifest.title.clone(),
        steps,
        issues: report.issues.clone(),
    };
    if report.valid {
        Ok(plan)
    } else {
        Err(MacroValidationReport {
            valid: false,
            issues: plan.issues,
        })
    }
}

pub fn supported_tool_names() -> Vec<&'static str> {
    SUPPORTED_TOOLS.iter().map(|tool| tool.name).collect()
}

pub fn tool_descriptor(name: &str) -> Option<ToolDescriptor> {
    SUPPORTED_TOOLS
        .iter()
        .find(|descriptor| descriptor.name == name)
        .copied()
}

fn validate_optional_tool_call(
    call: Option<&ToolCall>,
    section: MacroSection,
    path: &str,
    issues: &mut Vec<MacroValidationIssue>,
) {
    let Some(call) = call else {
        return;
    };
    if call.tool.trim().is_empty() {
        issues.push(error(
            "missing_tool",
            "tool call must include a stable tool name",
            format!("{path}/tool"),
        ));
        return;
    }
    if tool_descriptor(&call.tool).is_none() {
        issues.push(error(
            "unknown_tool",
            format!("unsupported {section:?} tool {}", call.tool),
            format!("{path}/tool"),
        ));
    }
}

fn validate_steps(
    manifest: &MacroManifest,
    steps: &[MacroStep],
    section: MacroSection,
    path: &str,
    ids: &mut HashSet<String>,
    issues: &mut Vec<MacroValidationIssue>,
) {
    for (index, step) in steps.iter().enumerate() {
        let step_path = format!("{path}/{index}");
        if step.id.trim().is_empty() {
            issues.push(error(
                "missing_step_id",
                "macro step ID is required",
                format!("{step_path}/id"),
            ));
        } else if !ids.insert(step.id.clone()) {
            issues.push(error(
                "duplicate_step_id",
                format!("step ID {} is used more than once", step.id),
                format!("{step_path}/id"),
            ));
        }
        let Some(descriptor) = tool_descriptor(&step.tool) else {
            issues.push(error(
                "unknown_tool",
                format!("unsupported macro tool {}", step.tool),
                format!("{step_path}/tool"),
            ));
            continue;
        };
        if descriptor.requires_bound_window && !manifest_has_bound_identity(manifest) {
            issues.push(error(
                "target_identity_required",
                format!(
                    "step {} uses {} and requires app identity plus a binding strategy",
                    step.id, step.tool
                ),
                step_path.clone(),
            ));
        }
        if matches!(step.target, Some(MacroTarget::Coordinates { .. }))
            && step.coordinate_fallback.is_none()
        {
            issues.push(error(
                "coordinate_fallback_metadata_required",
                "absolute or fallback coordinates require monitor, DPI, window, screenshot, and preflight metadata",
                format!("{step_path}/coordinate_fallback"),
            ));
        }
        if section == MacroSection::Assertion && descriptor.mutates_ui {
            issues.push(error(
                "mutating_assertion",
                "assertion steps must not mutate the UI",
                step_path,
            ));
        }
    }
}

fn plan_steps(manifest: &MacroManifest) -> Vec<MacroPlanStep> {
    let mut steps = Vec::new();
    if let Some(launch) = &manifest.launch {
        if let Some(descriptor) = tool_descriptor(&launch.tool) {
            steps.push(plan_tool_call(
                "launch",
                MacroSection::Launch,
                &launch.tool,
                descriptor,
                None,
            ));
        }
    }
    if let Some(bind) = &manifest.bind {
        steps.push(MacroPlanStep {
            id: "bind".into(),
            section: MacroSection::Bind,
            tool: "windows.bind".into(),
            mutates_ui: false,
            requires_bound_window: false,
            produces_artifact: false,
            category: "window".into(),
            target_strategy: bind.strategy.clone(),
        });
    }
    for (section, source) in [
        (MacroSection::Precondition, &manifest.preconditions),
        (MacroSection::Step, &manifest.steps),
        (MacroSection::Wait, &manifest.waits),
        (MacroSection::Assertion, &manifest.assertions),
        (MacroSection::Cleanup, &manifest.cleanup),
    ] {
        for step in source {
            if let Some(descriptor) = tool_descriptor(&step.tool) {
                steps.push(plan_tool_call(
                    &step.id,
                    section.clone(),
                    &step.tool,
                    descriptor,
                    step.target.as_ref(),
                ));
            }
        }
    }
    steps
}

fn plan_tool_call(
    id: &str,
    section: MacroSection,
    tool: &str,
    descriptor: ToolDescriptor,
    target: Option<&MacroTarget>,
) -> MacroPlanStep {
    MacroPlanStep {
        id: id.into(),
        section,
        tool: tool.into(),
        mutates_ui: descriptor.mutates_ui,
        requires_bound_window: descriptor.requires_bound_window,
        produces_artifact: descriptor.produces_artifact,
        category: descriptor.category.into(),
        target_strategy: target_strategy(target),
    }
}

fn target_strategy(target: Option<&MacroTarget>) -> String {
    match target {
        Some(MacroTarget::BoundWindow { .. }) => "bound_window".into(),
        Some(MacroTarget::LaunchedProcessWindow { .. }) => "launched_process_window".into(),
        Some(MacroTarget::UiElement(_)) => "uia_element".into(),
        Some(MacroTarget::ImageCheckpoint { .. }) => "image_checkpoint".into(),
        Some(MacroTarget::TextCheckpoint { .. }) => "text_checkpoint".into(),
        Some(MacroTarget::Coordinates { .. }) => "coordinate_fallback".into(),
        None => "tool_arguments".into(),
    }
}

fn manifest_has_bound_identity(manifest: &MacroManifest) -> bool {
    let app_identity_required = manifest
        .app_identity
        .as_ref()
        .map(|identity| {
            identity.required
                && (identity.executable_name.is_some() || identity.executable_path.is_some())
        })
        .unwrap_or(false);
    let bind_identity = manifest
        .bind
        .as_ref()
        .map(|bind| bind.required_executable.is_some() || bind.expected_identity_json.is_some())
        .unwrap_or(false);
    app_identity_required && bind_identity
}

fn error(
    code: impl Into<String>,
    message: impl Into<String>,
    path: impl Into<String>,
) -> MacroValidationIssue {
    MacroValidationIssue {
        severity: MacroValidationSeverity::Error,
        code: code.into(),
        message: message.into(),
        path: path.into(),
    }
}

fn warning(
    code: impl Into<String>,
    message: impl Into<String>,
    path: impl Into<String>,
) -> MacroValidationIssue {
    MacroValidationIssue {
        severity: MacroValidationSeverity::Warning,
        code: code.into(),
        message: message.into(),
        path: path.into(),
    }
}

fn default_required() -> bool {
    true
}

const SUPPORTED_TOOLS: &[ToolDescriptor] = &[
    ToolDescriptor {
        name: "app.launch",
        category: "app",
        mutates_ui: true,
        requires_bound_window: false,
        produces_artifact: false,
    },
    ToolDescriptor {
        name: "process.launch",
        category: "process",
        mutates_ui: true,
        requires_bound_window: false,
        produces_artifact: false,
    },
    ToolDescriptor {
        name: "process.wait_for_exit",
        category: "process",
        mutates_ui: false,
        requires_bound_window: false,
        produces_artifact: false,
    },
    ToolDescriptor {
        name: "process.describe",
        category: "process",
        mutates_ui: false,
        requires_bound_window: false,
        produces_artifact: false,
    },
    ToolDescriptor {
        name: "process.kill",
        category: "process",
        mutates_ui: true,
        requires_bound_window: false,
        produces_artifact: false,
    },
    ToolDescriptor {
        name: "windows.wait_for_window",
        category: "window",
        mutates_ui: false,
        requires_bound_window: false,
        produces_artifact: false,
    },
    ToolDescriptor {
        name: "windows.bind",
        category: "window",
        mutates_ui: false,
        requires_bound_window: false,
        produces_artifact: false,
    },
    ToolDescriptor {
        name: "windows.focus",
        category: "window",
        mutates_ui: true,
        requires_bound_window: true,
        produces_artifact: false,
    },
    ToolDescriptor {
        name: "windows.wait_for_state",
        category: "window",
        mutates_ui: false,
        requires_bound_window: true,
        produces_artifact: false,
    },
    ToolDescriptor {
        name: "windows.move",
        category: "window",
        mutates_ui: true,
        requires_bound_window: true,
        produces_artifact: false,
    },
    ToolDescriptor {
        name: "windows.resize",
        category: "window",
        mutates_ui: true,
        requires_bound_window: true,
        produces_artifact: false,
    },
    ToolDescriptor {
        name: "windows.minimize",
        category: "window",
        mutates_ui: true,
        requires_bound_window: true,
        produces_artifact: false,
    },
    ToolDescriptor {
        name: "windows.maximize",
        category: "window",
        mutates_ui: true,
        requires_bound_window: true,
        produces_artifact: false,
    },
    ToolDescriptor {
        name: "windows.restore",
        category: "window",
        mutates_ui: true,
        requires_bound_window: true,
        produces_artifact: false,
    },
    ToolDescriptor {
        name: "windows.close",
        category: "window",
        mutates_ui: true,
        requires_bound_window: true,
        produces_artifact: false,
    },
    ToolDescriptor {
        name: "windows.foreground_diagnostics",
        category: "window",
        mutates_ui: false,
        requires_bound_window: true,
        produces_artifact: false,
    },
    ToolDescriptor {
        name: "windows.for_process",
        category: "window",
        mutates_ui: false,
        requires_bound_window: false,
        produces_artifact: false,
    },
    ToolDescriptor {
        name: "capture.screenshot_window",
        category: "capture",
        mutates_ui: false,
        requires_bound_window: true,
        produces_artifact: true,
    },
    ToolDescriptor {
        name: "capture.screenshot_display",
        category: "capture",
        mutates_ui: false,
        requires_bound_window: false,
        produces_artifact: true,
    },
    ToolDescriptor {
        name: "capture.wait_for_window_image_change",
        category: "capture",
        mutates_ui: false,
        requires_bound_window: true,
        produces_artifact: true,
    },
    ToolDescriptor {
        name: "input.click",
        category: "input",
        mutates_ui: true,
        requires_bound_window: true,
        produces_artifact: false,
    },
    ToolDescriptor {
        name: "input.double_click",
        category: "input",
        mutates_ui: true,
        requires_bound_window: true,
        produces_artifact: false,
    },
    ToolDescriptor {
        name: "input.drag",
        category: "input",
        mutates_ui: true,
        requires_bound_window: true,
        produces_artifact: false,
    },
    ToolDescriptor {
        name: "input.scroll",
        category: "input",
        mutates_ui: true,
        requires_bound_window: true,
        produces_artifact: false,
    },
    ToolDescriptor {
        name: "input.type_text",
        category: "input",
        mutates_ui: true,
        requires_bound_window: true,
        produces_artifact: false,
    },
    ToolDescriptor {
        name: "input.shortcut",
        category: "input",
        mutates_ui: true,
        requires_bound_window: true,
        produces_artifact: false,
    },
    ToolDescriptor {
        name: "input.key_down",
        category: "input",
        mutates_ui: true,
        requires_bound_window: true,
        produces_artifact: false,
    },
    ToolDescriptor {
        name: "input.key_up",
        category: "input",
        mutates_ui: true,
        requires_bound_window: true,
        produces_artifact: false,
    },
    ToolDescriptor {
        name: "input.delay",
        category: "timing",
        mutates_ui: false,
        requires_bound_window: false,
        produces_artifact: false,
    },
    ToolDescriptor {
        name: "uia.snapshot",
        category: "uia",
        mutates_ui: false,
        requires_bound_window: true,
        produces_artifact: true,
    },
    ToolDescriptor {
        name: "uia.find",
        category: "uia",
        mutates_ui: false,
        requires_bound_window: true,
        produces_artifact: false,
    },
    ToolDescriptor {
        name: "uia.resolve",
        category: "uia",
        mutates_ui: false,
        requires_bound_window: true,
        produces_artifact: false,
    },
    ToolDescriptor {
        name: "macro.assert_uia_element",
        category: "assertion",
        mutates_ui: false,
        requires_bound_window: true,
        produces_artifact: true,
    },
    ToolDescriptor {
        name: "macro.assert_image_checkpoint",
        category: "assertion",
        mutates_ui: false,
        requires_bound_window: true,
        produces_artifact: true,
    },
    ToolDescriptor {
        name: "macro.assert_text_checkpoint",
        category: "assertion",
        mutates_ui: false,
        requires_bound_window: true,
        produces_artifact: true,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_round_trips_as_json() {
        let manifest = betty_manifest();
        let encoded = serde_json::to_string(&manifest).unwrap();
        let decoded: MacroManifest = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, manifest);
        assert_eq!(decoded.version, MACRO_MANIFEST_VERSION);
    }

    #[test]
    fn validator_accepts_semantic_betty_manifest() {
        let manifest = betty_manifest();
        let report = validate_manifest(&manifest);
        assert!(
            report.valid,
            "expected valid manifest, got issues: {:?}",
            report.issues
        );
    }

    #[test]
    fn validator_rejects_unknown_tool_and_coordinate_without_metadata() {
        let mut manifest = betty_manifest();
        manifest.steps.push(MacroStep {
            id: "bad-coordinate".into(),
            tool: "input.click".into(),
            args: serde_json::json!({"bound_id": "${bound_id}", "x": 4, "y": 9}),
            target: Some(MacroTarget::Coordinates {
                x: 4,
                y: 9,
                coordinate_space: CoordinateSpace::VirtualDesktop,
            }),
            timeout_ms: None,
            required: true,
            continue_on_failure: false,
            coordinate_fallback: None,
            audit: StepAudit::default(),
        });
        manifest.steps.push(MacroStep {
            id: "unknown".into(),
            tool: "browser.click_by_title".into(),
            args: Value::Null,
            target: None,
            timeout_ms: None,
            required: true,
            continue_on_failure: false,
            coordinate_fallback: None,
            audit: StepAudit::default(),
        });

        let report = validate_manifest(&manifest);
        assert!(!report.valid);
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.code == "unknown_tool"));
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.code == "coordinate_fallback_metadata_required"));
    }

    #[test]
    fn dry_run_marks_mutating_and_artifact_steps() {
        let manifest = betty_manifest();
        let plan = dry_run_plan(&manifest).unwrap();
        let click = plan
            .steps
            .iter()
            .find(|step| step.id == "click-settings")
            .unwrap();
        assert!(click.mutates_ui);
        assert_eq!(click.target_strategy, "uia_element");

        let capture = plan
            .steps
            .iter()
            .find(|step| step.id == "capture-before-settings")
            .unwrap();
        assert!(capture.produces_artifact);
    }

    fn betty_manifest() -> MacroManifest {
        MacroManifest {
            version: MACRO_MANIFEST_VERSION.into(),
            kind: "test_procedure".into(),
            title: "Smoke test Betty standalone settings panel".into(),
            description:
                "Launch Betty standalone, open settings, and assert that the settings UI appears."
                    .into(),
            tags: vec!["betty".into(), "standalone".into(), "settings".into()],
            app_identity: Some(AppIdentity {
                executable_name: Some("Betty.exe".into()),
                executable_path: None,
                product_name: Some("Betty".into()),
                required: true,
                extra: Value::Null,
            }),
            launch: Some(ToolCall {
                tool: "process.launch".into(),
                args: serde_json::json!({"exe": "C:\\Betty\\Betty.exe", "wait_for_window": true}),
            }),
            bind: Some(BindingStrategy {
                strategy: "launched_process_main_window".into(),
                required_executable: Some("Betty.exe".into()),
                expected_identity_json: None,
                allow_child_process_windows: false,
            }),
            preconditions: vec![MacroStep {
                id: "main-window-visible".into(),
                tool: "windows.wait_for_window".into(),
                args: serde_json::json!({"launch_id": "${launch_id}", "timeout_ms": 10000}),
                target: Some(MacroTarget::LaunchedProcessWindow {
                    launch_id: Some("${launch_id}".into()),
                    pid: None,
                    hwnd: None,
                }),
                timeout_ms: Some(10_000),
                required: true,
                continue_on_failure: false,
                coordinate_fallback: None,
                audit: StepAudit::default(),
            }],
            steps: vec![
                MacroStep {
                    id: "capture-before-settings".into(),
                    tool: "capture.screenshot_window".into(),
                    args: serde_json::json!({"bound_id": "${bound_id}"}),
                    target: Some(MacroTarget::BoundWindow {
                        bound_id: Some("${bound_id}".into()),
                    }),
                    timeout_ms: None,
                    required: true,
                    continue_on_failure: false,
                    coordinate_fallback: None,
                    audit: StepAudit::default(),
                },
                MacroStep {
                    id: "click-settings".into(),
                    tool: "input.click".into(),
                    args: serde_json::json!({"bound_id": "${bound_id}", "x": 0.5, "y": 0.5, "coordinate_space": "normalized_window"}),
                    target: Some(MacroTarget::UiElement(UiElementTarget {
                        element_ref: None,
                        name: Some("Settings".into()),
                        role: Some("Button".into()),
                        control_type: None,
                        automation_id: None,
                        class_name: None,
                        hierarchy_path: None,
                        visible_text: Some("Settings".into()),
                        required: true,
                    })),
                    timeout_ms: None,
                    required: true,
                    continue_on_failure: false,
                    coordinate_fallback: Some(CoordinateFallbackMetadata {
                        monitor_id: Some("primary".into()),
                        virtual_desktop_x: 100,
                        virtual_desktop_y: 120,
                        window_rect: Rect {
                            x: 80,
                            y: 80,
                            width: 1200,
                            height: 800,
                        },
                        client_rect: None,
                        dpi: Some(96),
                        scale_factor: Some(1.0),
                        screenshot_size: Some(Size {
                            width: 1200,
                            height: 800,
                        }),
                        original_resolved_target: serde_json::json!({"name": "Settings"}),
                        preflight_validation: serde_json::json!({"ok": true}),
                    }),
                    audit: StepAudit::default(),
                },
            ],
            waits: vec![MacroStep {
                id: "settings-image-change".into(),
                tool: "capture.wait_for_window_image_change".into(),
                args: serde_json::json!({"bound_id": "${bound_id}", "timeout_ms": 5000}),
                target: Some(MacroTarget::BoundWindow {
                    bound_id: Some("${bound_id}".into()),
                }),
                timeout_ms: Some(5_000),
                required: true,
                continue_on_failure: false,
                coordinate_fallback: None,
                audit: StepAudit::default(),
            }],
            assertions: vec![MacroStep {
                id: "assert-settings-panel".into(),
                tool: "macro.assert_uia_element".into(),
                args: serde_json::json!({"name": "Settings", "role": "Pane", "timeout_ms": 5000}),
                target: Some(MacroTarget::UiElement(UiElementTarget {
                    element_ref: None,
                    name: Some("Settings".into()),
                    role: Some("Pane".into()),
                    control_type: None,
                    automation_id: None,
                    class_name: None,
                    hierarchy_path: None,
                    visible_text: Some("Settings".into()),
                    required: true,
                })),
                timeout_ms: Some(5_000),
                required: true,
                continue_on_failure: false,
                coordinate_fallback: None,
                audit: StepAudit::default(),
            }],
            cleanup: vec![],
            artifacts: ArtifactPolicy::default(),
            replay: ReplayMetadata {
                created_at: Some("2026-05-29T00:00:00Z".into()),
                ..Default::default()
            },
        }
    }
}
