# Tray Controller

[Back to tool index](INDEX.md)

`winctl-tray` is the optional native Windows notification-area controller. The MCP server remains fully usable headless; the tray app starts, stops, restarts, and opens the local dashboard for the server.

## Commands

```powershell
winctl-tray.exe status
winctl-tray.exe start --server-exe C:\winctl\winctl-mcp-server.exe --listen 127.0.0.1:8765
winctl-tray.exe stop
winctl-tray.exe restart
winctl-tray.exe copy-mcp-url
winctl-tray.exe open-dashboard
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

The tray icon starts the server if needed. Right-click the icon for Open Dashboard, Copy MCP URL, Start, Stop, Restart, and Quit. Double-click opens the dashboard.
