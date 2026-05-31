use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::{
    AppState, BuildRunRequest, CrashReportRequest, MacroExportResultRequest, ProcessMetricsRequest,
    TestReportExportRequest,
};

const ALLOWED_BUILD_TOOLS: &[&str] = &["cargo", "dotnet", "msbuild", "cmake", "ctest"];
const MAX_CAPTURED_OUTPUT: usize = 256 * 1024;

pub fn build_run(state: &AppState, request: BuildRunRequest) -> serde_json::Value {
    tracing::info!(
        program = %request.program,
        args_count = request.args.len(),
        cwd = ?request.cwd,
        timeout_ms = ?request.timeout_ms,
        "build.run requested"
    );
    let basename = build_tool_basename(&request.program);
    if !ALLOWED_BUILD_TOOLS
        .iter()
        .any(|allowed| basename.eq_ignore_ascii_case(allowed))
    {
        return denied(
            "build_tool_not_allowlisted",
            "build.run only supports direct allowlisted build tools: cargo, dotnet, msbuild, cmake, ctest",
        );
    }
    let cwd = match request.cwd.as_ref() {
        Some(cwd) => match validate_cwd(state, cwd) {
            Ok(path) => Some(path),
            Err(error) => return error,
        },
        None => None,
    };
    let started = Instant::now();
    let mut command = Command::new(&request.program);
    command.args(&request.args);
    if let Some(cwd) = &cwd {
        command.current_dir(cwd);
    }
    let output = match command.output() {
        Ok(output) => output,
        Err(error) => {
            return serde_json::json!({
                "ok": false,
                "error": {
                    "code": "build_run_failed_to_start",
                    "message": format!("failed to start {}: {error}", request.program),
                }
            });
        }
    };
    let elapsed_ms = started.elapsed().as_millis() as u64;
    let stdout = bounded_utf8(output.stdout);
    let stderr = bounded_utf8(output.stderr);
    let diagnostics = parse_build_diagnostics(&stdout.text, &stderr.text);
    let timeout_exceeded = request
        .timeout_ms
        .map(|timeout_ms| elapsed_ms > timeout_ms)
        .unwrap_or(false);
    serde_json::json!({
        "ok": output.status.success() && !timeout_exceeded,
        "program": request.program,
        "args": request.args,
        "cwd": cwd,
        "status": {
            "success": output.status.success(),
            "code": output.status.code(),
        },
        "timing": {
            "elapsed_ms": elapsed_ms,
            "timeout_ms": request.timeout_ms,
            "timeout_exceeded": timeout_exceeded,
        },
        "stdout": stdout,
        "stderr": stderr,
        "diagnostics": diagnostics,
        "warnings": if timeout_exceeded {
            vec!["timeout was exceeded after the process completed; preemptive termination is not enabled in this first-pass runner"]
        } else {
            Vec::<&str>::new()
        },
    })
}

pub fn process_metrics(request: ProcessMetricsRequest) -> serde_json::Value {
    tracing::info!(pid = request.pid, "process.metrics requested");
    let process = match winctl::describe_process(request.pid) {
        Ok(process) => process,
        Err(error) => return serde_json::json!({"ok": false, "error": error}),
    };
    serde_json::json!({
        "ok": process.is_some(),
        "pid": request.pid,
        "process": process,
        "metrics": {
            "cpu_percent": serde_json::Value::Null,
            "working_set_bytes": serde_json::Value::Null,
            "handle_count": serde_json::Value::Null,
            "gdi_object_count": serde_json::Value::Null,
            "user_object_count": serde_json::Value::Null,
        },
        "provider_enabled": false,
        "warnings": [
            "native per-process metric counters are not enabled in this build; process identity metadata was returned"
        ],
    })
}

pub fn crash_report(state: &AppState, request: CrashReportRequest) -> serde_json::Value {
    tracing::info!(
        pid = ?request.pid,
        bound_id = ?request.bound_id,
        "diagnostics.crash_report requested"
    );
    let process = request
        .pid
        .and_then(|pid| winctl::describe_process(pid).ok().flatten());
    let windows: Vec<_> = match request.pid {
        Some(pid) => winctl::list_windows()
            .into_iter()
            .filter(|window| window.pid == pid)
            .collect(),
        None => Vec::new(),
    };
    let screenshot = request
        .bound_id
        .clone()
        .map(|bound_id| crate::tools::capture::screenshot_window(state, bound_id));
    serde_json::json!({
        "ok": true,
        "pid": request.pid,
        "bound_id": request.bound_id,
        "process": process,
        "windows": windows,
        "screenshot": screenshot,
        "event_log": {
            "provider_enabled": false,
            "entries": [],
        },
        "wer": {
            "provider_enabled": false,
            "dump_paths": [],
        },
        "warnings": [
            "Windows Event Log and WER dump discovery are not enabled in this build"
        ],
    })
}

