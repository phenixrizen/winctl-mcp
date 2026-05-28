# Winctl MCP Tool Index

| Agent name | Tool name | Description |
| --- | --- | --- |
| `winctl-cmp` | [`server.ping`](SERVER_PING.md) | Return a minimal health response without touching Win32 APIs. |
| `winctl-cmp` | [`windows.list`](WINDOWS_LIST.md) | List visible and discoverable top-level Windows windows with HWND, PID, executable, class, title, and virtual desktop geometry. |
| `winctl-cmp` | [`windows.find`](WINDOWS_FIND.md) | Find windows matching a selector and return scored diagnostics without binding or controlling them. |
| `winctl-cmp` | [`windows.bind`](WINDOWS_BIND.md) | Bind one strict window target by stable identity before any control action. |
| `winctl-cmp` | [`windows.describe`](WINDOWS_DESCRIBE.md) | Describe an existing bound window record by `bound_id`. |
| `winctl-cmp` | [`windows.focus`](WINDOWS_FOCUS.md) | Focus a previously bound window after revalidating HWND, PID, and executable identity. |
| `winctl-cmp` | [`windows.window_from_point`](WINDOWS_WINDOW_FROM_POINT.md) | Resolve a screen point to top-level and child window diagnostics, optionally relative to a bound target. |
| `winctl-cmp` | [`windows.monitors`](WINDOWS_MONITORS.md) | List monitor geometry, DPI scale, primary monitor flag, and total virtual desktop bounds. |
| `winctl-cmp` | [`windows.wait_for_window`](WINDOWS_WAIT_FOR_WINDOW.md) | Wait for visible top-level window candidates owned by a PID or MCP launch ID without title-only selection. |
| `winctl-cmp` | [`process.launch`](PROCESS_LAUNCH.md) | Launch a Windows executable via `CreateProcessW` and optionally wait for visible PID-owned window candidates. |
| `winctl-cmp` | [`process.list`](PROCESS_LIST.md) | List Windows process metadata, with optional top-level window candidates and MCP-launched process markers. |
| `winctl-cmp` | [`process.describe`](PROCESS_DESCRIBE.md) | Describe one process with executable metadata, tracked launch status, children, and top-level windows. |
| `winctl-cmp` | [`process.kill`](PROCESS_KILL.md) | Terminate only a process launched and tracked by this MCP server session. |
| `winctl-cmp` | [`input.click`](INPUT_CLICK.md) | Click a bound window coordinate after identity revalidation and window-from-point preflight. |
| `winctl-cmp` | [`input.type_text`](INPUT_TYPE_TEXT.md) | Focus a bound window and type Unicode text with `SendInput` after identity revalidation. |
| `winctl-cmp` | [`capture.screenshot_window`](CAPTURE_SCREENSHOT_WINDOW.md) | Capture a screenshot of a bound window and return exact virtual desktop region metadata. |
| `winctl-cmp` | [`capture.screenshot_display`](CAPTURE_SCREENSHOT_DISPLAY.md) | Capture a screenshot of a display by zero-based monitor index and return exact virtual desktop region metadata. |
