# Winctl MCP Tool Index

| Agent name | Tool name | Description |
| --- | --- | --- |
| `winctl-cmp` | [`server.ping`](SERVER_PING.md) | Return a minimal health response without touching Win32 APIs. |
| `winctl-cmp` | [`memory.remember`](MEMORY_REMEMBER.md) | Explicitly store a structured memory item with searchable text, tags, app/target identity, and sqlite-vec embedding metadata. |
| `winctl-cmp` | [`memory.search`](MEMORY_SEARCH.md) | Search remembered procedures, observations, macros, and recipes using hybrid sqlite-vec, FTS5, tag, identity, recency, and usefulness ranking. |
| `winctl-cmp` | [`memory.get`](MEMORY_GET.md) | Fetch one memory item by ID and update its explicit use metadata. |
| `winctl-cmp` | [`memory.update`](MEMORY_UPDATE.md) | Explicitly update a remembered item and rebuild its FTS5 and sqlite-vec indexes. |
| `winctl-cmp` | [`memory.delete`](MEMORY_DELETE.md) | Explicitly delete one remembered item by ID and remove it from memory indexes. |
| `winctl-cmp` | [`memory.list`](MEMORY_LIST.md) | List remembered items with optional kind and tag filtering. |
| `winctl-cmp` | [`memory.reindex`](MEMORY_REINDEX.md) | Rebuild memory FTS5 and sqlite-vec indexes for the local memory database. |
| `winctl-cmp` | [`uia.snapshot`](UIA_SNAPSHOT.md) | Capture a UI Automation tree for a bound window with element roles, names, automation IDs, bounds, state, hierarchy, and stable element references. |
| `winctl-cmp` | [`uia.find`](UIA_FIND.md) | Find UI Automation elements in a fresh bound-window snapshot by semantic selector fields. |
| `winctl-cmp` | [`uia.resolve`](UIA_RESOLVE.md) | Revalidate a UI Automation element reference path against the current bound window snapshot. |
| `winctl-cmp` | [`windows.list`](WINDOWS_LIST.md) | List visible and discoverable top-level Windows windows with HWND, PID, executable, class, title, and virtual desktop geometry. |
| `winctl-cmp` | [`windows.find`](WINDOWS_FIND.md) | Find windows matching a selector and return scored diagnostics without binding or controlling them. |
| `winctl-cmp` | [`windows.bind`](WINDOWS_BIND.md) | Bind one strict window target by stable identity before any control action. |
| `winctl-cmp` | [`windows.describe`](WINDOWS_DESCRIBE.md) | Describe an existing bound window record by `bound_id`. |
| `winctl-cmp` | [`windows.focus`](WINDOWS_FOCUS.md) | Focus a previously bound window after revalidating HWND, PID, and executable identity. |
| `winctl-cmp` | [`windows.window_from_point`](WINDOWS_WINDOW_FROM_POINT.md) | Resolve a screen point to top-level and child window diagnostics, optionally relative to a bound target. |
| `winctl-cmp` | [`windows.monitors`](WINDOWS_MONITORS.md) | List monitor geometry, DPI scale, primary monitor flag, and total virtual desktop bounds. |
| `winctl-cmp` | [`windows.wait_for_state`](WINDOWS_WAIT_FOR_STATE.md) | Wait for a bound window to satisfy state, foreground, title, or class conditions after identity revalidation. |
| `winctl-cmp` | [`windows.wait_for_window`](WINDOWS_WAIT_FOR_WINDOW.md) | Wait for visible top-level window candidates owned by a PID or MCP launch ID without title-only selection. |
| `winctl-cmp` | [`process.launch`](PROCESS_LAUNCH.md) | Launch a Windows executable via `CreateProcessW` and optionally wait for visible PID-owned window candidates. |
| `winctl-cmp` | [`process.list`](PROCESS_LIST.md) | List Windows process metadata, with optional top-level window candidates and MCP-launched process markers. |
| `winctl-cmp` | [`process.describe`](PROCESS_DESCRIBE.md) | Describe one process with executable metadata, tracked launch status, children, and top-level windows. |
| `winctl-cmp` | [`process.kill`](PROCESS_KILL.md) | Terminate only a process launched and tracked by this MCP server session. |
| `winctl-cmp` | [`process.wait_for_exit`](PROCESS_WAIT_FOR_EXIT.md) | Wait for a process identified by PID or MCP launch ID to exit and return lifecycle timing metadata. |
| `winctl-cmp` | [`input.click`](INPUT_CLICK.md) | Click a bound window coordinate after identity revalidation and window-from-point preflight. |
| `winctl-cmp` | [`input.mouse_move`](INPUT_MOUSE_MOVE.md) | Move the mouse to a bound window coordinate after identity revalidation and point preflight. |
| `winctl-cmp` | [`input.double_click`](INPUT_DOUBLE_CLICK.md) | Double-click a bound window coordinate after identity revalidation and point preflight. |
| `winctl-cmp` | [`input.drag`](INPUT_DRAG.md) | Drag between two bound window coordinates after identity revalidation and point preflight. |
| `winctl-cmp` | [`input.scroll`](INPUT_SCROLL.md) | Scroll at a bound window coordinate after identity revalidation and point preflight. |
| `winctl-cmp` | [`input.key_down`](INPUT_KEY_DOWN.md) | Focus a bound window and dispatch a virtual key-down event after identity revalidation. |
| `winctl-cmp` | [`input.key_up`](INPUT_KEY_UP.md) | Focus a bound window and dispatch a virtual key-up event after identity revalidation. |
| `winctl-cmp` | [`input.shortcut`](INPUT_SHORTCUT.md) | Focus a bound window and dispatch a virtual-key shortcut after identity revalidation. |
| `winctl-cmp` | [`input.delay`](INPUT_DELAY.md) | Wait for a bounded number of milliseconds and return timing metadata for replay manifests. |
| `winctl-cmp` | [`input.type_text`](INPUT_TYPE_TEXT.md) | Focus a bound window and type Unicode text with `SendInput` after identity revalidation. |
| `winctl-cmp` | [`macro.validate`](MACRO_VALIDATE.md) | Validate a winctl macro manifest version, tool names, target identity requirements, and coordinate fallback metadata. |
| `winctl-cmp` | [`macro.dry_run`](MACRO_DRY_RUN.md) | Build a dry-run plan for a macro manifest without performing mutating UI actions. |
| `winctl-cmp` | [`macro.run`](MACRO_RUN.md) | Execute a macro manifest through the existing MCP tool implementations with target revalidation before control actions. |
| `winctl-cmp` | [`macro.run_step`](MACRO_RUN_STEP.md) | Execute one macro step by ID for stepwise debugging. |
| `winctl-cmp` | [`macro.abort`](MACRO_ABORT.md) | Request a safe abort for an active macro run. |
| `winctl-cmp` | [`macro.list`](MACRO_LIST.md) | List session-promoted macros and memory-backed macro items. |
| `winctl-cmp` | [`macro.get`](MACRO_GET.md) | Get a promoted macro manifest by session macro ID or memory item ID. |
| `winctl-cmp` | [`macro.promote`](MACRO_PROMOTE.md) | Promote an approved macro manifest into the session registry and optionally explicit memory storage. |
| `winctl-cmp` | [`macro.export_result`](MACRO_EXPORT_RESULT.md) | Export a structured macro run result and artifact metadata by run ID. |
| `winctl-cmp` | [`capture.screenshot_window`](CAPTURE_SCREENSHOT_WINDOW.md) | Capture a screenshot of a bound window and return exact virtual desktop region metadata. |
| `winctl-cmp` | [`capture.screenshot_display`](CAPTURE_SCREENSHOT_DISPLAY.md) | Capture a screenshot of a display by zero-based monitor index and return exact virtual desktop region metadata. |
| `winctl-cmp` | [`capture.wait_for_window_image_change`](CAPTURE_WAIT_FOR_WINDOW_IMAGE_CHANGE.md) | Poll bound-window screenshots until the image bytes change, returning replay-safe capture diagnostics. |

## Related Docs

- [Macro manifest](MACRO_MANIFEST.md)
