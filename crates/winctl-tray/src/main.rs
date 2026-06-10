use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream, UdpSocket};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::Value;

fn main() {
    if let Err(error) = run() {
        append_tray_log(&format!("fatal error: {error:#}"));
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
        CommandMode::OpenRecorder | CommandMode::RecordingToggle => open_recorder(&cli.config),
        CommandMode::DashboardWindow => run_dashboard_window(&cli.config),
        CommandMode::RunTray => run_tray(&cli.config),
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
    OpenRecorder,
    RecordingToggle,
    DashboardWindow,
    RunTray,
}

#[derive(Debug, Clone)]
struct TrayConfig {
    server_exe: PathBuf,
    listen: SocketAddr,
    listen_overridden: bool,
    auth_token: Option<String>,
    auth_token_file: PathBuf,
    dashboard_url_override: Option<String>,
    log_file: Option<PathBuf>,
    log_file_overridden: bool,
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
        let data_dir = app_data_dir();
        let server_exe = std::env::current_exe()
            .ok()
            .and_then(|path| {
                path.parent()
                    .map(|parent| parent.join("winctl-mcp-server.exe"))
            })
            .unwrap_or_else(|| PathBuf::from("winctl-mcp-server.exe"));
        Self {
            server_exe,
            listen: "0.0.0.0:8765".parse().expect("default listen is valid"),
            listen_overridden: false,
            auth_token: None,
            auth_token_file: data_dir.join("http-auth-token"),
            dashboard_url_override: None,
            log_file: Some(data_dir.join("server.log")),
            log_file_overridden: false,
            config_file: None,
            pid_file: data_dir.join("winctl-mcp-server.pid"),
        }
    }
}

fn app_data_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("winctl-mcp")
}

fn append_tray_log(message: &str) {
    let dir = app_data_dir();
    if fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join("tray.log");
    let timestamp = chrono::Utc::now().to_rfc3339();
    let sanitized = sanitize_log_message(message);
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{timestamp} {sanitized}");
    }
}

fn sanitize_log_message(message: &str) -> String {
    let mut output = String::with_capacity(message.len());
    let mut remaining = message;
    while let Some(index) = remaining.find("token=") {
        output.push_str(&remaining[..index + "token=".len()]);
        output.push_str("<redacted>");
        let after_token = &remaining[index + "token=".len()..];
        let end = after_token
            .find(|character: char| {
                matches!(character, '&' | '"' | '\'' | '`' | ')' | ' ' | '\r' | '\n')
            })
            .unwrap_or(after_token.len());
        remaining = &after_token[end..];
    }
    output.push_str(remaining);
    output
}

impl TrayConfig {
    fn mcp_url(&self) -> String {
        format!("http://{}/mcp", self.mcp_addr())
    }

    fn dashboard_url(&self) -> String {
        if let Some(url) = &self.dashboard_url_override {
            return url.clone();
        }
        self.dashboard_base_url()
    }

    fn dashboard_status_url(&self) -> String {
        sanitize_log_message(&self.dashboard_url())
    }

    fn recorder_url(&self) -> String {
        self.dashboard_url_with_tab("recorder")
    }

    fn dashboard_base_url(&self) -> String {
        let dashboard_addr = self.dashboard_addr();
        match &self.auth_token {
            Some(token) if !token.is_empty() => format!(
                "http://{}/dashboard?token={}",
                dashboard_addr,
                percent_encode_query_value(token)
            ),
            _ => format!("http://{dashboard_addr}/dashboard"),
        }
    }

    fn dashboard_addr(&self) -> SocketAddr {
        if self.listen.ip().is_unspecified() {
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), self.listen.port())
        } else {
            self.listen
        }
    }

    fn listener_probe_addr(&self) -> SocketAddr {
        self.dashboard_addr()
    }

    fn mcp_addr(&self) -> SocketAddr {
        if self.listen.ip().is_unspecified() {
            let host = default_reachable_ip().unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
            SocketAddr::new(host, self.listen.port())
        } else {
            self.listen
        }
    }

    fn dashboard_url_with_tab(&self, tab: &str) -> String {
        let base = self
            .dashboard_url_override
            .clone()
            .unwrap_or_else(|| self.dashboard_base_url());
        append_query_param(base, "tab", tab)
    }

    fn apply_config_file_defaults(&mut self) -> anyhow::Result<()> {
        let Some(config_file) = &self.config_file else {
            return Ok(());
        };
        let text = fs::read_to_string(config_file)
            .with_context(|| format!("failed to read config file {}", config_file.display()))?;
        let config: TrayServerConfigFile = toml::from_str(&text)
            .with_context(|| format!("failed to parse config file {}", config_file.display()))?;
        if !self.listen_overridden {
            if let Some(listen) = config
                .transport
                .as_ref()
                .and_then(|transport| transport.listen.as_ref())
            {
                self.listen = listen
                    .parse()
                    .with_context(|| format!("invalid configured listen address {listen}"))?;
            }
        }
        if let Some(token) = config.auth.as_ref().and_then(|auth| auth.token.as_ref()) {
            self.auth_token = Some(token.clone());
        }
        Ok(())
    }

    fn ensure_auth_token(&mut self) -> anyhow::Result<()> {
        if self
            .auth_token
            .as_ref()
            .is_some_and(|token| !token.is_empty())
        {
            return Ok(());
        }
        if let Ok(token) = fs::read_to_string(&self.auth_token_file) {
            let token = token.trim().to_string();
            if !token.is_empty() {
                self.auth_token = Some(token);
                return Ok(());
            }
        }
        if let Some(parent) = self.auth_token_file.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let token = generate_auth_token();
        fs::write(&self.auth_token_file, format!("{token}\n"))
            .with_context(|| format!("failed to write {}", self.auth_token_file.display()))?;
        self.auth_token = Some(token);
        Ok(())
    }
}

