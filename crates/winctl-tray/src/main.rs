use std::ffi::OsString;
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use anyhow::Context;
use serde::Serialize;

fn main() {
    if let Err(error) = run() {
        eprintln!("winctl-tray: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    let cli = Cli::parse(std::env::args_os().skip(1))?;
    match cli.command {
        CommandMode::Status => print_status(&cli.config),
        CommandMode::Start => start_server(&cli.config),
        CommandMode::Stop => stop_server(&cli.config),
        CommandMode::Restart => {
            let _ = stop_server(&cli.config);
            start_server(&cli.config)
        }
        CommandMode::CopyMcpUrl => {
            println!("{}", cli.config.mcp_url());
            Ok(())
        }
        CommandMode::OpenDashboard => open_dashboard(&cli.config),
        CommandMode::RunTray => run_tray_placeholder(&cli.config),
    }
}

#[derive(Debug, Clone)]
struct Cli {
    command: CommandMode,
    config: TrayConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommandMode {
    Status,
    Start,
    Stop,
    Restart,
    CopyMcpUrl,
    OpenDashboard,
    RunTray,
}

#[derive(Debug, Clone)]
struct TrayConfig {
    server_exe: PathBuf,
    listen: SocketAddr,
    log_file: Option<PathBuf>,
    config_file: Option<PathBuf>,
    pid_file: PathBuf,
}

#[derive(Debug, Serialize)]
struct TrayStatus {
    running: bool,
    pid: Option<u32>,
    server_exe: PathBuf,
    listen: SocketAddr,
    mcp_url: String,
    dashboard_url: String,
    pid_file: PathBuf,
}

impl Default for TrayConfig {
    fn default() -> Self {
        let data_dir = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("winctl-mcp");
        Self {
            server_exe: PathBuf::from("winctl-mcp-server.exe"),
            listen: "127.0.0.1:8765".parse().expect("default listen is valid"),
            log_file: Some(data_dir.join("server.log")),
            config_file: None,
            pid_file: data_dir.join("winctl-mcp-server.pid"),
        }
    }
}

impl TrayConfig {
    fn mcp_url(&self) -> String {
        format!("http://{}/mcp", self.listen)
    }

    fn dashboard_url(&self) -> String {
        format!("http://{}/dashboard", self.listen)
    }
}

impl Cli {
    fn parse<I>(args: I) -> anyhow::Result<Self>
    where
        I: IntoIterator<Item = OsString>,
    {
        let mut config = TrayConfig::default();
        let mut command = CommandMode::Status;
        let args: Vec<_> = args.into_iter().collect();
        let mut index = 0usize;
        while index < args.len() {
            let arg = args[index].to_string_lossy();
            match arg.as_ref() {
                "status" => command = CommandMode::Status,
                "start" => command = CommandMode::Start,
                "stop" => command = CommandMode::Stop,
                "restart" => command = CommandMode::Restart,
                "copy-mcp-url" => command = CommandMode::CopyMcpUrl,
                "open-dashboard" => command = CommandMode::OpenDashboard,
                "run" => command = CommandMode::RunTray,
                "--server-exe" => {
                    index += 1;
                    config.server_exe = required_path(&args, index, "--server-exe")?;
                }
                "--listen" => {
                    index += 1;
                    let value = required_string(&args, index, "--listen")?;
                    config.listen = value
                        .parse()
                        .with_context(|| format!("invalid listen address {value}"))?;
                }
                "--log-file" => {
                    index += 1;
                    config.log_file = Some(required_path(&args, index, "--log-file")?);
                }
                "--config" => {
                    index += 1;
                    config.config_file = Some(required_path(&args, index, "--config")?);
                }
                "--pid-file" => {
                    index += 1;
                    config.pid_file = required_path(&args, index, "--pid-file")?;
                }
                other => anyhow::bail!("unknown argument {other}"),
            }
            index += 1;
        }
        Ok(Self { command, config })
    }
}

fn start_server(config: &TrayConfig) -> anyhow::Result<()> {
    if let Some(parent) = config.pid_file.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let mut command = Command::new(&config.server_exe);
    command
        .arg("serve")
        .arg("--transport")
        .arg("http")
        .arg("--listen")
        .arg(config.listen.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(config_file) = &config.config_file {
        command.arg("--config").arg(config_file);
    }
    if let Some(log_file) = &config.log_file {
        command.arg("--log-file").arg(log_file);
    }
    let child = command.spawn().with_context(|| {
        format!(
            "failed to launch server executable {}",
            config.server_exe.display()
        )
    })?;
    fs::write(&config.pid_file, child.id().to_string())
        .with_context(|| format!("failed to write {}", config.pid_file.display()))?;
    println!("started winctl-mcp-server pid {}", child.id());
    Ok(())
}

fn stop_server(config: &TrayConfig) -> anyhow::Result<()> {
    let pid = read_pid(&config.pid_file)?;
    #[cfg(windows)]
    {
        winctl::kill_process(pid).with_context(|| format!("failed to stop server pid {pid}"))?;
    }
    #[cfg(not(windows))]
    {
        Command::new("kill")
            .arg(pid.to_string())
            .status()
            .with_context(|| format!("failed to invoke kill for pid {pid}"))?;
    }
    let _ = fs::remove_file(&config.pid_file);
    println!("stopped winctl-mcp-server pid {pid}");
    Ok(())
}

fn print_status(config: &TrayConfig) -> anyhow::Result<()> {
    let pid = read_pid(&config.pid_file).ok();
    let running = pid.map(is_process_alive).unwrap_or(false);
    let status = TrayStatus {
        running,
        pid,
        server_exe: config.server_exe.clone(),
        listen: config.listen,
        mcp_url: config.mcp_url(),
        dashboard_url: config.dashboard_url(),
        pid_file: config.pid_file.clone(),
    };
    println!("{}", serde_json::to_string_pretty(&status)?);
    Ok(())
}

fn open_dashboard(config: &TrayConfig) -> anyhow::Result<()> {
    let url = config.dashboard_url();
    #[cfg(windows)]
    {
        Command::new("explorer.exe")
            .arg(&url)
            .spawn()
            .context("failed to open dashboard with explorer.exe")?;
    }
    #[cfg(not(windows))]
    {
        println!("{url}");
    }
    Ok(())
}

fn run_tray_placeholder(config: &TrayConfig) -> anyhow::Result<()> {
    print_status(config)?;
    eprintln!("native system tray UI is not enabled in this build; use start/stop/status commands");
    Ok(())
}

fn read_pid(path: &PathBuf) -> anyhow::Result<u32> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read pid file {}", path.display()))?;
    text.trim()
        .parse()
        .with_context(|| format!("invalid pid file {}", path.display()))
}

fn is_process_alive(pid: u32) -> bool {
    #[cfg(windows)]
    {
        winctl::describe_process(pid).ok().flatten().is_some()
    }
    #[cfg(not(windows))]
    {
        PathBuf::from(format!("/proc/{pid}")).exists()
    }
}

fn required_path(args: &[OsString], index: usize, flag: &str) -> anyhow::Result<PathBuf> {
    Ok(PathBuf::from(required_string(args, index, flag)?))
}

fn required_string(args: &[OsString], index: usize, flag: &str) -> anyhow::Result<String> {
    args.get(index)
        .map(|value| value.to_string_lossy().to_string())
        .ok_or_else(|| anyhow::anyhow!("{flag} requires a value"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_controller_args() {
        let cli = Cli::parse([
            OsString::from("start"),
            OsString::from("--server-exe"),
            OsString::from("C:/winctl/winctl-mcp-server.exe"),
            OsString::from("--listen"),
            OsString::from("127.0.0.1:9999"),
            OsString::from("--pid-file"),
            OsString::from("C:/winctl/server.pid"),
        ])
        .unwrap();
        assert_eq!(cli.command, CommandMode::Start);
        assert_eq!(
            cli.config.server_exe,
            PathBuf::from("C:/winctl/winctl-mcp-server.exe")
        );
        assert_eq!(cli.config.listen.port(), 9999);
    }
}
