# Tray Controller

[Back to tool index](INDEX.md)

`winctl-tray` is the optional native Windows notification-area controller. The MCP server remains fully usable headless; the tray app starts, stops, restarts, and opens the local dashboard for the server.

Release packages also include `winctl-launcher.exe`, a no-console Start Menu launcher that forwards commands to `winctl-tray.exe`. Use `winctl-tray.exe` directly for command-line output such as `status`.

## Commands

```powershell
winctl-tray.exe status
winctl-tray.exe start --server-exe C:\winctl\winctl-mcp-server.exe --listen 127.0.0.1:8765
winctl-tray.exe stop
winctl-tray.exe restart
winctl-tray.exe copy-mcp-url
winctl-tray.exe open-dashboard
winctl-tray.exe open-recorder
winctl-tray.exe recording-toggle
winctl-tray.exe run
```

## State

The controller writes a PID file under `%LOCALAPPDATA%\winctl-mcp` by default. Use `--pid-file <path>` to override it.

## Native Tray UI

Run the tray UI from Windows:

```powershell
$base = "$env:LOCALAPPDATA\winctl-mcp"
Start-Process "$base\bin\winctl-tray.exe" -ArgumentList @(
  "run",
  "--server-exe", "$base\bin\winctl-mcp-server.exe",
  "--config", "$base\config.toml"
)
```

The tray icon starts the server if needed. `open-dashboard` and `open-recorder` also start the server when the listener is not already accepting connections, which lets Start Menu shortcuts route through the controller rather than launching the server directly. Right-click the icon for Open Dashboard, Open Recorder, Copy MCP URL, Recording Toggle, Start, Stop, Restart, and Quit. Double-click opens the dashboard.

The MSI installer creates these Start Menu shortcuts:

| Shortcut | Command | Behavior |
| --- | --- | --- |
| `winctl Control` | `winctl-launcher.exe run` | Starts the MCP server if needed and keeps the tray control app running without opening a console window. |
| `winctl Dashboard` | `winctl-launcher.exe open-dashboard` | Starts the MCP server if needed and opens the native dashboard window without opening a console window. |
| `winctl Recorder` | `winctl-launcher.exe open-recorder` | Starts the MCP server if needed and opens the native recorder window without opening a console window. |

Open Dashboard launches a native WebView2 window through `wry`; it does not open the system browser. The WebView dashboard is enabled in the Windows MSVC build used for release packages and stores its WebView2 profile under `%LOCALAPPDATA%\winctl-mcp\webview`, not under `Program Files`. When the server config contains `[auth].token`, the tray reads that token and opens the dashboard with an authenticated local URL. MCP traffic still uses bearer-token auth on the `/mcp` endpoint.

While running, the tray polls the loopback dashboard state and shows native notification-area alerts when macro/test runs complete as passed, failed, or aborted.

If a no-console launch fails before a window is visible, diagnostics are appended to `%LOCALAPPDATA%\winctl-mcp\tray.log` or `%LOCALAPPDATA%\winctl-mcp\launcher.log`. Token query parameters are redacted from these logs.