fn generate_auth_token() -> String {
    format!(
        "wctl_{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

fn default_reachable_ip() -> Option<IpAddr> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect((Ipv4Addr::new(8, 8, 8, 8), 80)).ok()?;
    let ip = socket.local_addr().ok()?.ip();
    (!ip.is_loopback() && !ip.is_unspecified()).then_some(ip)
}

fn percent_encode_query_value(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn append_query_param(mut url: String, name: &str, value: &str) -> String {
    let separator = if url.contains('?') {
        if url.ends_with('?') || url.ends_with('&') {
            ""
        } else {
            "&"
        }
    } else {
        "?"
    };
    url.push_str(separator);
    url.push_str(name);
    url.push('=');
    url.push_str(&percent_encode_query_value(value));
    url
}

#[cfg(windows)]
fn icon_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = std::env::var_os("WINCTL_TRAY_ICON") {
        candidates.push(path.into());
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(bin_dir) = exe.parent() {
            candidates.push(bin_dir.join("winctl.ico"));
            if let Some(root_dir) = bin_dir.parent() {
                candidates.push(root_dir.join("assets").join("winctl.ico"));
                candidates.push(root_dir.join("assets").join("brand").join("winctl.ico"));
            }
        }
    }
    candidates.push(PathBuf::from("assets/brand/winctl.ico"));
    candidates
}

#[derive(Debug, Deserialize)]
struct TrayServerConfigFile {
    transport: Option<TrayTransportConfig>,
    auth: Option<TrayAuthConfig>,
}

#[derive(Debug, Deserialize)]
struct TrayTransportConfig {
    listen: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TrayAuthConfig {
    token: Option<String>,
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
                "open-recorder" => command = CommandMode::OpenRecorder,
                "recording-toggle" => command = CommandMode::RecordingToggle,
                "dashboard-window" => command = CommandMode::DashboardWindow,
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
                    config.listen_overridden = true;
                }
                "--log-file" => {
                    index += 1;
                    config.log_file = Some(required_path(&args, index, "--log-file")?);
                    config.log_file_overridden = true;
                }
                "--auth-token" => {
                    index += 1;
                    config.auth_token = Some(required_string(&args, index, "--auth-token")?);
                }
                "--config" => {
                    index += 1;
                    config.config_file = Some(required_path(&args, index, "--config")?);
                }
                "--pid-file" => {
                    index += 1;
                    config.pid_file = required_path(&args, index, "--pid-file")?;
                }
                "--dashboard-url" => {
                    index += 1;
                    config.dashboard_url_override =
                        Some(required_string(&args, index, "--dashboard-url")?);
                }
                other => anyhow::bail!("unknown argument {other}"),
            }
            index += 1;
        }
        config.apply_config_file_defaults()?;
        if command.uses_http_server() {
            config.ensure_auth_token()?;
        }
        Ok(Self { command, config })
    }
}

impl CommandMode {
    fn uses_http_server(self) -> bool {
        !matches!(self, CommandMode::Stop)
    }
}

fn start_server(config: &TrayConfig) -> anyhow::Result<()> {
    if server_listener_accepts(config.listener_probe_addr()) {
        return ensure_current_or_restart(config);
    }
    if let Some(pid) = server_pid(config) {
        println!("winctl-mcp-server already running pid {pid}");
        return Ok(());
    }

    if let Some(parent) = config.pid_file.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let mut command = Command::new(&config.server_exe);
    #[cfg(windows)]
    detach_child_process(&mut command);
    command
        .arg("serve")
        .arg("--transport")
        .arg("http")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if config.config_file.is_none() || config.listen_overridden {
        command.arg("--listen").arg(config.listen.to_string());
    }
    if let Some(config_file) = &config.config_file {
        command.arg("--config").arg(config_file);
    }
    if (config.config_file.is_none() || config.log_file_overridden) && config.log_file.is_some() {
        let log_file = config.log_file.as_ref().expect("checked above");
        command.arg("--log-file").arg(log_file);
    }
    if let Some(token) = &config.auth_token {
        command.arg("--auth-token").arg(token);
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
    let mut pids = server_process_pids(config);
    if pids.is_empty() {
        pids.push(read_pid(&config.pid_file)?);
    }
    pids.sort_unstable();
    pids.dedup();
    for pid in pids {
        stop_server_pid(pid)?;
        println!("stopped winctl-mcp-server pid {pid}");
    }
    let _ = fs::remove_file(&config.pid_file);
    Ok(())
}

fn print_status(config: &TrayConfig) -> anyhow::Result<()> {
    let pid = server_pid(config);
    let running = pid.is_some();
    let status = TrayStatus {
        running,
        pid,
        server_exe: config.server_exe.clone(),
        listen: config.listen,
        mcp_url: config.mcp_url(),
        dashboard_url: config.dashboard_status_url(),
        pid_file: config.pid_file.clone(),
    };
    println!("{}", serde_json::to_string_pretty(&status)?);
    Ok(())
}

fn open_dashboard(config: &TrayConfig) -> anyhow::Result<()> {
    ensure_server_ready(config)?;
    let url = config.dashboard_url();
    #[cfg(windows)]
    {
        spawn_dashboard_window(&url)?;
    }
    #[cfg(not(windows))]
    {
        println!("{url}");
    }
    Ok(())
}

fn open_recorder(config: &TrayConfig) -> anyhow::Result<()> {
    ensure_server_ready(config)?;
    let url = config.recorder_url();
    #[cfg(windows)]
    {
        spawn_dashboard_window(&url)?;
    }
    #[cfg(not(windows))]
    {
        println!("{url}");
    }
    Ok(())
}

fn ensure_server_ready(config: &TrayConfig) -> anyhow::Result<()> {
    let probe_addr = config.listener_probe_addr();
    if server_listener_accepts(probe_addr) {
        return ensure_current_or_restart(config);
    }
    start_server(config)?;
    let timeout = Duration::from_secs(10);
    if wait_for_server_listener(probe_addr, timeout) {
        return Ok(());
    }
    anyhow::bail!(
        "winctl-mcp-server did not accept connections on {} within {} ms (bind {})",
        probe_addr,
        timeout.as_millis(),
        config.listen,
    )
}

fn listener_label(config: &TrayConfig) -> String {
    let probe_addr = config.listener_probe_addr();
    if probe_addr == config.listen {
        probe_addr.to_string()
    } else {
        format!("{probe_addr} (bind {})", config.listen)
    }
}

fn ensure_current_or_restart(config: &TrayConfig) -> anyhow::Result<()> {
    let probe_addr = config.listener_probe_addr();
    let state = fetch_dashboard_state(config)?;
    if dashboard_state_matches_version(&state, build_version())? {
        println!(
            "winctl-mcp-server already listening on {}",
            listener_label(config)
        );
        return Ok(());
    }

    let live_version = state
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    append_tray_log(&format!(
        "restarting stale winctl-mcp-server on {}; live version={live_version}, launcher version={}",
        listener_label(config),
        build_version()
    ));
    stop_matching_server_processes(config)?;
    let stop_timeout = Duration::from_secs(5);
    if !wait_for_server_listener_closed(probe_addr, stop_timeout) {
        anyhow::bail!(
            "winctl-mcp-server did not release {} within {} ms",
            listener_label(config),
            stop_timeout.as_millis()
        );
    }
    start_server(config)?;
    let timeout = Duration::from_secs(10);
    if wait_for_server_listener(probe_addr, timeout) {
        return Ok(());
    }
    anyhow::bail!(
        "winctl-mcp-server did not restart on {} within {} ms",
        listener_label(config),
        timeout.as_millis()
    )
}

fn fetch_dashboard_state(config: &TrayConfig) -> anyhow::Result<Value> {
    let probe_addr = config.listener_probe_addr();
    let mut stream = TcpStream::connect_timeout(&probe_addr, Duration::from_millis(600))
        .context("failed to connect to dashboard state endpoint")?;
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .context("failed to set dashboard read timeout")?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .context("failed to set dashboard write timeout")?;
    let mut request = format!(
        "GET /dashboard/state HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n",
        probe_addr
    );
    if let Some(token) = &config.auth_token {
        request.push_str(&format!("Authorization: Bearer {token}\r\n"));
    }
    request.push_str("\r\n");
    stream
        .write_all(request.as_bytes())
        .context("failed to request dashboard state")?;
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .context("failed to read dashboard state response")?;
    let (headers, body) = response
        .split_once("\r\n\r\n")
        .ok_or_else(|| anyhow::anyhow!("dashboard state response was malformed"))?;
    if !headers.starts_with("HTTP/1.1 200") && !headers.starts_with("HTTP/1.0 200") {
        anyhow::bail!(
            "dashboard state returned {}",
            headers.lines().next().unwrap_or("HTTP error")
        );
    }
    serde_json::from_str(body).context("failed to parse dashboard state JSON")
}

fn wait_for_server_listener_closed(listen: SocketAddr, timeout: Duration) -> bool {
    let started = Instant::now();
    while started.elapsed() < timeout {
        if !server_listener_accepts(listen) {
            return true;
        }
        thread::sleep(Duration::from_millis(100));
    }
    !server_listener_accepts(listen)
}

fn dashboard_state_matches_version(state: &Value, expected_version: &str) -> anyhow::Result<bool> {
    let service = state
        .get("service")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if service != "winctl-mcp-server" {
        anyhow::bail!("listener is not winctl-mcp-server; service={service:?}");
    }
    Ok(state.get("version").and_then(Value::as_str) == Some(expected_version))
}

fn build_version() -> &'static str {
    match option_env!("WINCTL_BUILD_VERSION") {
        Some(version) if !version.is_empty() => version,
        _ => env!("CARGO_PKG_VERSION"),
    }
}

