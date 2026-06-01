use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use winctl::{UiElementSelector, WindowSelector};
use winctl_macro::{
    dry_run_plan, validate_manifest, AppIdentity, MacroArtifact, MacroExecutionResult,
    MacroExecutionStatus, MacroManifest, MacroPlan, MacroSection, MacroStep, MacroStepError,
    MacroStepErrorKind, MacroStepResult, MacroStepStatus,
};
use winctl_memory::{MemoryListRequest, RememberRequest};

use crate::{
    AppLaunchRequest, AppState, BoundIdRequest, BrowserAssertRequest, BrowserDescribeRequest,
    BrowserExtractContentRequest, BrowserListRequest, BrowserWaitForNavigationRequest,
    CaptureCompareBaselineRequest, CaptureOcrRegionRequest, CaptureReadTextRequest,
    DisplayScreenshotRequest, MacroAbortRequest, MacroDryRunRequest, MacroExportResultRequest,
    MacroGetRequest, MacroListRequest, MacroManifestRequest, MacroPromoteRequest, MacroRunRequest,
    MacroRunStepRequest, ProcessDescribeRequest, ProcessKillRequest, UiFindRequest,
    VideoStartRequest, VideoStopRequest, WaitForStateRequest, WaitForWindowRequest,
    WindowImageChangeWaitRequest, WindowMoveRequest, WindowResizeRequest, WindowsForProcessRequest,
};

#[derive(Default)]
pub struct MacroRuntimeState {
    next_macro_id: u64,
    next_run_id: u64,
    manifests: HashMap<String, StoredMacro>,
    results: HashMap<String, MacroExecutionResult>,
    active_runs: HashMap<String, Arc<AtomicBool>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredMacro {
    pub id: String,
    pub memory_id: Option<String>,
    pub title: String,
    pub kind: String,
    pub tags: Vec<String>,
    pub created_at: String,
    pub manifest: MacroManifest,
}

impl MacroRuntimeState {
    fn next_macro_id(&mut self) -> String {
        self.next_macro_id += 1;
        format!("macro-{}", self.next_macro_id)
    }

