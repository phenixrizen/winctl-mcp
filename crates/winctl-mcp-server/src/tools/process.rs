use std::collections::{HashMap, HashSet};
use std::thread;
use std::time::{Duration, Instant};

use crate::{
    AppLaunchRequest, AppState, ProcessDescribeRequest, ProcessKillRequest, ProcessLaunchRequest,
    ProcessListRequest, ProcessWaitForExitRequest, WaitForWindowRequest,
};
use winctl::{
    child_processes, describe_process, kill_process, launch_app, launch_process, list_processes,
    list_windows, AppLaunchSpec, ProcessInfo, ProcessLaunchResult, ProcessLaunchSpec, WindowInfo,
};

pub fn process_launch(state: &AppState, request: ProcessLaunchRequest) -> serde_json::Value {
    tracing::info!(
        exe = %request.exe,
        args_count = request.args.len(),
        cwd = ?request.cwd,
        wait_for_window = request.wait_for_window,
        timeout_ms = request.timeout_ms,
        allow_child_process_windows = request.allow_child_process_windows,
        "process.launch requested"
    );

    let spec = ProcessLaunchSpec {
        exe: request.exe,
        args: request.args,
        cwd: request.cwd,
        env: request.env,
    };

    match launch_process(spec.clone()) {
        Ok(launch) => {
            let tracked = state.track_launch(&spec, &launch);
            tracing::info!(
                launch_id = %tracked.launch_id,
                pid = launch.pid,
                process_name = ?launch.process_name,
                executable_path = ?launch.executable_path,
                "process.launch succeeded"
            );
            let window_result = if request.wait_for_window {
                Some(windows_wait_for_window(
                    state,
                    WaitForWindowRequest {
                        pid: Some(launch.pid),
                        launch_id: Some(tracked.launch_id.clone()),
                        timeout_ms: request.timeout_ms,
                        title_contains: None,
                        class_name_contains: None,
                        allow_child_process_windows: request.allow_child_process_windows,
                    },
                ))
            } else {
                None
            };

            serde_json::json!({
                "ok": true,
                "launch_id": tracked.launch_id,
                "pid": launch.pid,
                "process_name": launch.process_name,
                "executable_path": launch.executable_path,
                "launch_time_unix_ms": launch.launch_time_unix_ms,
                "launched_by_this_server": true,
                "parent_pid": launch.parent_pid,
                "candidate_top_level_windows": window_result
                    .as_ref()
                    .and_then(|value| value.get("matched_windows"))
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!([])),
                "window_wait": window_result,
                "warnings": launch.warnings,
            })
        }
        Err(error) => {
            tracing::warn!(
                error_code = %error.code,
                "process.launch failed"
            );
            serde_json::json!({"ok": false, "error": error})
        }
    }
}

pub fn app_launch(state: &AppState, request: AppLaunchRequest) -> serde_json::Value {
    tracing::info!(
        mode = ?request.mode,
        target = %request.target,
        args_count = request.args.len(),
        cwd = ?request.cwd,
        wait_for_window = request.wait_for_window,
        timeout_ms = request.timeout_ms,
        allow_child_process_windows = request.allow_child_process_windows,
        "app.launch requested"
    );
    let spec = AppLaunchSpec {
        mode: request.mode,
        target: request.target,
        args: request.args,
        cwd: request.cwd,
    };
    match launch_app(spec.clone()) {
        Ok(launch) => {
            let tracked = launch.pid.map(|pid| {
                state.track_launch(
                    &ProcessLaunchSpec {
                        exe: spec.target.clone(),
                        args: spec.args.clone(),
                        cwd: spec.cwd.clone(),
                        env: None,
                    },
                    &ProcessLaunchResult {
                        pid,
                        process_name: launch.process_name.clone(),
                        executable_path: launch.executable_path.clone(),
                        parent_pid: None,
                        launch_time_unix_ms: launch.launch_time_unix_ms,
                        warnings: launch.warnings.clone(),
                    },
                )
            });
            let window_result = if request.wait_for_window {
                launch.pid.map(|pid| {
                    windows_wait_for_window(
                        state,
                        WaitForWindowRequest {
                            pid: Some(pid),
                            launch_id: tracked.as_ref().map(|tracked| tracked.launch_id.clone()),
                            timeout_ms: request.timeout_ms,
                            title_contains: None,
                            class_name_contains: None,
                            allow_child_process_windows: request.allow_child_process_windows,
                        },
                    )
                })
            } else {
                None
            };
            serde_json::json!({
                "ok": true,
                "mode": launch.mode,
                "target": launch.target,
                "pid": launch.pid,
                "process_name": launch.process_name,
                "executable_path": launch.executable_path,
                "launch_time_unix_ms": launch.launch_time_unix_ms,
                "launched_by_this_server": tracked.is_some(),
                "launch_id": tracked.as_ref().map(|tracked| tracked.launch_id.clone()),
                "candidate_top_level_windows": window_result
                    .as_ref()
                    .and_then(|value| value.get("matched_windows"))
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!([])),
                "window_wait": window_result,
                "warnings": launch.warnings,
            })
        }
        Err(error) => {
            tracing::warn!(error_code = %error.code, "app.launch failed");
            serde_json::json!({"ok": false, "error": error})
        }
    }
}

