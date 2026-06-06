#![cfg_attr(windows, windows_subsystem = "windows")]

use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use anyhow::Context;

fn main() {
    if let Err(error) = run() {
        append_launcher_log(&format!("fatal error: {error:#}"));
        #[cfg(not(windows))]
        eprintln!("winctl-launcher: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    let tray_exe = tray_exe_path()?;
    let mut command = Command::new(&tray_exe);
    configure_detached_child(&mut command);
    command
        .args(std::env::args_os().skip(1))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("failed to launch {}", tray_exe.display()))?;
    Ok(())
}

fn tray_exe_path() -> anyhow::Result<PathBuf> {
    let current_exe = std::env::current_exe().context("failed to resolve launcher path")?;
    let parent = current_exe
        .parent()
        .ok_or_else(|| anyhow::anyhow!("launcher path has no parent directory"))?;
    Ok(parent.join(tray_exe_name()))
}

#[cfg(windows)]
fn tray_exe_name() -> OsString {
    OsString::from("winctl-tray.exe")
}

#[cfg(not(windows))]
fn tray_exe_name() -> OsString {
    OsString::from("winctl-tray")
}

#[cfg(windows)]
fn configure_detached_child(command: &mut Command) {
    use std::os::windows::process::CommandExt;

    const DETACHED_PROCESS: u32 = 0x00000008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
    command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
}

#[cfg(not(windows))]
fn configure_detached_child(_command: &mut Command) {}

fn append_launcher_log(message: &str) {
    let dir = app_data_dir();
    if fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join("launcher.log");
    let timestamp = chrono::Utc::now().to_rfc3339();
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{timestamp} {message}");
    }
}

fn app_data_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("winctl-mcp")
}
