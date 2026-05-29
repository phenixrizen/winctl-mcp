# Tray Controller

[Back to tool index](INDEX.md)

`winctl-tray` is the controller foundation for a future native Windows system tray app. The MCP server remains fully usable headless; the controller is optional.

## Commands

```powershell
winctl-tray.exe status
winctl-tray.exe start --server-exe C:\winctl\winctl-mcp-server.exe --listen 127.0.0.1:8765
winctl-tray.exe stop
winctl-tray.exe restart
winctl-tray.exe copy-mcp-url
winctl-tray.exe open-dashboard
```

## State

The controller writes a PID file under `%LOCALAPPDATA%\winctl-mcp` by default. Use `--pid-file <path>` to override it.

## Native Tray UI

The current crate provides start, stop, restart, status, URL, and dashboard actions. Native Windows tray icon integration will build on this controller without moving low-level automation logic out of the `winctl` crate or MCP tool logic out of `winctl-mcp-server`.