    fn next_run_id(&mut self) -> String {
        self.next_run_id += 1;
        format!("macro-run-{}", self.next_run_id)
    }
}

#[derive(Default)]
struct ExecutionContext {
    launch_id: Option<String>,
    pid: Option<u32>,
    bound_id: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct MacroImageCheckpointArgs {
    actual_path: Option<String>,
    baseline_path: Option<String>,
    diff_path: Option<String>,
    bound_id: Option<String>,
    tolerance: Option<u8>,
    max_different_pixels: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
struct MacroTextCheckpointArgs {
    text: Option<String>,
    expected: Option<String>,
    contains: Option<String>,
    actual_text: Option<String>,
    image_path: Option<String>,
    bound_id: Option<String>,
    x: Option<u32>,
    y: Option<u32>,
    width: Option<u32>,
    height: Option<u32>,
    max_depth: Option<usize>,
    max_elements: Option<usize>,
    use_ocr: Option<bool>,
    case_sensitive: Option<bool>,
}

impl ExecutionContext {
    fn from_json(value: Option<Value>) -> Self {
        let Some(value) = value else {
            return Self::default();
        };
        Self {
            launch_id: value
                .get("launch_id")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            pid: value
                .get("pid")
                .and_then(Value::as_u64)
                .and_then(|pid| u32::try_from(pid).ok()),
            bound_id: value
                .get("bound_id")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
        }
    }
}

pub fn macro_validate(request: MacroManifestRequest) -> serde_json::Value {
    tracing::info!(title = %request.manifest.title, "macro.validate requested");
    let report = validate_manifest(&request.manifest);
    serde_json::json!({
        "ok": true,
        "valid": report.valid,
        "report": report,
        "supported_tools": winctl_macro::supported_tool_names(),
    })
}

pub fn macro_dry_run(state: &AppState, request: MacroDryRunRequest) -> serde_json::Value {
    tracing::info!(memory_id = ?request.memory_id, "macro.dry_run requested");
    let (manifest, source_memory_id) =
        match resolve_macro_manifest(state, request.manifest, request.memory_id.as_deref()) {
            Ok(resolved) => resolved,
            Err(error) => return error,
        };
    match dry_run_plan(&manifest) {
        Ok(plan) => serde_json::json!({
            "ok": true,
            "valid": true,
            "source_memory_id": source_memory_id,
            "plan": plan
        }),
        Err(report) => serde_json::json!({
            "ok": false,
            "valid": false,
            "source_memory_id": source_memory_id,
            "report": report
        }),
    }
}

pub fn macro_run(state: &AppState, request: MacroRunRequest) -> serde_json::Value {
    tracing::info!(
        memory_id = ?request.memory_id,
        max_steps = ?request.max_steps,
        "macro.run requested"
    );
    if !state.policy.macro_execution_enabled {
        return policy_denied(
            "macro_execution_disabled",
            "macro.run is disabled by runtime policy",
        );
    }
    let (manifest, source_memory_id) =
        match resolve_macro_manifest(state, request.manifest, request.memory_id.as_deref()) {
            Ok(resolved) => resolved,
            Err(error) => return error,
        };
    match dry_run_plan(&manifest) {
        Ok(plan) => {
            let (run_id, abort_flag) = start_run(state);
            let max_steps = request.max_steps.or(state.policy.max_macro_steps);
            let mut video_start_warning = None;
            let video_recording_id = request.video.clone().and_then(|video| {
                let start = crate::tools::capture::video_start(state, video);
                if start.get("ok").and_then(Value::as_bool).unwrap_or(false) {
                    start
                        .get("recording")
                        .and_then(|recording| recording.get("recording_id"))
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned)
                } else {
                    tracing::warn!(response = %start, "macro video recording failed to start");
                    video_start_warning = Some(format!("video recording failed to start: {start}"));
                    None
                }
            });
            let mut result = execute_manifest(
                state,
                &manifest,
                &plan,
                &run_id,
                max_steps,
                None,
                ExecutionContext::default(),
                abort_flag.clone(),
            );
            if let Some(warning) = video_start_warning {
                result.diagnostics.push(warning);
            }
            if let Some(recording_id) = video_recording_id {
                let stop = crate::tools::capture::video_stop(
                    state,
                    VideoStopRequest {
                        recording_id: Some(recording_id),
                    },
                );
                attach_video_artifact(&mut result, stop);
            }
            record_successful_memory_use(state, source_memory_id.as_deref(), &result);
            finish_run(state, &run_id, result.clone());
            serde_json::json!({
                "ok": matches!(result.status, MacroExecutionStatus::Succeeded),
                "run_id": run_id,
                "source_memory_id": source_memory_id,
                "result": result,
            })
        }
        Err(report) => serde_json::json!({
            "ok": false,
            "error": {
                "code": "macro_validation_failed",
                "message": "macro manifest failed validation"
            },
            "report": report
        }),
    }
}

pub fn macro_run_step(state: &AppState, request: MacroRunStepRequest) -> serde_json::Value {
    tracing::info!(
        memory_id = ?request.memory_id,
        step_id = %request.step_id,
        "macro.run_step requested"
    );
    if !state.policy.macro_execution_enabled {
        return policy_denied(
            "macro_execution_disabled",
            "macro.run_step is disabled by runtime policy",
        );
    }
    let (manifest, source_memory_id) =
        match resolve_macro_manifest(state, request.manifest, request.memory_id.as_deref()) {
            Ok(resolved) => resolved,
            Err(error) => return error,
        };
    match dry_run_plan(&manifest) {
        Ok(plan) => {
            if !manifest_contains_step(&manifest, &request.step_id) {
                return serde_json::json!({
                    "ok": false,
                    "error": {
                        "code": "macro_step_not_found",
                        "message": format!("step {} was not found", request.step_id)
                    }
                });
            }
            let (run_id, abort_flag) = start_run(state);
            let result = execute_manifest(
                state,
                &manifest,
                &plan,
                &run_id,
                Some(1),
                Some(request.step_id),
                ExecutionContext::from_json(request.context_json),
                abort_flag,
            );
            record_successful_memory_use(state, source_memory_id.as_deref(), &result);
            finish_run(state, &run_id, result.clone());
            serde_json::json!({
                "ok": matches!(result.status, MacroExecutionStatus::Succeeded),
                "run_id": run_id,
                "source_memory_id": source_memory_id,
                "result": result,
            })
        }
        Err(report) => serde_json::json!({
            "ok": false,
            "error": {
                "code": "macro_validation_failed",
                "message": "macro manifest failed validation"
            },
            "report": report
        }),
    }
}

pub fn macro_abort(state: &AppState, request: MacroAbortRequest) -> serde_json::Value {
    tracing::info!(run_id = %request.run_id, "macro.abort requested");
    let runtime = state
        .macro_runtime
        .lock()
        .expect("macro runtime mutex poisoned");
    if let Some(flag) = runtime.active_runs.get(&request.run_id) {
        flag.store(true, Ordering::SeqCst);
        tracing::warn!(run_id = %request.run_id, "macro abort requested for active run");
        serde_json::json!({"ok": true, "run_id": request.run_id, "abort_requested": true})
    } else {
        serde_json::json!({
            "ok": false,
            "error": {
                "code": "macro_run_not_active",
                "message": format!("macro run {} is not active", request.run_id)
            }
        })
    }
}

pub fn macro_list(state: &AppState, request: MacroListRequest) -> serde_json::Value {
    tracing::info!(
        kind = ?request.kind,
        tags = ?request.tags,
        limit = ?request.limit,
        "macro.list requested"
    );
    let limit = request.limit.unwrap_or(100).min(1000);
    let session_macros: Vec<_> = state
        .macro_runtime
        .lock()
        .expect("macro runtime mutex poisoned")
        .manifests
        .values()
        .filter(|stored| macro_filters_match(stored, request.kind.as_deref(), &request.tags))
        .take(limit)
        .cloned()
        .collect();
    let memory_items = state
        .memory
        .lock()
        .expect("memory store mutex poisoned")
        .list(MemoryListRequest {
            kind: Some("macro".into()),
            tags: request.tags,
            limit: Some(limit),
        })
        .unwrap_or_default();
    serde_json::json!({
        "ok": true,
        "session_macros": session_macros,
        "memory_items": memory_items,
    })
}

pub fn macro_get(state: &AppState, request: MacroGetRequest) -> serde_json::Value {
    tracing::info!(id = %request.id, "macro.get requested");
    if let Some(stored) = state
        .macro_runtime
        .lock()
        .expect("macro runtime mutex poisoned")
        .manifests
        .get(&request.id)
        .cloned()
    {
        return serde_json::json!({"ok": true, "macro": stored});
    }

    let mut memory = state.memory.lock().expect("memory store mutex poisoned");
    match memory.get(&request.id) {
        Ok(Some(item)) => match item
            .manifest_json
            .as_ref()
            .and_then(|value| serde_json::from_value::<MacroManifest>(value.clone()).ok())
        {
            Some(manifest) => serde_json::json!({
                "ok": true,
                "memory_item": item,
                "manifest": manifest,
            }),
            None => serde_json::json!({
                "ok": false,
                "error": {
                    "code": "macro_manifest_not_found",
                    "message": "memory item does not contain a valid macro manifest"
                }
            }),
        },
        Ok(None) => serde_json::json!({
            "ok": false,
            "error": {
                "code": "macro_not_found",
                "message": format!("macro {} was not found", request.id)
            }
        }),
        Err(error) => serde_json::json!({
            "ok": false,
            "error": {
                "code": "macro_get_failed",
                "message": error.to_string()
            }
        }),
    }
}

pub fn macro_promote(state: &AppState, request: MacroPromoteRequest) -> serde_json::Value {
    tracing::info!(
        title = %request.manifest.title,
        remember = request.remember,
        "macro.promote requested"
    );
    let report = validate_manifest(&request.manifest);
    if !report.valid {
        return serde_json::json!({
            "ok": false,
            "error": {
                "code": "macro_validation_failed",
                "message": "macro manifest failed validation"
            },
            "report": report
        });
    }

    let memory_id = if request.remember {
        if !state.policy.memory_mutation_enabled {
            return policy_denied(
                "memory_mutation_disabled",
                "macro.promote remember=true is disabled by runtime policy",
            );
        }
        let mut memory = state.memory.lock().expect("memory store mutex poisoned");
        match memory.remember(RememberRequest {
            kind: "macro".into(),
            title: request.manifest.title.clone(),
            text: macro_search_text(&request.manifest),
            manifest_json: Some(serde_json::to_value(&request.manifest).unwrap_or(Value::Null)),
            tags: request.manifest.tags.clone(),
            app_identity_json: request
                .manifest
                .app_identity
                .as_ref()
                .and_then(|identity| serde_json::to_value(identity).ok()),
            target_identity_json: request
                .manifest
                .bind
                .as_ref()
                .and_then(|bind| serde_json::to_value(bind).ok()),
        }) {
            Ok(item) => Some(item.id),
            Err(error) => {
                return serde_json::json!({
                    "ok": false,
                    "error": {
                        "code": "macro_memory_promote_failed",
                        "message": error.to_string()
                    }
                });
            }
        }
    } else {
        None
    };

    let mut runtime = state
        .macro_runtime
        .lock()
        .expect("macro runtime mutex poisoned");
    let id = runtime.next_macro_id();
    let stored = StoredMacro {
        id: id.clone(),
        memory_id,
        title: request.manifest.title.clone(),
        kind: request.manifest.kind.clone(),
        tags: request.manifest.tags.clone(),
        created_at: chrono::Utc::now().to_rfc3339(),
        manifest: request.manifest,
    };
    runtime.manifests.insert(id.clone(), stored.clone());
    serde_json::json!({"ok": true, "macro": stored})
}

pub fn macro_export_result(
    state: &AppState,
    request: MacroExportResultRequest,
) -> serde_json::Value {
    tracing::info!(run_id = %request.run_id, "macro.export_result requested");
    match state
        .macro_runtime
        .lock()
        .expect("macro runtime mutex poisoned")
        .results
        .get(&request.run_id)
        .cloned()
    {
        Some(result) => serde_json::json!({"ok": true, "run_id": request.run_id, "result": result}),
        None => serde_json::json!({
            "ok": false,
            "error": {
                "code": "macro_result_not_found",
                "message": format!("macro result {} was not found", request.run_id)
            }
        }),
    }
}

fn resolve_macro_manifest(
    state: &AppState,
    manifest: Option<MacroManifest>,
    memory_id: Option<&str>,
) -> Result<(MacroManifest, Option<String>), serde_json::Value> {
    if let Some(manifest) = manifest {
        return Ok((manifest, memory_id.map(ToOwned::to_owned)));
    }
    let Some(memory_id) = memory_id else {
        return Err(serde_json::json!({
            "ok": false,
            "error": {
                "code": "missing_macro_manifest",
                "message": "provide either manifest or memory_id"
            }
        }));
    };
    let memory = state.memory.lock().expect("memory store mutex poisoned");
    let item = match memory.get_without_touch(memory_id) {
        Ok(Some(item)) => item,
        Ok(None) => {
            return Err(serde_json::json!({
                "ok": false,
                "error": {
                    "code": "macro_memory_item_not_found",
                    "message": format!("memory item {memory_id} was not found")
                }
            }));
        }
        Err(error) => {
            return Err(serde_json::json!({
                "ok": false,
                "error": {
                    "code": "macro_memory_load_failed",
                    "message": error.to_string()
                }
            }));
        }
    };
    let Some(manifest_json) = item.manifest_json else {
        return Err(serde_json::json!({
            "ok": false,
            "error": {
                "code": "macro_manifest_not_found",
                "message": "memory item does not contain manifest_json"
            }
        }));
    };
    match serde_json::from_value::<MacroManifest>(manifest_json) {
        Ok(manifest) => Ok((manifest, Some(memory_id.to_owned()))),
        Err(error) => Err(serde_json::json!({
            "ok": false,
            "error": {
                "code": "macro_manifest_invalid",
                "message": error.to_string()
            }
        })),
    }
}

fn record_successful_memory_use(
    state: &AppState,
    source_memory_id: Option<&str>,
    result: &MacroExecutionResult,
) {
    if !matches!(result.status, MacroExecutionStatus::Succeeded) {
        return;
    }
    let Some(memory_id) = source_memory_id else {
        return;
    };
    let mut memory = state.memory.lock().expect("memory store mutex poisoned");
    match memory.record_use(memory_id) {
        Ok(Some(item)) => tracing::info!(
            memory_id = %item.id,
            use_count = item.use_count,
            "memory-backed macro run recorded successful use"
        ),
        Ok(None) => tracing::warn!(
            memory_id = %memory_id,
            "memory-backed macro run succeeded but source memory item was not found"
        ),
        Err(error) => tracing::warn!(
            memory_id = %memory_id,
            error = %error,
            "failed to record memory-backed macro use"
        ),
    }
}

fn execute_manifest(
    state: &AppState,
    manifest: &MacroManifest,
    _plan: &MacroPlan,
    _run_id: &str,
    max_steps: Option<usize>,
    only_step_id: Option<String>,
    mut context: ExecutionContext,
    abort_flag: Arc<AtomicBool>,
) -> MacroExecutionResult {
    let started_at = chrono::Utc::now().to_rfc3339();
    let mut result = MacroExecutionResult {
        manifest_title: manifest.title.clone(),
        started_at,
        finished_at: None,
        status: MacroExecutionStatus::Running,
        step_results: Vec::new(),
        artifacts: Vec::new(),
        diagnostics: Vec::new(),
    };

    let mut executed = 0usize;
    let mut failed = false;

    if only_step_id.is_none() {
        if let Some(launch) = &manifest.launch {
            let launch_step = MacroStep {
                id: "launch".into(),
                tool: launch.tool.clone(),
                args: launch.args.clone(),
                target: None,
                timeout_ms: None,
                required: true,
                continue_on_failure: false,
                coordinate_fallback: None,
                audit: Default::default(),
            };
            let step_result = execute_step(state, &launch_step, MacroSection::Launch, &mut context);
            failed = step_failed(&step_result);
            result.step_results.push(step_result);
        }
        if !failed {
            maybe_bind_launched_window(state, manifest, &mut context, &mut result);
        }
    }

    let scheduled_steps = if only_step_id.is_some() {
        manifest_steps(manifest)
    } else {
        primary_manifest_steps(manifest)
    };
    for (section, step) in scheduled_steps {
        if failed {
            break;
        }
        if only_step_id
            .as_ref()
            .map(|step_id| step_id != &step.id)
            .unwrap_or(false)
        {
            continue;
        }
        if abort_flag.load(Ordering::SeqCst) {
            result.status = MacroExecutionStatus::Aborted;
            result.diagnostics.push("macro run was aborted".into());
            break;
        }
        if max_steps.map(|limit| executed >= limit).unwrap_or(false) {
            break;
        }
        let step_result = execute_step(state, step, section, &mut context);
        failed = step_failed(&step_result) && step.required && !step.continue_on_failure;
        result.step_results.push(step_result);
        executed += 1;
    }

    if only_step_id.is_none()
        && !matches!(result.status, MacroExecutionStatus::Aborted)
        && !manifest.cleanup.is_empty()
    {
        for step in &manifest.cleanup {
            if abort_flag.load(Ordering::SeqCst) {
                result.status = MacroExecutionStatus::Aborted;
                result
                    .diagnostics
                    .push("macro run was aborted during cleanup".into());
                break;
            }
            let cleanup_result = execute_step(state, step, MacroSection::Cleanup, &mut context);
            if step_failed(&cleanup_result) {
                failed = true;
            }
            result.step_results.push(cleanup_result);
        }
    }

    if matches!(result.status, MacroExecutionStatus::Running) {
        result.status = if failed {
            MacroExecutionStatus::Failed
        } else {
            MacroExecutionStatus::Succeeded
        };
    }
    result.finished_at = Some(chrono::Utc::now().to_rfc3339());
    result
}

fn execute_step(
    state: &AppState,
    step: &MacroStep,
    section: MacroSection,
    context: &mut ExecutionContext,
) -> MacroStepResult {
    let started_at = chrono::Utc::now().to_rfc3339();
    let resolved_args = substitute_value(&step.args, context);
    tracing::info!(
        step_id = %step.id,
        tool = %step.tool,
        section = ?section,
        "macro step executing"
    );

    let output = match dispatch_tool(state, &step.tool, resolved_args, context, step) {
        Ok(value) => value,
        Err(error) => {
            return MacroStepResult {
                step_id: step.id.clone(),
                tool: step.tool.clone(),
                status: MacroStepStatus::Failed,
                started_at,
                finished_at: Some(chrono::Utc::now().to_rfc3339()),
                output: Value::Null,
                diagnostics: vec![error.message.clone()],
                artifacts: Vec::new(),
                error: Some(error),
            };
        }
    };

    update_context_from_output(context, &step.tool, &output);
    let ok = output.get("ok").and_then(Value::as_bool).unwrap_or(true);
    let error = (!ok).then(|| step_error_from_output(&step.tool, &output));
    MacroStepResult {
        step_id: step.id.clone(),
        tool: step.tool.clone(),
        status: if ok {
            MacroStepStatus::Succeeded
        } else {
            MacroStepStatus::Failed
        },
        started_at,
        finished_at: Some(chrono::Utc::now().to_rfc3339()),
        output,
        diagnostics: Vec::new(),
        artifacts: artifacts_from_output(&step.tool),
        error,
    }
}

fn dispatch_tool(
    state: &AppState,
    tool: &str,
    args: Value,
    context: &mut ExecutionContext,
    step: &MacroStep,
) -> Result<Value, MacroStepError> {
    match tool {
        "app.launch" => Ok(crate::tools::process::app_launch(
            state,
            parse_args::<AppLaunchRequest>(args)?,
        )),
        "process.launch" => Ok(crate::tools::process::process_launch(
            state,
            parse_args(args)?,
        )),
        "process.wait_for_exit" => Ok(crate::tools::process::process_wait_for_exit(
            state,
            parse_args(args)?,
        )),
        "process.describe" => Ok(crate::tools::process::process_describe(
            state,
            parse_args::<ProcessDescribeRequest>(args)?,
        )),
        "process.kill" => Ok(crate::tools::process::process_kill(
            state,
            parse_args::<ProcessKillRequest>(args)?,
        )),
        "windows.bind" => Ok(crate::tools::windows::windows_bind(
            state,
            parse_args::<WindowSelector>(args)?,
        )),
        "windows.focus" => Ok(crate::tools::windows::windows_focus(
            state,
            bound_id_from_args(args, context)?,
        )),
        "windows.wait_for_window" => Ok(crate::tools::process::windows_wait_for_window(
            state,
            parse_args::<WaitForWindowRequest>(args)?,
        )),
        "windows.wait_for_state" => Ok(crate::tools::windows::windows_wait_for_state(
            state,
            parse_args::<WaitForStateRequest>(args)?,
        )),
        "windows.move" => Ok(crate::tools::windows::windows_move(
            state,
            parse_args::<WindowMoveRequest>(args)?,
        )),
        "windows.resize" => Ok(crate::tools::windows::windows_resize(
            state,
            parse_args::<WindowResizeRequest>(args)?,
        )),
        "windows.minimize" => Ok(crate::tools::windows::windows_minimize(
            state,
            bound_id_from_args(args, context)?,
        )),
        "windows.maximize" => Ok(crate::tools::windows::windows_maximize(
            state,
            bound_id_from_args(args, context)?,
        )),
        "windows.restore" => Ok(crate::tools::windows::windows_restore(
            state,
            bound_id_from_args(args, context)?,
        )),
        "windows.close" => Ok(crate::tools::windows::windows_close(
            state,
            bound_id_from_args(args, context)?,
        )),
        "windows.foreground_diagnostics" => {
            Ok(crate::tools::windows::windows_foreground_diagnostics(
                state,
                bound_id_from_args(args, context)?,
            ))
        }
        "windows.for_process" => Ok(crate::tools::windows::windows_for_process(
            state,
            parse_args::<WindowsForProcessRequest>(args)?,
        )),
        "browser.list" => Ok(crate::tools::browser::browser_list(
            state,
            parse_args::<BrowserListRequest>(args)?,
        )),
        "browser.describe" => Ok(crate::tools::browser::browser_describe(
            state,
            parse_args::<BrowserDescribeRequest>(args)?,
        )),
        "browser.wait_for_navigation" => Ok(crate::tools::browser::browser_wait_for_navigation(
            state,
            parse_args::<BrowserWaitForNavigationRequest>(args)?,
        )),
        "browser.assert" => Ok(crate::tools::browser::browser_assert(
            state,
            parse_args::<BrowserAssertRequest>(args)?,
        )),
        "browser.extract_content" => Ok(crate::tools::browser::browser_extract_content(
            state,
            parse_args::<BrowserExtractContentRequest>(args)?,
        )),
        "browser.screenshot_checkpoint" => {
            Ok(crate::tools::browser::browser_screenshot_checkpoint(
                state,
                parse_args::<BrowserExtractContentRequest>(args)?,
            ))
        }
        "capture.screenshot_window" => Ok(crate::tools::capture::screenshot_window(
            state,
            bound_id_from_args(args, context)?,
        )),
        "capture.screenshot_display" => Ok(crate::tools::capture::screenshot_display(
            state,
            parse_args::<DisplayScreenshotRequest>(args)?.display_index,
        )),
        "capture.wait_for_window_image_change" => {
            Ok(crate::tools::capture::wait_for_window_image_change(
                state,
                parse_args::<WindowImageChangeWaitRequest>(args)?,
            ))
        }
        "capture.video_start" => Ok(crate::tools::capture::video_start(
            state,
            parse_args::<VideoStartRequest>(args)?,
        )),
        "capture.video_stop" => Ok(crate::tools::capture::video_stop(
            state,
            parse_args::<VideoStopRequest>(args)?,
        )),
        "input.click" => Ok(crate::tools::input::input_click(state, parse_args(args)?)),
        "input.double_click" => Ok(crate::tools::input::input_double_click(
            state,
            parse_args(args)?,
        )),
        "input.drag" => Ok(crate::tools::input::input_drag(state, parse_args(args)?)),
        "input.scroll" => Ok(crate::tools::input::input_scroll(state, parse_args(args)?)),
        "input.type_text" => Ok(crate::tools::input::input_type_text(
            state,
            parse_args(args)?,
        )),
        "input.shortcut" => Ok(crate::tools::input::input_shortcut(
            state,
            parse_args(args)?,
        )),
        "input.key_down" => Ok(crate::tools::input::input_key_down(
            state,
            parse_args(args)?,
        )),
        "input.key_up" => Ok(crate::tools::input::input_key_up(state, parse_args(args)?)),
        "input.delay" => Ok(crate::tools::input::input_delay(parse_args(args)?)),
        "uia.snapshot" => Ok(crate::tools::uia::uia_snapshot(state, parse_args(args)?)),
        "uia.find" => Ok(crate::tools::uia::uia_find(state, parse_args(args)?)),
        "uia.resolve" => Ok(crate::tools::uia::uia_resolve(state, parse_args(args)?)),
        "macro.assert_uia_element" => assert_uia_element(state, args, context, step),
        "macro.assert_image_checkpoint" => assert_image_checkpoint(state, args, context, step),
        "macro.assert_text_checkpoint" => assert_text_checkpoint(state, args, context, step),
        other => Err(MacroStepError {
            kind: MacroStepErrorKind::ValidationFailure,
            code: "unknown_tool".into(),
            message: format!("unsupported macro tool {other}"),
            diagnostics: Value::Null,
        }),
    }
}

fn assert_image_checkpoint(
    state: &AppState,
    args: Value,
    context: &ExecutionContext,
    step: &MacroStep,
) -> Result<Value, MacroStepError> {
    let request = serde_json::from_value::<MacroImageCheckpointArgs>(args).map_err(|error| {
        MacroStepError {
            kind: MacroStepErrorKind::ValidationFailure,
            code: "macro_step_args_invalid".into(),
            message: error.to_string(),
            diagnostics: Value::Null,
        }
    })?;
    let baseline_path = request
        .baseline_path
        .or_else(|| match &step.target {
            Some(winctl_macro::MacroTarget::ImageCheckpoint { path, .. }) => path.clone(),
            _ => None,
        })
        .ok_or_else(|| MacroStepError {
            kind: MacroStepErrorKind::ValidationFailure,
            code: "image_baseline_required".into(),
            message: "macro.assert_image_checkpoint requires baseline_path or an image checkpoint target path".into(),
            diagnostics: Value::Null,
        })?;
    let actual_path = match request.actual_path {
        Some(path) => path,
        None => {
            let bound_id = request.bound_id.or_else(|| context.bound_id.clone()).ok_or_else(|| {
                MacroStepError {
                    kind: MacroStepErrorKind::TargetIdentityFailure,
                    code: "missing_bound_id".into(),
                    message: "macro.assert_image_checkpoint requires actual_path or a bound_id/context bound_id to capture".into(),
                    diagnostics: Value::Null,
                }
            })?;
            let capture = crate::tools::capture::screenshot_window(state, bound_id);
            if !capture.get("ok").and_then(Value::as_bool).unwrap_or(false) {
                return Ok(capture);
            }
            capture
                .get("screenshot")
                .and_then(|screenshot| screenshot.get("output_path"))
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
                .ok_or_else(|| MacroStepError {
                    kind: MacroStepErrorKind::ToolExecutionFailure,
                    code: "screenshot_path_missing".into(),
                    message: "screenshot did not include output_path".into(),
                    diagnostics: capture,
                })?
        }
    };
    let comparison = crate::tools::assertions::capture_compare_baseline(
        state,
        CaptureCompareBaselineRequest {
            actual_path: actual_path.clone(),
            baseline_path: baseline_path.clone(),
            tolerance: request.tolerance,
            max_different_pixels: request.max_different_pixels,
            diff_path: request.diff_path,
        },
    );
    let provider_ok = comparison
        .get("ok")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let passed = provider_ok
        && comparison
            .get("passed")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    Ok(serde_json::json!({
        "ok": passed,
        "passed": passed,
        "assertion": "image_checkpoint",
        "actual_path": actual_path,
        "baseline_path": baseline_path,
        "comparison": comparison,
        "error": (!passed).then(|| serde_json::json!({
            "code": "macro_assertion_failed",
            "message": "image checkpoint did not match baseline"
        })),
    }))
}

fn assert_text_checkpoint(
    state: &AppState,
    args: Value,
    context: &ExecutionContext,
    step: &MacroStep,
) -> Result<Value, MacroStepError> {
    let request = serde_json::from_value::<MacroTextCheckpointArgs>(args).map_err(|error| {
        MacroStepError {
            kind: MacroStepErrorKind::ValidationFailure,
            code: "macro_step_args_invalid".into(),
            message: error.to_string(),
            diagnostics: Value::Null,
        }
    })?;
    let expected = request
        .contains
        .or(request.expected)
        .or(request.text)
        .or_else(|| match &step.target {
            Some(winctl_macro::MacroTarget::TextCheckpoint { text }) => Some(text.clone()),
            _ => None,
        })
        .ok_or_else(|| MacroStepError {
            kind: MacroStepErrorKind::ValidationFailure,
            code: "text_checkpoint_required".into(),
            message: "macro.assert_text_checkpoint requires text, expected, contains, or a text checkpoint target".into(),
            diagnostics: Value::Null,
        })?;
    let bound_id = request.bound_id.or_else(|| context.bound_id.clone());
    let source = if let Some(actual_text) = request.actual_text {
        serde_json::json!({
            "ok": true,
            "provider": "literal",
            "text": actual_text,
        })
    } else if request.image_path.is_some() || request.use_ocr.unwrap_or(false) {
        crate::tools::assertions::capture_ocr_region(
            state,
            CaptureOcrRegionRequest {
                image_path: request.image_path,
                bound_id,
                x: request.x,
                y: request.y,
                width: request.width,
                height: request.height,
            },
        )
    } else {
        let bound_id = bound_id.ok_or_else(|| MacroStepError {
            kind: MacroStepErrorKind::TargetIdentityFailure,
            code: "missing_bound_id".into(),
            message: "macro.assert_text_checkpoint requires actual_text, image_path/use_ocr, or a bound_id/context bound_id".into(),
            diagnostics: Value::Null,
        })?;
        crate::tools::assertions::capture_read_text(
            state,
            CaptureReadTextRequest {
                bound_id,
                max_depth: request.max_depth,
                max_elements: request.max_elements,
            },
        )
    };
    if !source.get("ok").and_then(Value::as_bool).unwrap_or(false) {
        return Ok(source);
    }
    let actual_text = source
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let case_sensitive = request.case_sensitive.unwrap_or(false);
    let passed = if case_sensitive {
        actual_text.contains(&expected)
    } else {
        actual_text
            .to_ascii_lowercase()
            .contains(&expected.to_ascii_lowercase())
    };
    Ok(serde_json::json!({
        "ok": passed,
        "passed": passed,
        "assertion": "text_checkpoint",
        "expected": expected,
        "case_sensitive": case_sensitive,
        "actual_text": actual_text,
        "source": source,
        "error": (!passed).then(|| serde_json::json!({
            "code": "macro_assertion_failed",
            "message": "text checkpoint did not contain expected text"
        })),
    }))
}

fn assert_uia_element(
    state: &AppState,
    args: Value,
    context: &ExecutionContext,
    step: &MacroStep,
) -> Result<Value, MacroStepError> {
    let bound_id = args
        .get("bound_id")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| context.bound_id.clone())
        .ok_or_else(|| MacroStepError {
            kind: MacroStepErrorKind::TargetIdentityFailure,
            code: "missing_bound_id".into(),
            message: "macro.assert_uia_element requires a bound_id or macro context bound_id"
                .into(),
            diagnostics: Value::Null,
        })?;
    let selector = UiElementSelector {
        element_ref: args
            .get("element_ref")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .or_else(|| {
                step.target.as_ref().and_then(|target| match target {
                    winctl_macro::MacroTarget::UiElement(target) => target.element_ref.clone(),
                    _ => None,
                })
            }),
        name: args
            .get("name")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        role: args
            .get("role")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        automation_id: args
            .get("automation_id")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        class_name: args
            .get("class_name")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        text_contains: args
            .get("text_contains")
            .or_else(|| args.get("visible_text"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        include_offscreen: args.get("include_offscreen").and_then(Value::as_bool),
    };
    let value = crate::tools::uia::uia_find(
        state,
        UiFindRequest {
            bound_id,
            selector,
            max_depth: args
                .get("max_depth")
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok()),
            max_elements: args
                .get("max_elements")
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok()),
        },
    );
    let match_count = value
        .get("match_count")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    if value.get("ok").and_then(Value::as_bool).unwrap_or(false) && match_count > 0 {
        Ok(value)
    } else {
        Ok(serde_json::json!({
            "ok": false,
            "error": {
                "code": "macro_assertion_failed",
                "message": "UI Automation assertion did not match any element"
            },
            "uia_find": value,
        }))
    }
}

fn parse_args<T: DeserializeOwned>(args: Value) -> Result<T, MacroStepError> {
    serde_json::from_value(args).map_err(|error| MacroStepError {
        kind: MacroStepErrorKind::ValidationFailure,
        code: "macro_step_args_invalid".into(),
        message: error.to_string(),
        diagnostics: Value::Null,
    })
}

fn bound_id_from_args(args: Value, context: &ExecutionContext) -> Result<String, MacroStepError> {
    if let Ok(request) = serde_json::from_value::<BoundIdRequest>(args.clone()) {
        return Ok(request.bound_id);
    }
    args.get("bound_id")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| context.bound_id.clone())
        .ok_or_else(|| MacroStepError {
            kind: MacroStepErrorKind::TargetIdentityFailure,
            code: "missing_bound_id".into(),
            message: "step requires a bound_id or macro context bound_id".into(),
            diagnostics: Value::Null,
        })
}

