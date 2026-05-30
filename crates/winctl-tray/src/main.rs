use std::ffi::OsString;
use std::fs;
#[cfg(windows)]
use std::io::Write;
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::Context;
use serde::{Deserialize, Serialize};

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
    RunTray,
}

#[derive(Debug, Clone)]
struct TrayConfig {
    server_exe: PathBuf,
    listen: SocketAddr,
    listen_overridden: bool,
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
        let data_dir = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("winctl-mcp");
        let server_exe = std::env::current_exe()
            .ok()
            .and_then(|path| {
                path.parent()
                    .map(|parent| parent.join("winctl-mcp-server.exe"))
            })
            .unwrap_or_else(|| PathBuf::from("winctl-mcp-server.exe"));
        Self {
            server_exe,
            listen: "127.0.0.1:8765".parse().expect("default listen is valid"),
            listen_overridden: false,
            log_file: Some(data_dir.join("server.log")),
            log_file_overridden: false,
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

    fn apply_config_file_defaults(&mut self) -> anyhow::Result<()> {
        let Some(config_file) = &self.config_file else {
            return Ok(());
        };
        if self.listen_overridden {
            return Ok(());
        }
        let text = fs::read_to_string(config_file)
            .with_context(|| format!("failed to read config file {}", config_file.display()))?;
        let config: TrayServerConfigFile = toml::from_str(&text)
            .with_context(|| format!("failed to parse config file {}", config_file.display()))?;
        if let Some(listen) = config.transport.and_then(|transport| transport.listen) {
            self.listen = listen
                .parse()
                .with_context(|| format!("invalid configured listen address {listen}"))?;
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct TrayServerConfigFile {
    transport: Option<TrayTransportConfig>,
}

#[derive(Debug, Deserialize)]
struct TrayTransportConfig {
    listen: Option<String>,
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
                    config.listen_overridden = true;
                }
                "--log-file" => {
                    index += 1;
                    config.log_file = Some(required_path(&args, index, "--log-file")?);
                    config.log_file_overridden = true;
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
        config.apply_config_file_defaults()?;
        Ok(Self { command, config })
    }
}

fn start_server(config: &TrayConfig) -> anyhow::Result<()> {
    if let Some(pid) = server_pid(config) {
        println!("winctl-mcp-server already running pid {pid}");
        return Ok(());
    }
    if server_listener_accepts(config.listen) {
        println!("winctl-mcp-server already listening on {}", config.listen);
        return Ok(());
    }

    if let Some(parent) = config.pid_file.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let mut command = Command::new(&config.server_exe);
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
    let pid = server_pid(config);
    let running = pid.is_some();
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
        .filter(|pid| is_process_alive(*pid))
}

fn server_listener_accepts(listen: SocketAddr) -> bool {
    TcpStream::connect_timeout(&listen, Duration::from_millis(200)).is_ok()
}

#[cfg(windows)]
mod windows_tray {
    use std::ffi::{c_void, OsStr};
    use std::mem::size_of;
    use std::os::windows::ffi::OsStrExt;

    use anyhow::Context;
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Shell::{
        Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_ERROR, NIIF_INFO,
        NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyIcon, DestroyMenu,
        DestroyWindow, DispatchMessageW, GetCursorPos, GetMessageW, GetWindowLongPtrW, LoadCursorW,
        LoadIconW, LoadImageW, PostQuitMessage, RegisterClassW, SetForegroundWindow,
        SetWindowLongPtrW, TrackPopupMenu, TranslateMessage, CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW,
        CW_USEDEFAULT, GWLP_USERDATA, HICON, IDC_ARROW, IDI_APPLICATION, IMAGE_ICON,
        LR_DEFAULTSIZE, LR_LOADFROMFILE, MF_ENABLED, MF_GRAYED, MF_SEPARATOR, MF_STRING, MSG,
        TPM_RETURNCMD, TPM_RIGHTBUTTON, WINDOW_EX_STYLE, WM_APP, WM_COMMAND, WM_DESTROY,
        WM_LBUTTONDBLCLK, WM_NCCREATE, WM_RBUTTONUP, WNDCLASSW, WS_OVERLAPPED,
    };

    use super::{
        copy_mcp_url_to_clipboard, open_dashboard, server_pid, start_server, stop_server,
        TrayConfig,
    };

    const TRAY_UID: u32 = 1;
    const WM_TRAYICON: u32 = WM_APP + 1;
    const MENU_OPEN_DASHBOARD: u32 = 1001;
    const MENU_START_SERVER: u32 = 1002;
    const MENU_STOP_SERVER: u32 = 1003;
    const MENU_RESTART_SERVER: u32 = 1004;
    const MENU_COPY_MCP_URL: u32 = 1005;
    const MENU_QUIT: u32 = 1006;

    struct TrayState {
        config: TrayConfig,
        icon: HICON,
        icon_needs_destroy: bool,
    }

    pub(super) fn run(config: TrayConfig) -> anyhow::Result<()> {
        if server_pid(&config).is_none() {
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
            WM_DESTROY => {
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
            append_menu_item(menu, MENU_COPY_MCP_URL, "Copy MCP URL", true)?;
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

    fn icon_candidates() -> Vec<std::path::PathBuf> {
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
        candidates.push(std::path::PathBuf::from("assets/brand/winctl.ico"));
        candidates
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
        assert!(cli.config.listen_overridden);
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
    fn closed_listener_is_not_reported_as_accepting() {
        let addr: SocketAddr = "127.0.0.1:9".parse().unwrap();
        assert!(!server_listener_accepts(addr));
    }
}
