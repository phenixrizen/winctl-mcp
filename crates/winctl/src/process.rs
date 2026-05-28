use std::collections::HashMap;
use std::path::Path;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub fn basename(path: &str) -> Option<String> {
    Path::new(path)
        .file_name()
        .map(|v| v.to_string_lossy().to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProcessInfo {
    pub pid: u32,
    pub process_name: Option<String>,
    pub exe_path: Option<String>,
    pub parent_pid: Option<u32>,
    pub command_line: Option<String>,
    pub terminal_like: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct ProcessLaunchSpec {
    pub exe: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub env: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProcessLaunchResult {
    pub pid: u32,
    pub process_name: Option<String>,
    pub executable_path: Option<String>,
    pub parent_pid: Option<u32>,
    pub launch_time_unix_ms: u64,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProcessKillResult {
    pub pid: u32,
    pub process_name: Option<String>,
    pub kill_attempted: bool,
    pub exited: bool,
    pub exit_code: Option<u32>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Error)]
#[error("{message}")]
pub struct ProcessError {
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub warnings: Vec<String>,
}

pub fn list_processes() -> Result<Vec<ProcessInfo>, ProcessError> {
    #[cfg(windows)]
    {
        windows_impl::list_processes()
    }

    #[cfg(not(windows))]
    {
        Ok(non_windows_processes())
    }
}

pub fn describe_process(pid: u32) -> Result<Option<ProcessInfo>, ProcessError> {
    Ok(list_processes()?
        .into_iter()
        .find(|process| process.pid == pid))
}

pub fn child_processes(parent_pid: u32) -> Result<Vec<ProcessInfo>, ProcessError> {
    Ok(list_processes()?
        .into_iter()
        .filter(|process| process.parent_pid == Some(parent_pid))
        .collect())
}

pub fn launch_process(spec: ProcessLaunchSpec) -> Result<ProcessLaunchResult, ProcessError> {
    #[cfg(windows)]
    {
        windows_impl::launch_process(spec)
    }

    #[cfg(not(windows))]
    {
        let _ = spec;
        Err(ProcessError {
            code: "unsupported_platform".into(),
            message: "process.launch requires Windows runtime".into(),
            warnings: vec![],
        })
    }
}

pub fn kill_process(pid: u32) -> Result<ProcessKillResult, ProcessError> {
    #[cfg(windows)]
    {
        windows_impl::kill_process(pid)
    }

    #[cfg(not(windows))]
    {
        Err(ProcessError {
            code: "unsupported_platform".into(),
            message: format!("process.kill requires Windows runtime; refused pid {pid}"),
            warnings: vec![],
        })
    }
}

pub fn is_terminal_process_name(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "windowsterminal.exe" | "cmd.exe" | "powershell.exe" | "pwsh.exe"
    )
}

pub fn windows_argv_command_line(exe: &str, args: &[String]) -> String {
    std::iter::once(exe)
        .chain(args.iter().map(String::as_str))
        .map(windows_quote_arg)
        .collect::<Vec<_>>()
        .join(" ")
}

fn windows_quote_arg(arg: &str) -> String {
    if arg.is_empty() {
        return "\"\"".into();
    }
    if !arg
        .bytes()
        .any(|b| matches!(b, b' ' | b'\t' | b'\n' | b'"'))
    {
        return arg.into();
    }

    let mut out = String::from("\"");
    let mut backslashes = 0usize;
    for ch in arg.chars() {
        match ch {
            '\\' => backslashes += 1,
            '"' => {
                out.extend(std::iter::repeat_n('\\', backslashes * 2 + 1));
                out.push('"');
                backslashes = 0;
            }
            _ => {
                out.extend(std::iter::repeat_n('\\', backslashes));
                backslashes = 0;
                out.push(ch);
            }
        }
    }
    out.extend(std::iter::repeat_n('\\', backslashes * 2));
    out.push('"');
    out
}

#[cfg(not(windows))]
fn non_windows_processes() -> Vec<ProcessInfo> {
    let pid = std::process::id();
    let exe_path = std::env::current_exe()
        .ok()
        .map(|path| path.to_string_lossy().to_string());
    let process_name = exe_path.as_deref().and_then(basename);
    vec![ProcessInfo {
        pid,
        process_name: process_name.clone(),
        exe_path,
        parent_pid: None,
        command_line: None,
        terminal_like: process_name
            .as_deref()
            .map(is_terminal_process_name)
            .unwrap_or(false),
        warnings: vec!["full process enumeration requires Windows runtime".into()],
    }]
}

#[cfg(windows)]
mod windows_impl {
    use std::collections::{BTreeMap, HashMap};
    use std::ffi::{c_void, OsStr, OsString};
    use std::mem::size_of;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    use windows::core::{PCWSTR, PWSTR};
    use windows::Win32::Foundation::{CloseHandle, WAIT_TIMEOUT};
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::Threading::{
        CreateProcessW, GetExitCodeProcess, OpenProcess, QueryFullProcessImageNameW,
        TerminateProcess, WaitForSingleObject, CREATE_UNICODE_ENVIRONMENT, PROCESS_CREATION_FLAGS,
        PROCESS_INFORMATION, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        PROCESS_SYNCHRONIZE, PROCESS_TERMINATE, STARTUPINFOW,
    };

    use super::{
        basename, is_terminal_process_name, windows_argv_command_line, ProcessError, ProcessInfo,
        ProcessKillResult, ProcessLaunchResult, ProcessLaunchSpec,
    };

    const STILL_ACTIVE_CODE: u32 = 259;

    pub(super) fn list_processes() -> Result<Vec<ProcessInfo>, ProcessError> {
        let snapshot =
            unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }.map_err(|error| {
                ProcessError {
                    code: "process_snapshot_failed".into(),
                    message: format!("failed to create process snapshot: {error}"),
                    warnings: vec![],
                }
            })?;

        let mut entry = PROCESSENTRY32W::default();
        entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
        let mut processes = Vec::new();
        let first = unsafe { Process32FirstW(snapshot, &mut entry) };
        if first.is_ok() {
            loop {
                processes.push(process_info_from_entry(&entry));
                if unsafe { Process32NextW(snapshot, &mut entry) }.is_err() {
                    break;
                }
            }
        }
        let _ = unsafe { CloseHandle(snapshot) };
        Ok(processes)
    }

    pub(super) fn launch_process(
        spec: ProcessLaunchSpec,
    ) -> Result<ProcessLaunchResult, ProcessError> {
        if spec.exe.trim().is_empty() {
            return Err(ProcessError {
                code: "missing_exe".into(),
                message: "process.launch requires a non-empty exe path".into(),
                warnings: vec![],
            });
        }

        let command_line = windows_argv_command_line(&spec.exe, &spec.args);
        let mut command_line_wide = wide_null(&command_line);
        let exe_wide = wide_null(&spec.exe);
        let cwd_wide = spec.cwd.as_deref().map(wide_null);
        let env_block = spec.env.as_ref().map(build_environment_block);
        let creation_flags = if env_block.is_some() {
            CREATE_UNICODE_ENVIRONMENT
        } else {
            PROCESS_CREATION_FLAGS(0)
        };
        let mut startup = STARTUPINFOW::default();
        startup.cb = size_of::<STARTUPINFOW>() as u32;
        let mut process_info = PROCESS_INFORMATION::default();
        let current_dir = cwd_wide
            .as_ref()
            .map(|cwd| PCWSTR(cwd.as_ptr()))
            .unwrap_or_else(PCWSTR::null);
        let environment = env_block
            .as_ref()
            .map(|block| block.as_ptr().cast::<c_void>());

        unsafe {
            CreateProcessW(
                PCWSTR(exe_wide.as_ptr()),
                Some(PWSTR(command_line_wide.as_mut_ptr())),
                None,
                None,
                false,
                creation_flags,
                environment,
                current_dir,
                &startup,
                &mut process_info,
            )
        }
        .map_err(|error| ProcessError {
            code: "create_process_failed".into(),
            message: format!("CreateProcessW failed for {}: {error}", spec.exe),
            warnings: vec![],
        })?;

        let pid = process_info.dwProcessId;
        let _ = unsafe { CloseHandle(process_info.hThread) };
        let _ = unsafe { CloseHandle(process_info.hProcess) };
        let mut warnings = Vec::new();
        let metadata = super::describe_process(pid).unwrap_or_else(|error| {
            warnings.push(format!(
                "metadata lookup failed after launch: {}",
                error.message
            ));
            None
        });
        if metadata.is_none() {
            warnings.push("launched process metadata was not immediately available".into());
        }
        let process_name = metadata
            .as_ref()
            .and_then(|process| process.process_name.clone());
        let executable_path = metadata
            .as_ref()
            .and_then(|process| process.exe_path.clone());
        let parent_pid = metadata.as_ref().and_then(|process| process.parent_pid);
        warnings.extend(
            metadata
                .as_ref()
                .map(|process| process.warnings.clone())
                .unwrap_or_default(),
        );

        Ok(ProcessLaunchResult {
            pid,
            process_name,
            executable_path,
            parent_pid,
            launch_time_unix_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_millis() as u64)
                .unwrap_or_default(),
            warnings,
        })
    }

    pub(super) fn kill_process(pid: u32) -> Result<ProcessKillResult, ProcessError> {
        let metadata = super::describe_process(pid).ok().flatten();
        let process_name = metadata
            .as_ref()
            .and_then(|process| process.process_name.clone());
        let process = unsafe {
            OpenProcess(
                PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                false,
                pid,
            )
        }
        .map_err(|error| ProcessError {
            code: "open_process_failed".into(),
            message: format!("failed to open process {pid} for termination: {error}"),
            warnings: vec![],
        })?;

        unsafe { TerminateProcess(process, 1) }.map_err(|error| {
            let _ = unsafe { CloseHandle(process) };
            ProcessError {
                code: "terminate_process_failed".into(),
                message: format!("failed to terminate process {pid}: {error}"),
                warnings: vec![],
            }
        })?;

        let started = Instant::now();
        let mut exited = false;
        let mut exit_code = None;
        while started.elapsed() < Duration::from_secs(5) {
            let wait = unsafe { WaitForSingleObject(process, 50) };
            if wait != WAIT_TIMEOUT {
                let mut code = 0u32;
                if unsafe { GetExitCodeProcess(process, &mut code) }.is_ok() {
                    if code != STILL_ACTIVE_CODE {
                        exit_code = Some(code);
                    }
                }
                exited = true;
                break;
            }
        }
        let _ = unsafe { CloseHandle(process) };

        Ok(ProcessKillResult {
            pid,
            process_name,
            kill_attempted: true,
            exited,
            exit_code,
            warnings: if exited {
                vec![]
            } else {
                vec!["process did not exit before timeout".into()]
            },
        })
    }

    fn process_info_from_entry(entry: &PROCESSENTRY32W) -> ProcessInfo {
        let pid = entry.th32ProcessID;
        let snapshot_name = wide_array_to_string(&entry.szExeFile);
        let parent_pid = (entry.th32ParentProcessID != 0).then_some(entry.th32ParentProcessID);
        let mut warnings = Vec::new();
        let exe_path = match process_exe_path(pid) {
            Ok(path) => path,
            Err(warning) => {
                warnings.push(warning);
                None
            }
        };
        let process_name = exe_path
            .as_deref()
            .and_then(basename)
            .or_else(|| (!snapshot_name.is_empty()).then_some(snapshot_name));
        ProcessInfo {
            pid,
            terminal_like: process_name
                .as_deref()
                .map(is_terminal_process_name)
                .unwrap_or(false),
            process_name,
            exe_path,
            parent_pid,
            command_line: None,
            warnings,
        }
    }

    fn process_exe_path(pid: u32) -> Result<Option<String>, String> {
        if pid == 0 {
            return Ok(None);
        }

        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }
            .map_err(|error| {
                format!(
                    "access denied or unavailable querying executable path for pid {pid}: {error}"
                )
            })?;
        let mut buffer = vec![0u16; 32_768];
        let mut size = buffer.len() as u32;
        let result = unsafe {
            QueryFullProcessImageNameW(
                process,
                PROCESS_NAME_WIN32,
                PWSTR(buffer.as_mut_ptr()),
                &mut size,
            )
        };
        let _ = unsafe { CloseHandle(process) };

        result
            .map(|_| Some(String::from_utf16_lossy(&buffer[..size as usize])))
            .map_err(|error| format!("failed to query executable path for pid {pid}: {error}"))
    }

    fn build_environment_block(overrides: &HashMap<String, String>) -> Vec<u16> {
        let mut env = BTreeMap::new();
        for (key, value) in std::env::vars() {
            env.insert(key, value);
        }
        for (key, value) in overrides {
            env.insert(key.clone(), value.clone());
        }

        let mut block = Vec::new();
        for (key, value) in env {
            block.extend(OsStr::new(&format!("{key}={value}")).encode_wide());
            block.push(0);
        }
        block.push(0);
        block
    }

    fn wide_null(value: &str) -> Vec<u16> {
        OsStr::new(value).encode_wide().chain(Some(0)).collect()
    }

    fn wide_array_to_string(buffer: &[u16]) -> String {
        let len = buffer
            .iter()
            .position(|ch| *ch == 0)
            .unwrap_or(buffer.len());
        OsString::from_wide(&buffer[..len])
            .to_string_lossy()
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_process_names_are_marked() {
        assert!(is_terminal_process_name("WindowsTerminal.exe"));
        assert!(is_terminal_process_name("pwsh.exe"));
        assert!(!is_terminal_process_name("Betty.exe"));
    }

    #[test]
    fn windows_argv_command_line_quotes_without_shell_concatenation() {
        let args = vec![
            "simple".to_string(),
            "has space".to_string(),
            r#"quote"and\slash\"#.to_string(),
        ];
        assert_eq!(
            windows_argv_command_line(r#"C:\Program Files\App\app.exe"#, &args),
            r#""C:\Program Files\App\app.exe" simple "has space" "quote\"and\slash\\""#,
        );
    }
}