pub fn process_list(state: &AppState, request: ProcessListRequest) -> serde_json::Value {
    tracing::info!(
        name_contains = ?request.name_contains,
        exe_path_contains = ?request.exe_path_contains,
        include_windows = request.include_windows,
        only_mcp_launched = request.only_mcp_launched,
        "process.list requested"
    );
    let windows_by_pid = if request.include_windows {
        windows_by_pid(list_windows())
    } else {
        HashMap::new()
    };
    match list_processes() {
        Ok(processes) => {
            let mut warnings = Vec::new();
            let processes: Vec<_> = processes
                .into_iter()
                .filter(|process| matches_process_filters(process, &request))
                .filter_map(|process| {
                    let tracked = state.tracked_by_pid(process.pid);
                    if request.only_mcp_launched && tracked.is_none() {
                        return None;
                    }
                    warnings.extend(process.warnings.clone());
                    let owned_windows = if request.include_windows {
                        Some(
                            windows_by_pid
                                .get(&process.pid)
                                .cloned()
                                .unwrap_or_default(),
                        )
                    } else {
                        None
                    };
                    Some(decorate_process(process, tracked, owned_windows))
                })
                .collect();
            serde_json::json!({
                "ok": true,
                "processes": processes,
                "warnings": warnings,
            })
        }
        Err(error) => {
            tracing::warn!(error_code = %error.code, "process.list failed");
            serde_json::json!({"ok": false, "error": error})
        }
    }
}

pub fn process_describe(state: &AppState, request: ProcessDescribeRequest) -> serde_json::Value {
    tracing::info!(pid = request.pid, "process.describe requested");
    match describe_process(request.pid) {
        Ok(Some(process)) => {
            let tracked = state.tracked_by_pid(request.pid);
            let windows: Vec<_> = list_windows()
                .into_iter()
                .filter(|window| window.pid == request.pid)
                .map(|window| decorate_window(window, false))
                .collect();
            let children = child_processes(request.pid).unwrap_or_default();
            let command_line = tracked
                .as_ref()
                .map(|tracked| tracked.command_line.clone())
                .or_else(|| process.command_line.clone());
            serde_json::json!({
                "ok": true,
                "process": decorate_process(process.clone(), tracked.clone(), None),
                "pid": process.pid,
                "process_name": process.process_name,
                "executable_path": process.exe_path,
                "parent_pid": process.parent_pid,
                "command_line": command_line,
                "launched_by_this_server": tracked.is_some(),
                "launch_id": tracked.as_ref().map(|tracked| tracked.launch_id.clone()),
                "child_processes": children,
                "top_level_windows": windows,
                "warnings": process.warnings,
            })
        }
        Ok(None) => {
            tracing::warn!(pid = request.pid, "process.describe process not found");
            serde_json::json!({
                "ok": false,
                "error": {
                    "code": "process_not_found",
                    "message": format!("pid {} was not found", request.pid)
                }
            })
        }
        Err(error) => {
            tracing::warn!(
                pid = request.pid,
                error_code = %error.code,
                "process.describe failed"
            );
            serde_json::json!({"ok": false, "error": error})
        }
    }
}