fn stop_matching_server_processes(config: &TrayConfig) -> anyhow::Result<()> {
    let pids = server_process_pids(config);
    if pids.is_empty() {
        anyhow::bail!(
            "refusing to restart stale listener on {}; no running process matched {}",
            listener_label(config),
            config.server_exe.display()
        );
    }
    for pid in pids {
        stop_server_pid(pid)?;
        append_tray_log(&format!("stopped stale winctl-mcp-server pid {pid}"));
    }
    let _ = fs::remove_file(&config.pid_file);
    Ok(())
}

fn stop_server_pid(pid: u32) -> anyhow::Result<()> {
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
    Ok(())
}

fn wait_for_server_listener(listen: SocketAddr, timeout: Duration) -> bool {
    let started = Instant::now();
    while started.elapsed() < timeout {
        if server_listener_accepts(listen) {
            return true;
        }
        thread::sleep(Duration::from_millis(100));
    }
    server_listener_accepts(listen)
}

#[cfg(windows)]
fn detach_child_process(command: &mut Command) {
    use std::os::windows::process::CommandExt;

    const DETACHED_PROCESS: u32 = 0x00000008;
    command.creation_flags(DETACHED_PROCESS);
}

#[cfg(all(windows, target_env = "msvc"))]
fn spawn_dashboard_window(url: &str) -> anyhow::Result<()> {
    let tray_exe = std::env::current_exe().context("failed to resolve tray executable path")?;
    let mut command = Command::new(tray_exe);
    detach_child_process(&mut command);
    command
        .arg("dashboard-window")
        .arg("--dashboard-url")
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("failed to launch dashboard webview window")?;
    Ok(())
}

#[cfg(all(windows, not(target_env = "msvc")))]
fn spawn_dashboard_window(_url: &str) -> anyhow::Result<()> {
    anyhow::bail!("dashboard WebView window requires the x86_64-pc-windows-msvc build")
}

fn run_dashboard_window(config: &TrayConfig) -> anyhow::Result<()> {
    let url = config.dashboard_url();
    #[cfg(all(windows, target_env = "msvc"))]
    {
        run_dashboard_webview(&url)
    }

    #[cfg(all(windows, not(target_env = "msvc")))]
    {
        let _ = url;
        anyhow::bail!("dashboard WebView window requires the x86_64-pc-windows-msvc build")
    }

    #[cfg(not(windows))]
    {
        println!("{url}");
        Ok(())
    }
}

