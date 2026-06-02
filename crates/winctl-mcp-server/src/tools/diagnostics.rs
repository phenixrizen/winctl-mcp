use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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
    let output = match run_command_with_timeout(command, request.timeout_ms) {
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
    let success = output
        .status
        .map(|status| status.success())
        .unwrap_or(false);
    serde_json::json!({
        "ok": success && !output.timed_out,
        "program": request.program,
        "args": request.args,
        "cwd": cwd,
        "status": {
            "success": success,
            "code": output.status.and_then(|status| status.code()),
            "terminated_by_timeout": output.timed_out,
            "kill_error": output.kill_error,
        },
        "timing": {
            "elapsed_ms": elapsed_ms,
            "timeout_ms": request.timeout_ms,
            "timeout_exceeded": output.timed_out,
        },
        "stdout": stdout,
        "stderr": stderr,
        "diagnostics": diagnostics,
        "warnings": if output.timed_out {
            vec!["timeout was exceeded and the build process was terminated"]
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
    let metrics = match winctl::process_metrics(request.pid) {
        Ok(metrics) => metrics,
        Err(error) => {
            return serde_json::json!({
                "ok": false,
                "pid": request.pid,
                "process": process,
                "error": error,
            });
        }
    };
    serde_json::json!({
        "ok": process.is_some(),
        "pid": request.pid,
        "process": process,
        "metrics": metrics,
        "provider_enabled": true,
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
    let process_name = process
        .as_ref()
        .and_then(|process| process.process_name.clone());
    let event_log = windows_event_log_entries(process_name.as_deref(), request.pid);
    let wer = discover_wer_dumps(process_name.as_deref());
    serde_json::json!({
        "ok": true,
        "pid": request.pid,
        "bound_id": request.bound_id,
        "process": process,
        "windows": windows,
        "screenshot": screenshot,
        "event_log": event_log,
        "wer": wer,
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
    let format = request
        .format
        .unwrap_or_else(|| "json".into())
        .to_ascii_lowercase();
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
    let output_path = request.output_path.map(PathBuf::from).unwrap_or_else(|| {
        state
            .capture_dir
            .join(format!("{}-report.{extension}", request.run_id))
    });
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
    let canonical =
        fs::canonicalize(&path).map_err(|error| io_error("build_cwd_unavailable", &path, error))?;
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
                warnings
                    .push(serde_json::json!({"stream": stream, "line": index + 1, "text": line}));
            }
        }
    }
    serde_json::json!({
        "errors": errors,
        "warnings": warnings,
    })
}

struct CapturedCommandOutput {
    status: Option<ExitStatus>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    timed_out: bool,
    kill_error: Option<String>,
}

fn run_command_with_timeout(
    mut command: Command,
    timeout_ms: Option<u64>,
) -> Result<CapturedCommandOutput, std::io::Error> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn()?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_reader = thread::spawn(move || read_pipe(stdout));
    let stderr_reader = thread::spawn(move || read_pipe(stderr));
    let timeout = timeout_ms.map(Duration::from_millis);
    let started = Instant::now();
    let mut timed_out = false;
    let mut kill_error = None;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        if timeout
            .map(|timeout| started.elapsed() >= timeout)
            .unwrap_or(false)
        {
            timed_out = true;
            if let Err(error) = child.kill() {
                kill_error = Some(error.to_string());
            }
            break Some(child.wait()?);
        }
        thread::sleep(Duration::from_millis(50));
    };

    let stdout = stdout_reader.join().unwrap_or_default();
    let stderr = stderr_reader.join().unwrap_or_default();
    Ok(CapturedCommandOutput {
        status,
        stdout,
        stderr,
        timed_out,
        kill_error,
    })
}

fn read_pipe(pipe: Option<impl Read>) -> Vec<u8> {
    let Some(mut pipe) = pipe else {
        return Vec::new();
    };
    let mut buffer = Vec::new();
    let _ = pipe.read_to_end(&mut buffer);
    buffer
}

#[cfg(windows)]
fn windows_event_log_entries(process_name: Option<&str>, pid: Option<u32>) -> serde_json::Value {
    match query_windows_event_log(process_name, pid) {
        Ok(entries) => serde_json::json!({
            "provider_enabled": true,
            "source": "wevtapi",
            "channel": "Application",
            "entries": entries,
        }),
        Err(error) => serde_json::json!({
            "provider_enabled": false,
            "source": "wevtapi",
            "channel": "Application",
            "entries": [],
            "error": {
                "code": "event_log_query_failed",
                "message": error,
            }
        }),
    }
}

#[cfg(not(windows))]
fn windows_event_log_entries(process_name: Option<&str>, pid: Option<u32>) -> serde_json::Value {
    let _ = (process_name, pid);
    serde_json::json!({
        "provider_enabled": false,
        "entries": [],
        "error": {
            "code": "unsupported_platform",
            "message": "Windows Event Log discovery requires Windows runtime",
        }
    })
}

#[cfg(windows)]
fn query_windows_event_log(
    process_name: Option<&str>,
    pid: Option<u32>,
) -> Result<Vec<serde_json::Value>, String> {
    use windows::core::{w, HRESULT};
    use windows::Win32::Foundation::ERROR_NO_MORE_ITEMS;
    use windows::Win32::System::EventLog::{
        EvtClose, EvtNext, EvtQuery, EvtQueryChannelPath, EvtQueryReverseDirection, EVT_HANDLE,
    };

    const MAX_EVENTS_TO_SCAN: usize = 80;
    const MAX_MATCHES: usize = 10;

    let query = unsafe {
        EvtQuery(
            None,
            w!("Application"),
            w!("*[System[(Level=1 or Level=2)]]"),
            EvtQueryChannelPath.0 | EvtQueryReverseDirection.0,
        )
    }
    .map_err(|error| error.to_string())?;

    let mut entries = Vec::new();
    let mut scanned = 0usize;
    let mut handles = [0isize; 8];
    loop {
        let mut returned = 0u32;
        match unsafe { EvtNext(query, &mut handles, 1_000, 0, &mut returned) } {
            Ok(()) => {}
            Err(error) if error.code() == HRESULT::from_win32(ERROR_NO_MORE_ITEMS.0) => break,
            Err(error) => {
                unsafe {
                    let _ = EvtClose(query);
                }
                return Err(error.to_string());
            }
        }
        if returned == 0 {
            break;
        }
        for raw_handle in handles.iter().copied().take(returned as usize) {
            scanned += 1;
            let handle = EVT_HANDLE(raw_handle);
            let entry = render_event_xml(handle)
                .ok()
                .and_then(|xml| structured_event_from_xml(&xml, process_name, pid));
            unsafe {
                let _ = EvtClose(handle);
            }
            if let Some(entry) = entry {
                entries.push(entry);
                if entries.len() >= MAX_MATCHES {
                    break;
                }
            }
            if scanned >= MAX_EVENTS_TO_SCAN {
                break;
            }
        }
        if entries.len() >= MAX_MATCHES || scanned >= MAX_EVENTS_TO_SCAN {
            break;
        }
    }
    unsafe {
        let _ = EvtClose(query);
    }
    Ok(entries)
}

#[cfg(windows)]
fn render_event_xml(
    handle: windows::Win32::System::EventLog::EVT_HANDLE,
) -> Result<String, String> {
    use windows::Win32::System::EventLog::{EvtRender, EvtRenderEventXml};

    let mut buffer_used = 0u32;
    let mut property_count = 0u32;
    let _ = unsafe {
        EvtRender(
            None,
            handle,
            EvtRenderEventXml.0,
            0,
            None,
            &mut buffer_used,
            &mut property_count,
        )
    };
    if buffer_used == 0 {
        return Err("EvtRender did not report an XML buffer size".into());
    }
    let mut buffer = vec![0u16; (buffer_used as usize).div_ceil(2)];
    unsafe {
        EvtRender(
            None,
            handle,
            EvtRenderEventXml.0,
            buffer_used,
            Some(buffer.as_mut_ptr().cast()),
            &mut buffer_used,
            &mut property_count,
        )
    }
    .map_err(|error| error.to_string())?;
    let len = buffer
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(buffer.len());
    Ok(String::from_utf16_lossy(&buffer[..len]))
}

#[cfg(windows)]
fn structured_event_from_xml(
    xml: &str,
    process_name: Option<&str>,
    pid: Option<u32>,
) -> Option<serde_json::Value> {
    if !event_matches_filter(xml, process_name, pid) {
        return None;
    }
    structured_event_from_xml_unfiltered(xml).ok()
}

#[cfg(any(windows, test))]
fn structured_event_from_xml_unfiltered(xml: &str) -> Result<serde_json::Value, String> {
    let document = roxmltree::Document::parse(xml).map_err(|error| error.to_string())?;
    let root = document.root_element();
    let system = root
        .children()
        .find(|node| node.tag_name().name() == "System")
        .ok_or_else(|| "event XML did not include a System node".to_owned())?;
    let provider = system
        .children()
        .find(|node| node.tag_name().name() == "Provider")
        .map(|node| {
            serde_json::json!({
                "name": node.attribute("Name"),
                "guid": node.attribute("Guid"),
            })
        })
        .unwrap_or_else(|| serde_json::json!({}));
    let execution = system
        .children()
        .find(|node| node.tag_name().name() == "Execution")
        .map(|node| {
            serde_json::json!({
                "process_id": node.attribute("ProcessID").and_then(parse_u32),
                "thread_id": node.attribute("ThreadID").and_then(parse_u32),
            })
        })
        .unwrap_or_else(|| serde_json::json!({}));
    let event_data = root
        .descendants()
        .filter(|node| node.tag_name().name() == "Data")
        .enumerate()
        .map(|(index, node)| {
            serde_json::json!({
                "index": index,
                "name": node.attribute("Name"),
                "value": node.text().unwrap_or_default(),
            })
        })
        .collect::<Vec<_>>();

    Ok(serde_json::json!({
        "system": {
            "provider": provider,
            "event_id": child_text(&system, "EventID").and_then(|value| parse_u32(&value)),
            "qualifiers": system.children()
                .find(|node| node.tag_name().name() == "EventID")
                .and_then(|node| node.attribute("Qualifiers"))
                .and_then(parse_u32),
            "level": child_text(&system, "Level").and_then(|value| parse_u32(&value)),
            "task": child_text(&system, "Task").and_then(|value| parse_u32(&value)),
            "opcode": child_text(&system, "Opcode").and_then(|value| parse_u32(&value)),
            "keywords": child_text(&system, "Keywords"),
            "time_created": system.children()
                .find(|node| node.tag_name().name() == "TimeCreated")
                .and_then(|node| node.attribute("SystemTime"))
                .map(ToOwned::to_owned),
            "event_record_id": child_text(&system, "EventRecordID").and_then(|value| parse_u64(&value)),
            "channel": child_text(&system, "Channel"),
            "computer": child_text(&system, "Computer"),
            "execution": execution,
        },
        "event_data": event_data,
        "raw_xml": xml,
    }))
}

#[cfg(any(windows, test))]
fn child_text(parent: &roxmltree::Node<'_, '_>, name: &str) -> Option<String> {
    parent
        .children()
        .find(|node| node.tag_name().name() == name)
        .and_then(|node| node.text())
        .map(ToOwned::to_owned)
}

#[cfg(any(windows, test))]
fn event_matches_filter(xml: &str, process_name: Option<&str>, pid: Option<u32>) -> bool {
    let process_name = process_name.map(|value| value.to_ascii_lowercase());
    let pid_values = pid
        .map(|pid| {
            vec![
                pid.to_string(),
                format!("0x{pid:x}"),
                format!("0x{pid:X}"),
                format!("{pid:x}"),
                format!("{pid:X}"),
            ]
        })
        .unwrap_or_default();
    if process_name.is_none() && pid_values.is_empty() {
        return true;
    }
    let lower = xml.to_ascii_lowercase();
    process_name
        .as_ref()
        .map(|name| lower.contains(name))
        .unwrap_or(false)
        || pid_values
            .iter()
            .any(|pid| lower.contains(&pid.to_ascii_lowercase()))
}

#[cfg(any(windows, test))]
fn parse_u32(value: &str) -> Option<u32> {
    value.parse().ok()
}

#[cfg(any(windows, test))]
fn parse_u64(value: &str) -> Option<u64> {
    value.parse().ok()
}

#[cfg(windows)]
fn discover_wer_dumps(process_name: Option<&str>) -> serde_json::Value {
    let mut roots = Vec::new();
    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
        roots.push(PathBuf::from(local_app_data).join("CrashDumps"));
    }
    if let Some(program_data) = std::env::var_os("PROGRAMDATA") {
        roots.push(
            PathBuf::from(program_data.clone())
                .join("Microsoft")
                .join("Windows")
                .join("WER")
                .join("ReportQueue"),
        );
        roots.push(
            PathBuf::from(program_data)
                .join("Microsoft")
                .join("Windows")
                .join("WER")
                .join("ReportArchive"),
        );
    }
    let mut warnings = Vec::new();
    let mut dump_paths = Vec::new();
    for root in roots {
        collect_wer_paths(&root, process_name, &mut dump_paths, &mut warnings);
    }
    dump_paths.sort();
    dump_paths.dedup();
    serde_json::json!({
        "provider_enabled": true,
        "dump_paths": dump_paths,
        "warnings": warnings,
    })
}

#[cfg(not(windows))]
fn discover_wer_dumps(process_name: Option<&str>) -> serde_json::Value {
    let _ = process_name;
    serde_json::json!({
        "provider_enabled": false,
        "dump_paths": [],
        "error": {
            "code": "unsupported_platform",
            "message": "WER dump discovery requires Windows runtime",
        }
    })
}

#[cfg(windows)]
fn collect_wer_paths(
    root: &Path,
    process_name: Option<&str>,
    output: &mut Vec<String>,
    warnings: &mut Vec<String>,
) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    let process_name = process_name.map(|value| value.to_ascii_lowercase());
    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = path
            .file_name()
            .map(|value| value.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        let matches_process = process_name
            .as_ref()
            .map(|name| file_name.contains(name))
            .unwrap_or(true);
        if path.is_dir() {
            collect_wer_paths(&path, process_name.as_deref(), output, warnings);
        } else if matches_process
            && matches!(
                path.extension()
                    .and_then(|value| value.to_str())
                    .map(str::to_ascii_lowercase)
                    .as_deref(),
                Some("dmp" | "mdmp" | "wer")
            )
        {
            output.push(path.to_string_lossy().to_string());
        }
    }
    if output.len() > 100 {
        output.truncate(100);
        warnings.push("WER discovery truncated at 100 paths".into());
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_structured_event_xml() {
        let xml = r#"
<Event xmlns="http://schemas.microsoft.com/win/2004/08/events/event">
  <System>
    <Provider Name="Application Error" />
    <EventID Qualifiers="0">1000</EventID>
    <Level>2</Level>
    <Task>100</Task>
    <TimeCreated SystemTime="2026-05-31T12:34:56.0000000Z" />
    <EventRecordID>42</EventRecordID>
    <Channel>Application</Channel>
    <Computer>desktop</Computer>
    <Execution ProcessID="1234" ThreadID="5678" />
  </System>
  <EventData>
    <Data Name="AppName">winctl-test-target.exe</Data>
    <Data Name="ProcessId">0x4d2</Data>
  </EventData>
</Event>
"#;

        let event = structured_event_from_xml_unfiltered(xml).expect("event XML should parse");

        assert_eq!(event["system"]["provider"]["name"], "Application Error");
        assert_eq!(event["system"]["event_id"], 1000);
        assert_eq!(event["system"]["execution"]["process_id"], 1234);
        assert_eq!(event["event_data"][0]["name"], "AppName");
        assert!(event_matches_filter(
            xml,
            Some("winctl-test-target.exe"),
            Some(1234)
        ));
    }
}