pub fn process_kill(state: &AppState, request: ProcessKillRequest) -> serde_json::Value {
    tracing::info!(
        pid = ?request.pid,
        launch_id = ?request.launch_id,
        force = request.force,
        kill_tree = request.kill_tree,
        "process.kill requested"
    );
    let tracked = match resolve_tracked_process(state, request.pid, request.launch_id.as_deref()) {
        Ok(tracked) => tracked,
        Err(error) => {
            tracing::warn!(
                pid = ?request.pid,
                launch_id = ?request.launch_id,
                error_code = %error["code"].as_str().unwrap_or("process_not_owned_by_server"),
                "process.kill ownership rejected"
            );
            return serde_json::json!({"ok": false, "error": error});
        }
    };

    if request.kill_tree {
        return serde_json::json!({
            "ok": false,
            "error": {
                "code": "kill_tree_not_implemented",
                "message": "kill_tree is not implemented; child processes are not terminated by default"
            }
        });
    }
    if !request.force {
        return serde_json::json!({
            "ok": false,
            "error": {
                "code": "force_required",
                "message": "graceful process termination is not implemented; call process.kill with force=true"
            }
        });
    }

    if let Some(current) = describe_process(tracked.pid).ok().flatten() {
        if let Err(error) = validate_tracked_process_identity(&tracked, &current) {
            tracing::warn!(
                launch_id = %tracked.launch_id,
                pid = tracked.pid,
                expected_executable_path = ?tracked.executable_path,
                actual_executable_path = ?current.exe_path,
                expected_process_name = ?tracked.process_name,
                actual_process_name = ?current.process_name,
                "process.kill ownership identity check failed"
            );
            return serde_json::json!({"ok": false, "error": error});
        }
    }

    match kill_process(tracked.pid) {
        Ok(outcome) => {
            tracing::info!(
                launch_id = %tracked.launch_id,
                pid = tracked.pid,
                process_name = ?outcome.process_name,
                exited = outcome.exited,
                exit_code = ?outcome.exit_code,
                "process.kill completed"
            );
            if outcome.exited {
                state.forget_launch(&tracked.launch_id);
            }
            serde_json::json!({
                "ok": true,
                "pid": outcome.pid,
                "launch_id": tracked.launch_id,
                "process_name": outcome.process_name,
                "ownership_status": "owned_by_server",
                "kill_attempted": outcome.kill_attempted,
                "exited": outcome.exited,
                "exit_code": outcome.exit_code,
                "warnings": outcome.warnings,
            })
        }
        Err(error) => {
            tracing::warn!(
                launch_id = %tracked.launch_id,
                pid = tracked.pid,
                error_code = %error.code,
                "process.kill failed"
            );
            serde_json::json!({"ok": false, "error": error})
        }
    }
}

pub fn process_wait_for_exit(
    state: &AppState,
    request: ProcessWaitForExitRequest,
) -> serde_json::Value {
    tracing::info!(
        pid = ?request.pid,
        launch_id = ?request.launch_id,
        timeout_ms = request.timeout_ms,
        poll_interval_ms = request.poll_interval_ms,
        "process.wait_for_exit requested"
    );
    let pid = match resolve_pid(state, request.pid, request.launch_id.as_deref()) {
        Ok(pid) => pid,
        Err(error) => return serde_json::json!({"ok": false, "error": error}),
    };
    let timeout = Duration::from_millis(request.timeout_ms.unwrap_or(5_000));
    let poll_interval = Duration::from_millis(request.poll_interval_ms.unwrap_or(100).max(10));
    let started = Instant::now();
    let mut last_process = describe_process(pid).ok().flatten();

    loop {
        match describe_process(pid) {
            Ok(Some(process)) => {
                last_process = Some(process);
            }
            Ok(None) => {
                tracing::info!(
                    pid = pid,
                    launch_id = ?request.launch_id,
                    elapsed_ms = started.elapsed().as_millis() as u64,
                    "process.wait_for_exit matched"
                );
                if let Some(launch_id) = &request.launch_id {
                    state.forget_launch(launch_id);
                }
                return serde_json::json!({
                    "ok": true,
                    "pid": pid,
                    "launch_id": request.launch_id,
                    "exited": true,
                    "timeout": false,
                    "timing": {"elapsed_ms": started.elapsed().as_millis() as u64},
                    "last_process": last_process,
                });
            }
            Err(error) => {
                tracing::warn!(
                    pid = pid,
                    launch_id = ?request.launch_id,
                    error_code = %error.code,
                    "process.wait_for_exit describe failed"
                );
                return serde_json::json!({"ok": false, "error": error});
            }
        }

        if started.elapsed() >= timeout {
            tracing::warn!(
                pid = pid,
                launch_id = ?request.launch_id,
                timeout_ms = timeout.as_millis() as u64,
                "process.wait_for_exit timed out"
            );
            return serde_json::json!({
                "ok": false,
                "pid": pid,
                "launch_id": request.launch_id,
                "exited": false,
                "timeout": true,
                "timing": {"elapsed_ms": started.elapsed().as_millis() as u64},
                "last_process": last_process,
                "error": {
                    "code": "process_exit_timeout",
                    "message": format!("pid {pid} did not exit before timeout")
                }
            });
        }

        thread::sleep(poll_interval);
    }
}