fn update_context_from_output(context: &mut ExecutionContext, tool: &str, output: &Value) {
    if tool == "process.launch" {
        context.launch_id = output
            .get("launch_id")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .or_else(|| context.launch_id.clone());
        context.pid = output
            .get("pid")
            .and_then(Value::as_u64)
            .and_then(|pid| u32::try_from(pid).ok())
            .or(context.pid);
    }
    if tool == "windows.bind" {
        context.bound_id = output
            .get("bound")
            .and_then(|bound| bound.get("bound_id"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .or_else(|| context.bound_id.clone());
    }
}

fn maybe_bind_launched_window(
    state: &AppState,
    manifest: &MacroManifest,
    context: &mut ExecutionContext,
    result: &mut MacroExecutionResult,
) {
    if context.bound_id.is_some() || manifest.bind.is_none() {
        return;
    }
    let Some(pid) = context.pid else {
        return;
    };
    let selector = WindowSelector {
        pid: Some(pid),
        exe_path_ends_with: manifest
            .bind
            .as_ref()
            .and_then(|bind| bind.required_executable.clone()),
        must_be_visible: Some(true),
        ..Default::default()
    };
    let step = MacroStep {
        id: "bind".into(),
        tool: "windows.bind".into(),
        args: serde_json::to_value(selector).unwrap_or(Value::Null),
        target: None,
        timeout_ms: None,
        required: true,
        continue_on_failure: false,
        coordinate_fallback: None,
        audit: Default::default(),
    };
    let step_result = execute_step(state, &step, MacroSection::Bind, context);
    result.step_results.push(step_result);
}

fn substitute_value(value: &Value, context: &ExecutionContext) -> Value {
    match value {
        Value::String(text) => substitute_string(text, context),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| substitute_value(item, context))
                .collect(),
        ),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| (key.clone(), substitute_value(value, context)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn substitute_string(text: &str, context: &ExecutionContext) -> Value {
    match text {
        "${launch_id}" => context
            .launch_id
            .clone()
            .map(Value::String)
            .unwrap_or_else(|| Value::String(text.into())),
        "${pid}" => context
            .pid
            .map(|pid| Value::Number(serde_json::Number::from(pid)))
            .unwrap_or_else(|| Value::String(text.into())),
        "${bound_id}" => context
            .bound_id
            .clone()
            .map(Value::String)
            .unwrap_or_else(|| Value::String(text.into())),
        _ => {
            let mut replaced = text.to_string();
            if let Some(launch_id) = &context.launch_id {
                replaced = replaced.replace("${launch_id}", launch_id);
            }
            if let Some(pid) = context.pid {
                replaced = replaced.replace("${pid}", &pid.to_string());
            }
            if let Some(bound_id) = &context.bound_id {
                replaced = replaced.replace("${bound_id}", bound_id);
            }
            Value::String(replaced)
        }
    }
}

pub fn macro_results_snapshot(state: &AppState, limit: usize) -> serde_json::Value {
    let runtime = state.macro_runtime.lock().expect("macro mutex poisoned");
    let mut results = runtime
        .results
        .iter()
        .map(|(run_id, result)| {
            serde_json::json!({
                "run_id": run_id,
                "result": result,
            })
        })
        .collect::<Vec<_>>();
    results.sort_by(|left, right| {
        let left_finished = left
            .get("result")
            .and_then(|result| result.get("finished_at"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        let right_finished = right
            .get("result")
            .and_then(|result| result.get("finished_at"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        right_finished.cmp(left_finished)
    });
    results.truncate(limit);
    serde_json::json!({
        "ok": true,
        "results": results,
    })
}

fn start_run(state: &AppState) -> (String, Arc<AtomicBool>) {
    let mut runtime = state
        .macro_runtime
        .lock()
        .expect("macro runtime mutex poisoned");
    let run_id = runtime.next_run_id();
    let abort_flag = Arc::new(AtomicBool::new(false));
    runtime
        .active_runs
        .insert(run_id.clone(), abort_flag.clone());
    (run_id, abort_flag)
}

fn finish_run(state: &AppState, run_id: &str, result: MacroExecutionResult) {
    let mut runtime = state
        .macro_runtime
        .lock()
        .expect("macro runtime mutex poisoned");
    runtime.active_runs.remove(run_id);
    runtime.results.insert(run_id.into(), result);
}

fn manifest_contains_step(manifest: &MacroManifest, step_id: &str) -> bool {
    manifest_steps(manifest)
        .into_iter()
        .any(|(_, step)| step.id == step_id)
}

fn manifest_steps(manifest: &MacroManifest) -> Vec<(MacroSection, &MacroStep)> {
    let mut steps = Vec::new();
    for step in &manifest.preconditions {
        steps.push((MacroSection::Precondition, step));
    }
    for step in &manifest.steps {
        steps.push((MacroSection::Step, step));
    }
    for step in &manifest.waits {
        steps.push((MacroSection::Wait, step));
    }
    for step in &manifest.assertions {
        steps.push((MacroSection::Assertion, step));
    }
    for step in &manifest.cleanup {
        steps.push((MacroSection::Cleanup, step));
    }
    steps
}

fn primary_manifest_steps(manifest: &MacroManifest) -> Vec<(MacroSection, &MacroStep)> {
    let mut steps = Vec::new();
    for step in &manifest.preconditions {
        steps.push((MacroSection::Precondition, step));
    }
    for step in &manifest.steps {
        steps.push((MacroSection::Step, step));
    }
    for step in &manifest.waits {
        steps.push((MacroSection::Wait, step));
    }
    for step in &manifest.assertions {
        steps.push((MacroSection::Assertion, step));
    }
    steps
}

fn step_failed(result: &MacroStepResult) -> bool {
    matches!(result.status, MacroStepStatus::Failed)
}

fn step_error_from_output(tool: &str, output: &Value) -> MacroStepError {
    let code = output
        .get("error")
        .and_then(|error| error.get("code"))
        .and_then(Value::as_str)
        .unwrap_or("tool_returned_error")
        .to_owned();
    let message = output
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .unwrap_or("macro step tool returned ok=false")
        .to_owned();
    MacroStepError {
        kind: error_kind_for_tool(tool, &code),
        code,
        message,
        diagnostics: output.clone(),
    }
}

fn error_kind_for_tool(tool: &str, code: &str) -> MacroStepErrorKind {
    if code.contains("identity") || code.contains("binding") || code.contains("bound_id") {
        MacroStepErrorKind::TargetIdentityFailure
    } else if code.contains("timeout") {
        MacroStepErrorKind::WaitTimeout
    } else if tool.starts_with("macro.assert") || code.contains("assert") {
        MacroStepErrorKind::AssertionFailure
    } else if code.contains("policy") || code.contains("denied") {
        MacroStepErrorKind::PolicyDenial
    } else {
        MacroStepErrorKind::ToolExecutionFailure
    }
}

fn artifacts_from_output(tool: &str) -> Vec<MacroArtifact> {
    if !tool.starts_with("capture.") && !tool.starts_with("uia.") {
        return Vec::new();
    }
    vec![MacroArtifact {
        kind: tool.into(),
        path: None,
        metadata: Value::Null,
    }]
}

fn attach_video_artifact(result: &mut MacroExecutionResult, stop: Value) {
    if stop.get("ok").and_then(Value::as_bool).unwrap_or(false) {
        let recording = stop.get("recording").cloned().unwrap_or(Value::Null);
        let path = recording
            .get("output_path")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        result.artifacts.push(MacroArtifact {
            kind: "capture.video".into(),
            path,
            metadata: recording,
        });
    } else {
        result
            .diagnostics
            .push(format!("video recording failed to stop: {stop}"));
    }
}

fn macro_filters_match(stored: &StoredMacro, kind: Option<&str>, tags: &[String]) -> bool {
    kind.map(|kind| stored.kind == kind).unwrap_or(true)
        && tags.iter().all(|tag| {
            stored
                .tags
                .iter()
                .any(|item_tag| item_tag.eq_ignore_ascii_case(tag))
        })
}

fn macro_search_text(manifest: &MacroManifest) -> String {
    let mut text = format!("{}\n{}", manifest.title, manifest.description);
    if !manifest.tags.is_empty() {
        text.push_str("\ntags: ");
        text.push_str(&manifest.tags.join(" "));
    }
    if let Some(AppIdentity {
        executable_name,
        product_name,
        ..
    }) = &manifest.app_identity
    {
        if let Some(executable_name) = executable_name {
            text.push_str("\nexecutable: ");
            text.push_str(executable_name);
        }
        if let Some(product_name) = product_name {
            text.push_str("\nproduct: ");
            text.push_str(product_name);
        }
    }
    text
}

fn policy_denied(code: &str, message: &str) -> serde_json::Value {
    tracing::warn!(code = code, "macro request denied by policy");
    serde_json::json!({
        "ok": false,
        "error": {
            "code": code,
            "message": message,
        }
    })
}