#[cfg(all(windows, target_env = "msvc"))]
fn run_dashboard_webview(url: &str) -> anyhow::Result<()> {
    use winit::application::ApplicationHandler;
    use winit::dpi::{LogicalSize, PhysicalSize};
    use winit::event::WindowEvent;
    use winit::event_loop::{ActiveEventLoop, EventLoop};
    use winit::platform::windows::WindowAttributesExtWindows;
    use winit::window::{Window, WindowId};
    use wry::{WebContext, WebView, WebViewBuilder};

    struct DashboardWebviewApp {
        url: String,
        window: Option<Window>,
        web_context: Option<WebContext>,
        webview: Option<WebView>,
        startup_error: Option<anyhow::Error>,
    }

    impl ApplicationHandler for DashboardWebviewApp {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            if self.window.is_some() || self.startup_error.is_some() {
                return;
            }

            let window_icon = dashboard_window_icon(None);
            let taskbar_icon = dashboard_window_icon(Some(PhysicalSize::new(256, 256)))
                .or_else(|| window_icon.clone());
            let mut attributes = Window::default_attributes()
                .with_title("winctl Dashboard")
                .with_inner_size(LogicalSize::new(1180.0, 820.0))
                .with_min_inner_size(LogicalSize::new(860.0, 620.0));
            if let Some(icon) = window_icon {
                attributes = attributes.with_window_icon(Some(icon));
            }
            if let Some(icon) = taskbar_icon {
                attributes = attributes.with_taskbar_icon(Some(icon));
            }
            let window = match event_loop.create_window(attributes) {
                Ok(window) => window,
                Err(error) => {
                    self.startup_error = Some(anyhow::Error::new(error));
                    event_loop.exit();
                    return;
                }
            };
            let data_dir = app_data_dir().join("webview");
            if let Err(error) = fs::create_dir_all(&data_dir) {
                self.startup_error = Some(anyhow::Error::new(error).context(format!(
                    "failed to create dashboard WebView data directory {}",
                    data_dir.display()
                )));
                event_loop.exit();
                return;
            }
            let mut web_context = WebContext::new(Some(data_dir));
            let webview = match WebViewBuilder::new_with_web_context(&mut web_context)
                .with_url(&self.url)
                .build(&window)
            {
                Ok(webview) => webview,
                Err(error) => {
                    self.startup_error = Some(anyhow::Error::new(error));
                    event_loop.exit();
                    return;
                }
            };
            self.window = Some(window);
            self.web_context = Some(web_context);
            self.webview = Some(webview);
        }

        fn window_event(
            &mut self,
            event_loop: &ActiveEventLoop,
            window_id: WindowId,
            event: WindowEvent,
        ) {
            if self.window.as_ref().map(Window::id) == Some(window_id) {
                if matches!(event, WindowEvent::CloseRequested) {
                    event_loop.exit();
                }
            }
        }
    }

    let event_loop = EventLoop::new().context("failed to create dashboard event loop")?;
    let mut app = DashboardWebviewApp {
        url: url.to_string(),
        window: None,
        web_context: None,
        webview: None,
        startup_error: None,
    };
    event_loop
        .run_app(&mut app)
        .context("dashboard webview event loop failed")?;
    if let Some(error) = app.startup_error {
        return Err(error).context("dashboard webview failed to start");
    }
    Ok(())
}

#[cfg(all(windows, target_env = "msvc"))]
fn dashboard_window_icon(
    size: Option<winit::dpi::PhysicalSize<u32>>,
) -> Option<winit::window::Icon> {
    use winit::platform::windows::IconExtWindows;
    use winit::window::Icon;

    icon_candidates()
        .into_iter()
        .filter(|candidate| candidate.is_file())
        .find_map(|candidate| Icon::from_path(candidate, size).ok())
}

fn run_tray(config: &TrayConfig) -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        windows_tray::run(config.clone())
    }

    #[cfg(not(windows))]
    {
        run_tray_placeholder(config)
    }
}

#[cfg(not(windows))]
fn run_tray_placeholder(config: &TrayConfig) -> anyhow::Result<()> {
    print_status(config)?;
    eprintln!("native system tray UI is not enabled in this build; use start/stop/status commands");
    Ok(())
}

#[cfg(windows)]
fn copy_mcp_url_to_clipboard(config: &TrayConfig) -> anyhow::Result<()> {
    let mut child = Command::new("clip.exe")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("failed to launch clip.exe")?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(config.mcp_url().as_bytes())
            .context("failed to write MCP URL to clip.exe")?;
    }
    let status = child.wait().context("failed to wait for clip.exe")?;
    if !status.success() {
        anyhow::bail!("clip.exe failed with status {status}");
    }
    Ok(())
}

fn server_pid(config: &TrayConfig) -> Option<u32> {
    read_pid(&config.pid_file)
        .ok()
        .filter(|pid| is_server_process_alive(*pid, &config.server_exe))
}

fn server_process_pids(config: &TrayConfig) -> Vec<u32> {
    #[cfg(windows)]
    {
        let expected = normalize_windows_path(&config.server_exe.to_string_lossy());
        winctl::list_processes()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|process| {
                let actual = process.exe_path?;
                let actual = normalize_windows_path(&actual);
                actual
                    .eq_ignore_ascii_case(&expected)
                    .then_some(process.pid)
            })
            .collect()
    }
    #[cfg(not(windows))]
    {
        server_pid(config).into_iter().collect()
    }
}

fn server_listener_accepts(listen: SocketAddr) -> bool {
    TcpStream::connect_timeout(&listen, Duration::from_millis(200)).is_ok()
}

#[cfg(windows)]
mod windows_tray {
    use std::ffi::{c_void, OsStr};
    use std::io::{Read, Write};
    use std::mem::size_of;
    use std::net::TcpStream;
    use std::os::windows::ffi::OsStrExt;
    use std::time::Duration;

    use anyhow::Context;
    use serde_json::Value;
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Shell::{
        Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_ERROR, NIIF_INFO,
        NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyIcon, DestroyMenu,
        DestroyWindow, DispatchMessageW, GetCursorPos, GetMessageW, GetWindowLongPtrW, KillTimer,
        LoadCursorW, LoadIconW, LoadImageW, PostQuitMessage, RegisterClassW, SetForegroundWindow,
        SetTimer, SetWindowLongPtrW, TrackPopupMenu, TranslateMessage, CREATESTRUCTW, CS_HREDRAW,
        CS_VREDRAW, CW_USEDEFAULT, GWLP_USERDATA, HICON, IDC_ARROW, IDI_APPLICATION, IMAGE_ICON,
        LR_DEFAULTSIZE, LR_LOADFROMFILE, MF_ENABLED, MF_GRAYED, MF_SEPARATOR, MF_STRING, MSG,
        TPM_RETURNCMD, TPM_RIGHTBUTTON, WINDOW_EX_STYLE, WM_APP, WM_COMMAND, WM_DESTROY,
        WM_LBUTTONDBLCLK, WM_NCCREATE, WM_RBUTTONUP, WM_TIMER, WNDCLASSW, WS_OVERLAPPED,
    };