pub fn windows_wait_for_window(
    state: &AppState,
    request: WaitForWindowRequest,
) -> serde_json::Value {
    tracing::info!(
        pid = ?request.pid,
        launch_id = ?request.launch_id,
        timeout_ms = request.timeout_ms,
        title_contains = ?request.title_contains,
        class_name_contains = ?request.class_name_contains,
        allow_child_process_windows = request.allow_child_process_windows,
        "windows.wait_for_window requested"
    );
    let pid = match resolve_pid(state, request.pid, request.launch_id.as_deref()) {
        Ok(pid) => pid,
        Err(error) => return serde_json::json!({"ok": false, "error": error}),
    };
    let timeout = Duration::from_millis(request.timeout_ms.unwrap_or(5_000));
    let started = Instant::now();
    let mut last_process = describe_process(pid).ok().flatten();
    let mut warnings = Vec::new();

    loop {
        let child_pid_set = if request.allow_child_process_windows {
            match child_processes(pid) {
                Ok(children) => children.into_iter().map(|child| child.pid).collect(),
                Err(error) => {
                    warnings.push(error.message);
                    HashSet::new()
                }
            }
        } else {
            HashSet::new()
        };
        let matches = matching_windows(pid, &child_pid_set, &request);
        if !matches.is_empty() {
            tracing::info!(
                pid = pid,
                launch_id = ?request.launch_id,
                matched_windows = matches.len(),
                "windows.wait_for_window matched"
            );
            return serde_json::json!({
                "ok": true,
                "pid": pid,
                "launch_id": request.launch_id,
                "matched_windows": matches,
                "timeout": false,
                "process": last_process,
                "warnings": warnings,
            });
        }

        last_process = describe_process(pid).ok().flatten();
        if started.elapsed() >= timeout {
            tracing::warn!(
                pid = pid,
                launch_id = ?request.launch_id,
                timeout_ms = timeout.as_millis() as u64,
                "windows.wait_for_window timed out"
            );
            return serde_json::json!({
                "ok": false,
                "pid": pid,
                "launch_id": request.launch_id,
                "matched_windows": [],
                "timeout": true,
                "process": last_process,
                "warnings": warnings,
                "error": {
                    "code": "window_timeout",
                    "message": format!("no visible top-level window appeared for pid {pid} before timeout")
                }
            });
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn validate_tracked_process_identity(
    tracked: &crate::TrackedProcess,
    current: &ProcessInfo,
) -> Result<(), serde_json::Value> {
    if let (Some(expected), Some(actual)) = (&tracked.executable_path, &current.exe_path) {
        if expected.eq_ignore_ascii_case(actual) {
            return Ok(());
        }
        return Err(serde_json::json!({
            "code": "tracked_process_identity_mismatch",
            "message": "tracked process PID now points at a different executable path",
            "expected_executable_path": expected,
            "actual_executable_path": actual,
        }));
    }

    if let (Some(expected), Some(actual)) = (&tracked.process_name, &current.process_name) {
        if expected.eq_ignore_ascii_case(actual) {
            return Ok(());
        }
        return Err(serde_json::json!({
            "code": "tracked_process_identity_mismatch",
            "message": "tracked process PID now points at a different process name",
            "expected_process_name": expected,
            "actual_process_name": actual,
        }));
    }

    Err(serde_json::json!({
        "code": "process_ownership_not_verifiable",
        "message": "current process identity could not be verified before termination",
    }))
}

fn matches_process_filters(process: &ProcessInfo, request: &ProcessListRequest) -> bool {
    if let Some(name) = &request.name_contains {
        let Some(process_name) = &process.process_name else {
            return false;
        };
        if !contains_case_insensitive(process_name, name) {
            return false;
        }
    }
    if let Some(exe_path) = &request.exe_path_contains {
        let Some(process_exe_path) = &process.exe_path else {
            return false;
        };
        if !contains_case_insensitive(process_exe_path, exe_path) {
            return false;
        }
    }
    true
}

fn decorate_process(
    process: ProcessInfo,
    tracked: Option<crate::TrackedProcess>,
    windows: Option<Vec<WindowInfo>>,
) -> serde_json::Value {
    serde_json::json!({
        "pid": process.pid,
        "process_name": process.process_name,
        "executable_path": process.exe_path,
        "parent_pid": process.parent_pid,
        "command_line": tracked
            .as_ref()
            .map(|tracked| tracked.command_line.clone())
            .or(process.command_line),
        "terminal_like": process.terminal_like,
        "launched_by_this_server": tracked.is_some(),
        "launch_id": tracked.as_ref().map(|tracked| tracked.launch_id.clone()),
        "owned_top_level_windows": windows,
        "warnings": process.warnings,
    })
}

fn windows_by_pid(windows: Vec<WindowInfo>) -> HashMap<u32, Vec<WindowInfo>> {
    let mut by_pid: HashMap<u32, Vec<WindowInfo>> = HashMap::new();
    for window in windows {
        if window.top_level && window.visible {
            by_pid.entry(window.pid).or_default().push(window);
        }
    }
    by_pid
}

fn matching_windows(
    pid: u32,
    child_pids: &HashSet<u32>,
    request: &WaitForWindowRequest,
) -> Vec<serde_json::Value> {
    list_windows()
        .into_iter()
        .filter(|window| window.top_level && window.visible)
        .filter(|window| window.pid == pid || child_pids.contains(&window.pid))
        .filter(|window| {
            request
                .title_contains
                .as_ref()
                .map(|needle| contains_case_insensitive(&window.title, needle))
                .unwrap_or(true)
        })
        .filter(|window| {
            request
                .class_name_contains
                .as_ref()
                .map(|needle| contains_case_insensitive(&window.class_name, needle))
                .unwrap_or(true)
        })
        .map(|window| {
            let child_process_window = window.pid != pid;
            decorate_window(window, child_process_window)
        })
        .collect()
}

fn decorate_window(window: WindowInfo, child_process_window: bool) -> serde_json::Value {
    serde_json::json!({
        "hwnd": window.hwnd,
        "hwnd_hex": window.hwnd_hex,
        "id": window.id,
        "pid": window.pid,
        "process_name": window.process_name,
        "executable_path": window.exe_path,
        "title": window.title,
        "class_name": window.class_name,
        "visible": window.visible,
        "minimized": window.minimized,
        "cloaked": window.cloaked,
        "foreground": window.foreground,
        "top_level": window.top_level,
        "rect": {
            "x": window.x,
            "y": window.y,
            "width": window.width,
            "height": window.height,
        },
        "belongs_to_original_pid": !child_process_window,
        "child_process_window": child_process_window,
    })
}

fn resolve_pid(
    state: &AppState,
    pid: Option<u32>,
    launch_id: Option<&str>,
) -> Result<u32, serde_json::Value> {
    if let Some(launch_id) = launch_id {
        let Some(tracked) = state.tracked_by_launch_id(launch_id) else {
            return Err(serde_json::json!({
                "code": "launch_not_found",
                "message": format!("launch_id {launch_id} is not tracked by this server")
            }));
        };
        if let Some(pid) = pid {
            if pid != tracked.pid {
                return Err(serde_json::json!({
                    "code": "pid_launch_mismatch",
                    "message": format!("pid {pid} does not match launch_id {launch_id} pid {}", tracked.pid)
                }));
            }
        }
        return Ok(tracked.pid);
    }
    pid.ok_or_else(|| {
        serde_json::json!({
            "code": "missing_process_target",
            "message": "pid or launch_id is required"
        })
    })
}

fn resolve_tracked_process(
    state: &AppState,
    pid: Option<u32>,
    launch_id: Option<&str>,
) -> Result<crate::TrackedProcess, serde_json::Value> {
    if let Some(launch_id) = launch_id {
        let Some(tracked) = state.tracked_by_launch_id(launch_id) else {
            return Err(serde_json::json!({
                "code": "process_not_owned_by_server",
                "message": format!("launch_id {launch_id} was not launched by this MCP server session")
            }));
        };
        if let Some(pid) = pid {
            if tracked.pid != pid {
                return Err(serde_json::json!({
                    "code": "pid_launch_mismatch",
                    "message": format!("pid {pid} does not match launch_id {launch_id} pid {}", tracked.pid)
                }));
            }
        }
        return Ok(tracked);
    }

    let Some(pid) = pid else {
        return Err(serde_json::json!({
            "code": "missing_process_target",
            "message": "pid or launch_id is required"
        }));
    };
    state.tracked_by_pid(pid).ok_or_else(|| {
        serde_json::json!({
            "code": "process_not_owned_by_server",
            "message": format!("pid {pid} was not launched by this MCP server session")
        })
    })
}

fn contains_case_insensitive(haystack: &str, needle: &str) -> bool {
    haystack
        .to_ascii_lowercase()
        .contains(&needle.to_ascii_lowercase())
}