pub fn test_report_export(state: &AppState, request: TestReportExportRequest) -> serde_json::Value {
    tracing::info!(
        run_id = %request.run_id,
        format = ?request.format,
        output_path = ?request.output_path,
        "test.report_export requested"
    );
    let exported = crate::tools::macros::macro_export_result(
        state,
        MacroExportResultRequest {
            run_id: request.run_id.clone(),
        },
    );
    if !exported
        .get("ok")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    {
        return exported;
    }
    let result = exported.get("result").cloned().unwrap_or_default();
    let format = request.format.unwrap_or_else(|| "json".into()).to_ascii_lowercase();
    let content = match format.as_str() {
        "json" => serde_json::to_string_pretty(&result).unwrap_or_else(|_| "{}".into()),
        "junit" | "xml" => junit_report(&request.run_id, &result),
        "html" => html_report(&request.run_id, &result),
        _ => {
            return denied(
                "unsupported_report_format",
                "supported report formats are json, junit, and html",
            );
        }
    };
    let extension = match format.as_str() {
        "junit" | "xml" => "xml",
        "html" => "html",
        _ => "json",
    };
    let output_path = request
        .output_path
        .map(PathBuf::from)
        .unwrap_or_else(|| state.capture_dir.join(format!("{}-report.{extension}", request.run_id)));
    if let Some(parent) = output_path.parent() {
        if let Err(error) = fs::create_dir_all(parent) {
            return io_error("report_dir_create_failed", parent, error);
        }
    }
    match fs::write(&output_path, content) {
        Ok(()) => serde_json::json!({
            "ok": true,
            "run_id": request.run_id,
            "format": format,
            "output_path": output_path,
        }),
        Err(error) => io_error("report_write_failed", &output_path, error),
    }
}

fn validate_cwd(state: &AppState, cwd: &str) -> Result<PathBuf, serde_json::Value> {
    let path = PathBuf::from(cwd);
    let canonical = fs::canonicalize(&path)
        .map_err(|error| io_error("build_cwd_unavailable", &path, error))?;
    if state.policy.filesystem_roots.is_empty()
        || state
            .policy
            .filesystem_roots
            .iter()
            .any(|root| canonical.starts_with(root))
    {
        Ok(canonical)
    } else {
        Err(serde_json::json!({
            "ok": false,
            "error": {
                "code": "build_cwd_not_allowed",
                "message": format!("cwd {} is outside configured filesystem roots", canonical.display()),
                "allowed_roots": state.policy.filesystem_roots,
            }
        }))
    }
}

#[derive(serde::Serialize)]
struct BoundedText {
    text: String,
    bytes: usize,
    truncated: bool,
}

fn bounded_utf8(bytes: Vec<u8>) -> BoundedText {
    let original_len = bytes.len();
    let truncated = original_len > MAX_CAPTURED_OUTPUT;
    let bytes = if truncated {
        bytes.into_iter().take(MAX_CAPTURED_OUTPUT).collect()
    } else {
        bytes
    };
    BoundedText {
        text: String::from_utf8_lossy(&bytes).into_owned(),
        bytes: original_len,
        truncated,
    }
}

fn parse_build_diagnostics(stdout: &str, stderr: &str) -> serde_json::Value {
    let mut warnings = Vec::new();
    let mut errors = Vec::new();
    for (stream, text) in [("stdout", stdout), ("stderr", stderr)] {
        for (index, line) in text.lines().enumerate() {
            let lower = line.to_ascii_lowercase();
            if lower.contains("error") {
                errors.push(serde_json::json!({"stream": stream, "line": index + 1, "text": line}));
            } else if lower.contains("warning") {
                warnings.push(serde_json::json!({"stream": stream, "line": index + 1, "text": line}));
            }
        }
    }
    serde_json::json!({
        "errors": errors,
        "warnings": warnings,
    })
}

fn build_tool_basename(program: &str) -> String {
    Path::new(program)
        .file_stem()
        .or_else(|| Path::new(program).file_name())
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| program.to_owned())
}

fn junit_report(run_id: &str, result: &serde_json::Value) -> String {
    let status = result
        .get("status")
        .and_then(|value| value.as_str())
        .unwrap_or("unknown");
    let failures = if status.eq_ignore_ascii_case("succeeded") {
        0
    } else {
        1
    };
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<testsuite name="winctl" tests="1" failures="{failures}">
  <testcase classname="winctl" name="{run_id}">
    {failure}
  </testcase>
</testsuite>
"#,
        failure = if failures == 0 {
            String::new()
        } else {
            format!(r#"<failure message="run status {status}"/>"#)
        }
    )
}

fn html_report(run_id: &str, result: &serde_json::Value) -> String {
    let result_json = serde_json::to_string_pretty(result).unwrap_or_else(|_| "{}".into());
    format!(
        r#"<!doctype html>
<html lang="en">
<head><meta charset="utf-8"><title>winctl report {run_id}</title></head>
<body><h1>winctl report {run_id}</h1><pre>{}</pre></body>
</html>
"#,
        html_escape(&result_json)
    )
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
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
    tracing::warn!(code = code, "diagnostics request denied");
    serde_json::json!({
        "ok": false,
        "error": {
            "code": code,
            "message": message,
        }
    })
}

#[allow(dead_code)]
fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}