    use super::{
        copy_mcp_url_to_clipboard, icon_candidates, open_dashboard, open_recorder, server_pid,
        start_server, stop_server, TrayConfig,
    };

    const TRAY_UID: u32 = 1;
    const WM_TRAYICON: u32 = WM_APP + 1;
    const MENU_OPEN_DASHBOARD: u32 = 1001;
    const MENU_START_SERVER: u32 = 1002;
    const MENU_STOP_SERVER: u32 = 1003;
    const MENU_RESTART_SERVER: u32 = 1004;
    const MENU_COPY_MCP_URL: u32 = 1005;
    const MENU_OPEN_RECORDER: u32 = 1006;
    const MENU_RECORDING_TOGGLE: u32 = 1007;
    const MENU_QUIT: u32 = 1008;
    const COMPLETION_TIMER_ID: usize = 2001;

    struct TrayState {
        config: TrayConfig,
        icon: HICON,
        icon_needs_destroy: bool,
        last_completion_key: Option<String>,
    }

    pub(super) fn run(config: TrayConfig) -> anyhow::Result<()> {
        if !super::server_listener_accepts(config.listener_probe_addr()) {
            if let Err(error) = start_server(&config) {
                eprintln!("failed to start winctl-mcp-server: {error:#}");
            }
        }

        unsafe {
            let module = GetModuleHandleW(None).context("failed to get module handle")?;
            let instance = HINSTANCE(module.0);
            let class_name = w!("winctl_mcp_tray_window");
            let window_name = w!("winctl-mcp tray");
            let (icon, icon_needs_destroy) = load_tray_icon();
            let cursor = LoadCursorW(None, IDC_ARROW).unwrap_or_default();
            let class = WNDCLASSW {
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(window_proc),
                hInstance: instance,
                hIcon: icon,
                hCursor: cursor,
                lpszClassName: class_name,
                ..Default::default()
            };
            let atom = RegisterClassW(&class);
            if atom == 0 {
                return Err(windows::core::Error::from_thread())
                    .context("failed to register tray window class");
            }

            let state = Box::new(TrayState {
                config,
                icon,
                icon_needs_destroy,
                last_completion_key: None,
            });
            let state_ptr = Box::into_raw(state);
            let hwnd = match CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class_name,
                window_name,
                WS_OVERLAPPED,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                None,
                None,
                Some(instance),
                Some(state_ptr as *const c_void),
            ) {
                Ok(hwnd) => hwnd,
                Err(error) => {
                    drop(Box::from_raw(state_ptr));
                    return Err(error).context("failed to create tray window");
                }
            };

            let state = tray_state(hwnd).context("tray state was not attached to window")?;
            if let Err(error) = add_icon(hwnd, state.icon) {
                let _ = DestroyWindow(hwnd);
                return Err(error);
            }
            let _ = SetTimer(Some(hwnd), COMPLETION_TIMER_ID, 5_000, None);

            let mut message = MSG::default();
            while GetMessageW(&mut message, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        Ok(())
    }

    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_NCCREATE => {
                let create = lparam.0 as *const CREATESTRUCTW;
                if !create.is_null() {
                    let state_ptr = unsafe { (*create).lpCreateParams as *mut TrayState };
                    unsafe {
                        SetWindowLongPtrW(hwnd, GWLP_USERDATA, state_ptr as isize);
                    }
                }
                LRESULT(1)
            }
            WM_TRAYICON => {
                let event = (lparam.0 as u32) & 0xffff;
                if event == WM_RBUTTONUP {
                    if let Some(state) = unsafe { tray_state(hwnd) } {
                        let _ = unsafe { show_menu(hwnd, state) };
                    }
                    return LRESULT(0);
                }
                if event == WM_LBUTTONDBLCLK {
                    if let Some(state) = unsafe { tray_state(hwnd) } {
                        if let Err(error) = open_dashboard(&state.config) {
                            eprintln!("tray command failed: {error:#}");
                            let _ = unsafe {
                                show_error_balloon(
                                    hwnd,
                                    "winctl-mcp command failed",
                                    &error.to_string(),
                                )
                            };
                        }
                    }
                    return LRESULT(0);
                }
                unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
            }
            WM_COMMAND => {
                let command_id = (wparam.0 as u32) & 0xffff;
                if let Some(state) = unsafe { tray_state(hwnd) } {
                    unsafe { handle_command(hwnd, state, command_id) };
                }
                LRESULT(0)
            }
            WM_TIMER => {
                if wparam.0 == COMPLETION_TIMER_ID {
                    if let Some(state) = unsafe { tray_state(hwnd) } {
                        let _ = unsafe { poll_completion(hwnd, state) };
                    }
                    return LRESULT(0);
                }
                unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
            }
            WM_DESTROY => {
                let _ = unsafe { KillTimer(Some(hwnd), COMPLETION_TIMER_ID) };
                if let Some(state) = unsafe { tray_state(hwnd) } {
                    let _ = unsafe { remove_icon(hwnd, state.icon) };
                }
                let state_ptr =
                    unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) as *mut TrayState };
                if !state_ptr.is_null() {
                    unsafe {
                        let state = Box::from_raw(state_ptr);
                        if state.icon_needs_destroy {
                            let _ = DestroyIcon(state.icon);
                        }
                    }
                }
                unsafe { PostQuitMessage(0) };
                LRESULT(0)
            }
            _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        }
    }

    unsafe fn tray_state(hwnd: HWND) -> Option<&'static mut TrayState> {
        let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut TrayState };
        if ptr.is_null() {
            None
        } else {
            Some(unsafe { &mut *ptr })
        }
    }

    unsafe fn show_menu(hwnd: HWND, state: &mut TrayState) -> anyhow::Result<()> {
        let menu = unsafe { CreatePopupMenu().context("failed to create tray menu")? };
        let running = server_pid(&state.config).is_some();
        unsafe {
            append_menu_item(menu, MENU_OPEN_DASHBOARD, "Open Dashboard", true)?;
            append_menu_item(menu, MENU_OPEN_RECORDER, "Open Recorder", true)?;
            append_menu_item(menu, MENU_COPY_MCP_URL, "Copy MCP URL", true)?;
            append_separator(menu)?;
            append_menu_item(menu, MENU_RECORDING_TOGGLE, "Recording Toggle", true)?;
            append_separator(menu)?;
            append_menu_item(menu, MENU_START_SERVER, "Start Server", !running)?;
            append_menu_item(menu, MENU_STOP_SERVER, "Stop Server", running)?;
            append_menu_item(menu, MENU_RESTART_SERVER, "Restart Server", true)?;
            append_separator(menu)?;
            append_menu_item(menu, MENU_QUIT, "Quit Tray", true)?;
        }

        let mut point = POINT::default();
        unsafe {
            GetCursorPos(&mut point).context("failed to read cursor position")?;
            let _ = SetForegroundWindow(hwnd);
            let selected = TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_RIGHTBUTTON,
                point.x,
                point.y,
                None,
                hwnd,
                None,
            )
            .0 as u32;
            let _ = DestroyMenu(menu);
            if selected != 0 {
                handle_command(hwnd, state, selected);
            }
        }
        Ok(())
    }

    unsafe fn append_menu_item(
        menu: windows::Win32::UI::WindowsAndMessaging::HMENU,
        id: u32,
        label: &str,
        enabled: bool,
    ) -> windows::core::Result<()> {
        let label = wide_z(label);
        let state = if enabled { MF_ENABLED } else { MF_GRAYED };
        unsafe { AppendMenuW(menu, MF_STRING | state, id as usize, PCWSTR(label.as_ptr())) }
    }

    unsafe fn append_separator(
        menu: windows::Win32::UI::WindowsAndMessaging::HMENU,
    ) -> windows::core::Result<()> {
        unsafe { AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null()) }
    }

    unsafe fn handle_command(hwnd: HWND, state: &mut TrayState, command_id: u32) {
        let result = match command_id {
            MENU_OPEN_DASHBOARD => open_dashboard(&state.config),
            MENU_OPEN_RECORDER | MENU_RECORDING_TOGGLE => open_recorder(&state.config),
            MENU_START_SERVER => start_server(&state.config),
            MENU_STOP_SERVER => stop_server(&state.config),
            MENU_RESTART_SERVER => {
                let _ = stop_server(&state.config);
                start_server(&state.config)
            }
            MENU_COPY_MCP_URL => copy_mcp_url_to_clipboard(&state.config),
            MENU_QUIT => {
                let _ = unsafe { DestroyWindow(hwnd) };
                Ok(())
            }
            _ => Ok(()),
        };

        match result {
            Ok(()) => {
                let _ = unsafe { modify_icon_tip(hwnd, state.icon) };
                if command_id == MENU_COPY_MCP_URL {
                    let _ = unsafe { show_balloon(hwnd, "winctl-mcp", "MCP URL copied") };
                } else if command_id == MENU_RECORDING_TOGGLE {
                    let _ = unsafe { show_balloon(hwnd, "winctl-mcp", "Recorder opened") };
                }
            }
            Err(error) => {
                eprintln!("tray command failed: {error:#}");
                let _ = unsafe {
                    show_error_balloon(hwnd, "winctl-mcp command failed", &error.to_string())
                };
            }
        }
    }

    unsafe fn poll_completion(hwnd: HWND, state: &mut TrayState) -> anyhow::Result<()> {
        let Some(completion) = latest_completion(&state.config)? else {
            return Ok(());
        };
        if state.last_completion_key.is_none() {
            state.last_completion_key = Some(completion.key);
            return Ok(());
        }
        if state.last_completion_key.as_deref() == Some(&completion.key) {
            return Ok(());
        }
        state.last_completion_key = Some(completion.key);
        let title = if completion.status == "succeeded" {
            "winctl-mcp run passed"
        } else {
            "winctl-mcp run failed"
        };
        let message = format!("{} ({})", completion.manifest_title, completion.status);
        if completion.status == "succeeded" {
            unsafe { show_balloon(hwnd, title, &message) }?;
        } else {
            unsafe { show_error_balloon(hwnd, title, &message) }?;
        }
        Ok(())
    }

    struct Completion {
        key: String,
        manifest_title: String,
        status: String,
    }

    fn latest_completion(config: &TrayConfig) -> anyhow::Result<Option<Completion>> {
        let state = fetch_dashboard_state(config)?;
        let mut completions = state
            .get("macro_results")
            .and_then(|value| value.get("results"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|entry| {
                let run_id = entry.get("run_id")?.as_str()?.to_owned();
                let result = entry.get("result")?;
                let status = result.get("status")?.as_str()?.to_owned();
                if !matches!(status.as_str(), "succeeded" | "failed" | "aborted") {
                    return None;
                }
                let finished_at = result
                    .get("finished_at")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                let manifest_title = result
                    .get("manifest_title")
                    .and_then(Value::as_str)
                    .unwrap_or("macro run")
                    .to_owned();
                Some((
                    finished_at.clone(),
                    Completion {
                        key: format!("{run_id}:{status}:{finished_at}"),
                        manifest_title,
                        status,
                    },
                ))
            })
            .collect::<Vec<_>>();
        completions.sort_by(|left, right| right.0.cmp(&left.0));
        Ok(completions
            .into_iter()
            .map(|(_, completion)| completion)
            .next())
    }

    fn fetch_dashboard_state(config: &TrayConfig) -> anyhow::Result<Value> {
        let probe_addr = config.listener_probe_addr();
        let mut stream = TcpStream::connect_timeout(&probe_addr, Duration::from_millis(600))
            .context("failed to connect to dashboard state endpoint")?;
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .context("failed to set dashboard read timeout")?;
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .context("failed to set dashboard write timeout")?;
        let mut request = format!(
            "GET /dashboard/state HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n",
            probe_addr
        );
        if let Some(token) = &config.auth_token {
            request.push_str(&format!("Authorization: Bearer {token}\r\n"));
        }
        request.push_str("\r\n");
        stream
            .write_all(request.as_bytes())
            .context("failed to request dashboard state")?;
        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .context("failed to read dashboard state response")?;
        let (headers, body) = response
            .split_once("\r\n\r\n")
            .ok_or_else(|| anyhow::anyhow!("dashboard state response was malformed"))?;
        if !headers.starts_with("HTTP/1.1 200") && !headers.starts_with("HTTP/1.0 200") {
            anyhow::bail!(
                "dashboard state returned {}",
                headers.lines().next().unwrap_or("HTTP error")
            );
        }
        serde_json::from_str(body).context("failed to decode dashboard state JSON")
    }

    unsafe fn add_icon(hwnd: HWND, icon: HICON) -> anyhow::Result<()> {
        let data = icon_data(hwnd, icon, NIF_MESSAGE | NIF_ICON | NIF_TIP);
        unsafe {
            if !Shell_NotifyIconW(NIM_ADD, &data).as_bool() {
                return Err(windows::core::Error::from_thread())
                    .context("failed to add notification-area icon");
            }
        }
        Ok(())
    }

    unsafe fn modify_icon_tip(hwnd: HWND, icon: HICON) -> anyhow::Result<()> {
        let data = icon_data(hwnd, icon, NIF_ICON | NIF_TIP);
        unsafe {
            if !Shell_NotifyIconW(NIM_MODIFY, &data).as_bool() {
                return Err(windows::core::Error::from_thread())
                    .context("failed to update notification-area icon");
            }
        }
        Ok(())
    }

    unsafe fn remove_icon(hwnd: HWND, icon: HICON) -> anyhow::Result<()> {
        let data = icon_data(hwnd, icon, NIF_MESSAGE);
        unsafe {
            if !Shell_NotifyIconW(NIM_DELETE, &data).as_bool() {
                return Err(windows::core::Error::from_thread())
                    .context("failed to remove notification-area icon");
            }
        }
        Ok(())
    }

    unsafe fn show_balloon(hwnd: HWND, title: &str, message: &str) -> anyhow::Result<()> {
        show_balloon_with_flags(hwnd, title, message, NIIF_INFO)
    }

    unsafe fn show_error_balloon(hwnd: HWND, title: &str, message: &str) -> anyhow::Result<()> {
        show_balloon_with_flags(hwnd, title, message, NIIF_ERROR)
    }

    unsafe fn show_balloon_with_flags(
        hwnd: HWND,
        title: &str,
        message: &str,
        flags: windows::Win32::UI::Shell::NOTIFY_ICON_INFOTIP_FLAGS,
    ) -> anyhow::Result<()> {
        let icon = tray_state(hwnd)
            .map(|state| state.icon)
            .unwrap_or_else(|| unsafe { LoadIconW(None, IDI_APPLICATION).unwrap_or_default() });
        let mut data = icon_data(hwnd, icon, NIF_INFO);
        copy_fixed_wide(&mut data.szInfoTitle, title);
        copy_fixed_wide(&mut data.szInfo, message);
        data.dwInfoFlags = flags;
        unsafe {
            if !Shell_NotifyIconW(NIM_MODIFY, &data).as_bool() {
                return Err(windows::core::Error::from_thread())
                    .context("failed to show notification-area balloon");
            }
        }
        Ok(())
    }

    fn icon_data(
        hwnd: HWND,
        icon: HICON,
        flags: windows::Win32::UI::Shell::NOTIFY_ICON_DATA_FLAGS,
    ) -> NOTIFYICONDATAW {
        let mut data = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: TRAY_UID,
            uFlags: flags,
            uCallbackMessage: WM_TRAYICON,
            ..Default::default()
        };
        data.hIcon = icon;
        copy_fixed_wide(&mut data.szTip, "winctl-mcp");
        data
    }

    fn load_tray_icon() -> (HICON, bool) {
        for candidate in icon_candidates() {
            if candidate.is_file() {
                let mut wide = candidate.as_os_str().encode_wide().collect::<Vec<_>>();
                wide.push(0);
                let handle = unsafe {
                    LoadImageW(
                        None,
                        PCWSTR(wide.as_ptr()),
                        IMAGE_ICON,
                        0,
                        0,
                        LR_LOADFROMFILE | LR_DEFAULTSIZE,
                    )
                };
                if let Ok(handle) = handle {
                    return (HICON(handle.0), true);
                }
            }
        }
        (
            unsafe { LoadIconW(None, IDI_APPLICATION).unwrap_or_default() },
            false,
        )
    }

    fn copy_fixed_wide<const N: usize>(dest: &mut [u16; N], text: &str) {
        let wide = wide(text);
        let len = wide.len().min(N.saturating_sub(1));
        dest[..len].copy_from_slice(&wide[..len]);
        if len < N {
            dest[len] = 0;
        }
    }

    fn wide(text: &str) -> Vec<u16> {
        OsStr::new(text).encode_wide().collect()
    }

    fn wide_z(text: &str) -> Vec<u16> {
        let mut value = wide(text);
        value.push(0);
        value
    }
}

fn read_pid(path: &PathBuf) -> anyhow::Result<u32> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read pid file {}", path.display()))?;
    text.trim()
        .parse()
        .with_context(|| format!("invalid pid file {}", path.display()))
}

fn is_server_process_alive(pid: u32, server_exe: &PathBuf) -> bool {
    #[cfg(windows)]
    {
        let Some(process) = winctl::describe_process(pid).ok().flatten() else {
            return false;
        };
        let Some(actual_path) = process.exe_path else {
            return false;
        };
        let expected = normalize_windows_path(&server_exe.to_string_lossy());
        let actual = normalize_windows_path(&actual_path);
        actual.eq_ignore_ascii_case(&expected)
    }
    #[cfg(not(windows))]
    {
        let _ = server_exe;
        PathBuf::from(format!("/proc/{pid}")).exists()
    }
}

#[cfg(windows)]
fn normalize_windows_path(path: &str) -> String {
    let normalized = path.trim_matches('"').replace('/', "\\");
    normalized
        .strip_prefix("\\\\?\\")
        .or_else(|| normalized.strip_prefix("\\??\\"))
        .unwrap_or(&normalized)
        .to_string()
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
        assert!(cli.config.listen_overridden);
    }

    #[test]
    fn default_launcher_listens_for_wsl_with_token() {
        let cli = Cli::parse([
            OsString::from("open-dashboard"),
            OsString::from("--auth-token"),
            OsString::from("test-token"),
        ])
        .unwrap();

        assert_eq!(cli.config.listen.to_string(), "0.0.0.0:8765");
        assert_eq!(
            cli.config.listener_probe_addr().to_string(),
            "127.0.0.1:8765"
        );
        assert!(!cli.config.mcp_url().contains("0.0.0.0"));
        assert_eq!(
            cli.config.dashboard_url(),
            "http://127.0.0.1:8765/dashboard?token=test-token"
        );
        assert_eq!(
            cli.config.recorder_url(),
            "http://127.0.0.1:8765/dashboard?token=test-token&tab=recorder"
        );
    }

    #[test]
    fn parses_run_tray_command() {
        let path = std::env::temp_dir().join(format!(
            "winctl-tray-run-config-{}.toml",
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        fs::write(
            &path,
            r#"
[transport]
mode = "http"
listen = "127.0.0.1:8765"
"#,
        )
        .unwrap();

        let cli = Cli::parse([
            OsString::from("run"),
            OsString::from("--server-exe"),
            OsString::from("C:/winctl/winctl-mcp-server.exe"),
            OsString::from("--config"),
            path.clone().into(),
        ])
        .unwrap();
        assert_eq!(cli.command, CommandMode::RunTray);
        assert_eq!(
            cli.config.server_exe,
            PathBuf::from("C:/winctl/winctl-mcp-server.exe")
        );
        assert_eq!(cli.config.config_file, Some(path.clone()));
        assert!(!cli.config.listen_overridden);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn parses_dashboard_window_url_override() {
        let cli = Cli::parse([
            OsString::from("dashboard-window"),
            OsString::from("--dashboard-url"),
            OsString::from("http://127.0.0.1:8765/dashboard?token=test"),
        ])
        .unwrap();

        assert_eq!(cli.command, CommandMode::DashboardWindow);
        assert_eq!(
            cli.config.dashboard_url(),
            "http://127.0.0.1:8765/dashboard?token=test"
        );
    }

    #[test]
    fn parses_recorder_shortcuts() {
        let cli = Cli::parse([
            OsString::from("recording-toggle"),
            OsString::from("--auth-token"),
            OsString::from("test-token"),
        ])
        .unwrap();
        assert_eq!(cli.command, CommandMode::RecordingToggle);
        assert_eq!(
            cli.config.recorder_url(),
            "http://127.0.0.1:8765/dashboard?token=test-token&tab=recorder"
        );
    }

    #[test]
    fn parses_start_menu_shortcut_commands() {
        let dashboard = Cli::parse([OsString::from("open-dashboard")]).unwrap();
        assert_eq!(dashboard.command, CommandMode::OpenDashboard);

        let recorder = Cli::parse([OsString::from("open-recorder")]).unwrap();
        assert_eq!(recorder.command, CommandMode::OpenRecorder);
    }

    #[test]
    fn redacts_dashboard_tokens_in_logs() {
        let sanitized =
            sanitize_log_message("failed at http://127.0.0.1:8765/dashboard?token=secret&x=1");
        assert_eq!(
            sanitized,
            "failed at http://127.0.0.1:8765/dashboard?token=<redacted>&x=1"
        );
    }

    #[test]
    fn status_url_redacts_dashboard_token() {
        let cli = Cli::parse([
            OsString::from("status"),
            OsString::from("--auth-token"),
            OsString::from("secret-token"),
        ])
        .unwrap();

        assert_eq!(
            cli.config.dashboard_status_url(),
            "http://127.0.0.1:8765/dashboard?token=<redacted>"
        );
        assert!(cli.config.dashboard_url().contains("secret-token"));
    }

    #[test]
    fn dashboard_state_version_check_requires_current_server() {
        let current = serde_json::json!({
            "ok": true,
            "service": "winctl-mcp-server",
            "version": "9.9.9"
        });
        assert!(dashboard_state_matches_version(&current, "9.9.9").unwrap());
        assert!(!dashboard_state_matches_version(&current, "9.9.8").unwrap());

        let other_service = serde_json::json!({
            "ok": true,
            "service": "other",
            "version": "9.9.9"
        });
        assert!(dashboard_state_matches_version(&other_service, "9.9.9").is_err());
    }

    #[test]
    fn config_file_sets_tray_listen_default() {
        let path = std::env::temp_dir().join(format!(
            "winctl-tray-config-{}.toml",
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        fs::write(
            &path,
            r#"
[transport]
mode = "http"
listen = "172.26.16.1:8765"
"#,
        )
        .unwrap();

        let cli = Cli::parse([
            OsString::from("run"),
            OsString::from("--config"),
            path.clone().into(),
        ])
        .unwrap();

        assert_eq!(cli.config.listen.to_string(), "172.26.16.1:8765");
        assert!(!cli.config.listen_overridden);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn config_file_sets_dashboard_auth_token() {
        let path = std::env::temp_dir().join(format!(
            "winctl-tray-auth-config-{}.toml",
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        fs::write(
            &path,
            r#"
[transport]
mode = "http"
listen = "172.26.16.1:8765"

[auth]
token = "s e+cret?"
"#,
        )
        .unwrap();

        let cli = Cli::parse([
            OsString::from("run"),
            OsString::from("--config"),
            path.clone().into(),
        ])
        .unwrap();

        assert_eq!(
            cli.config.dashboard_url(),
            "http://172.26.16.1:8765/dashboard?token=s%20e%2Bcret%3F"
        );
        assert_eq!(
            cli.config.recorder_url(),
            "http://172.26.16.1:8765/dashboard?token=s%20e%2Bcret%3F&tab=recorder"
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn closed_listener_is_not_reported_as_accepting() {
        let addr: SocketAddr = "127.0.0.1:9".parse().unwrap();
        assert!(!server_listener_accepts(addr));
    }
}
