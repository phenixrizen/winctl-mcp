# Winctl MCP Tool Index

| Agent name | Tool name | Description |
| --- | --- | --- |
| `winctl-mcp` | [`app.launch`](APP_LAUNCH.md) | Launch an executable, protocol handler, packaged app, or Start Menu app target without shell command concatenation. |
| `winctl-mcp` | [`artifact.export`](ARTIFACT_EXPORT.md) | Export a captured artifact to the capture export directory or an allowlisted destination. |
| `winctl-mcp` | [`assert.element`](ASSERT_ELEMENT.md) | Assert UI Automation element existence, enabled state, and name conditions. |
| `winctl-mcp` | [`assert.text_visible`](ASSERT_TEXT_VISIBLE.md) | Assert text is visible through window metadata or the UI Automation tree. |
| `winctl-mcp` | [`assert.pixel_color`](ASSERT_PIXEL_COLOR.md) | Sample an image or bound-window screenshot pixel and optionally assert expected RGB. |
| `winctl-mcp` | [`assert.window_count`](ASSERT_WINDOW_COUNT.md) | Assert the number of current windows matching a selector. |
| `winctl-mcp` | [`assert.clipboard`](ASSERT_CLIPBOARD.md) | Assert current clipboard text equals or contains expected text. |
| `winctl-mcp` | [`browser.list`](BROWSER_LIST.md) | List Chrome, Edge, and Firefox process/window state with explicit PID/HWND identity metadata. |
| `winctl-mcp` | [`browser.describe`](BROWSER_DESCRIBE.md) | Describe one browser target by bound window, PID, or HWND without tab-title selection. |
| `winctl-mcp` | [`browser.wait_for_navigation`](BROWSER_WAIT_FOR_NAVIGATION.md) | Wait for a bound browser window title transition while revalidating browser PID/HWND/executable identity. |
| `winctl-mcp` | [`browser.assert`](BROWSER_ASSERT.md) | Assert browser kind and window title/class conditions against a revalidated bound browser window. |
| `winctl-mcp` | [`browser.extract_content`](BROWSER_EXTRACT_CONTENT.md) | Return safe browser window content hints and identity metadata for a revalidated bound browser window. |
| `winctl-mcp` | [`browser.screenshot_checkpoint`](BROWSER_SCREENSHOT_CHECKPOINT.md) | Capture a screenshot checkpoint for a revalidated bound browser window. |
| `winctl-mcp` | [`build.run`](BUILD_RUN.md) | Run an allowlisted build tool directly and return structured output diagnostics. |
| `winctl-mcp` | [`clipboard.read`](CLIPBOARD_READ.md) | Read Unicode clipboard text with optional truncation. |
| `winctl-mcp` | [`clipboard.write`](CLIPBOARD_WRITE.md) | Write Unicode clipboard text only when clipboard mutation is explicitly enabled. |
| `winctl-mcp` | [`control.state`](CONTROL_STATE.md) | Return desktop-control gate state, active target identity, consent decision, and recent control events. |
| `winctl-mcp` | [`control.arm`](CONTROL_ARM.md) | Arm desktop control for a session or bound target before sensitive actions. |
| `winctl-mcp` | [`control.consent`](CONTROL_CONSENT.md) | Record an allow_once, allow_session, deny, or revoke_session decision for desktop-control actions. |
| `winctl-mcp` | [`control.notify`](CONTROL_NOTIFY.md) | Record a pending desktop-control notification event for tray or dashboard display. |
| `winctl-mcp` | [`control.revoke`](CONTROL_REVOKE.md) | Emergency-stop desktop control and reject future sensitive actions until rearmed. |
| `winctl-mcp` | [`control.emergency_stop`](CONTROL_EMERGENCY_STOP.md) | Alias for control.revoke. |
| `winctl-mcp` | [`diagnostics.crash_report`](DIAGNOSTICS_CRASH_REPORT.md) | Collect process, window, screenshot, and platform diagnostic context. |
| `winctl-mcp` | [`filesystem.read`](FILESYSTEM_READ.md) | Read a UTF-8 file from an allowlisted filesystem root with bounded size. |
| `winctl-mcp` | [`filesystem.list`](FILESYSTEM_LIST.md) | List files and directories beneath an allowlisted filesystem root. |
| `winctl-mcp` | [`filesystem.search`](FILESYSTEM_SEARCH.md) | Search file names and bounded UTF-8 file content beneath an allowlisted filesystem root. |
| `winctl-mcp` | [`filesystem.copy`](FILESYSTEM_COPY.md) | Copy a file within allowlisted roots only when filesystem mutation is explicitly enabled. |
| `winctl-mcp` | [`filesystem.move`](FILESYSTEM_MOVE.md) | Move a file within allowlisted roots only when filesystem mutation is explicitly enabled. |
| `winctl-mcp` | [`filesystem.delete`](FILESYSTEM_DELETE.md) | Delete an allowlisted filesystem path only when filesystem mutation is explicitly enabled. |
| `winctl-mcp` | [`network.fetch`](NETWORK_FETCH.md) | Fetch an HTTP/HTTPS URL with timeout, response-size, redirect, and private-network guards. |
| `winctl-mcp` | [`network.scrape`](NETWORK_SCRAPE.md) | Fetch and extract basic title, link, and text content from an HTTP/HTTPS page under network policy. |
| `winctl-mcp` | [`notifications.list`](NOTIFICATIONS_LIST.md) | Return Windows notification inspection status and any available provider-backed notifications. |
| `winctl-mcp` | [`registry.list`](REGISTRY_LIST.md) | List Windows registry subkeys and optional values from a selected hive. |
| `winctl-mcp` | [`registry.read`](REGISTRY_READ.md) | Read a Windows registry value from a selected hive. |
| `winctl-mcp` | [`registry.write`](REGISTRY_WRITE.md) | Write a Windows registry value only when registry mutation is explicitly enabled. |
| `winctl-mcp` | [`registry.delete`](REGISTRY_DELETE.md) | Delete a Windows registry value only when registry mutation is explicitly enabled. |
| `winctl-mcp` | [`server.config`](SERVER_CONFIG.md) | Return effective runtime configuration and security policy diagnostics. |
| `winctl-mcp` | [`server.ping`](SERVER_PING.md) | Return a minimal health response without touching Win32 APIs. |
| `winctl-mcp` | [`test.validate`](TEST_VALIDATE.md) | Validate a winctl test manifest and its aligned macro manifest. |
| `winctl-mcp` | [`test.dry_run`](TEST_DRY_RUN.md) | Build a dry-run plan for a winctl test manifest without mutating UI state. |
| `winctl-mcp` | [`test.run`](TEST_RUN.md) | Run a winctl test manifest through the macro execution engine. |
| `winctl-mcp` | [`test.export_result`](TEST_EXPORT_RESULT.md) | Export a test run result by run ID. |
| `winctl-mcp` | [`test.report_export`](TEST_REPORT_EXPORT.md) | Export a test or macro run result as JSON, JUnit XML, or HTML. |
| `winctl-mcp` | [`memory.remember`](MEMORY_REMEMBER.md) | Explicitly store a structured memory item with searchable text, tags, app/target identity, and sqlite-vec embedding metadata. |
| `winctl-mcp` | [`memory.search`](MEMORY_SEARCH.md) | Search remembered procedures, observations, macros, and recipes using hybrid sqlite-vec, FTS5, tag, identity, recency, and usefulness ranking. |
| `winctl-mcp` | [`memory.get`](MEMORY_GET.md) | Fetch one memory item by ID and update its explicit use metadata. |
| `winctl-mcp` | [`memory.update`](MEMORY_UPDATE.md) | Explicitly update a remembered item and rebuild its FTS5 and sqlite-vec indexes. |
| `winctl-mcp` | [`memory.delete`](MEMORY_DELETE.md) | Explicitly delete one remembered item by ID and remove it from memory indexes. |
| `winctl-mcp` | [`memory.list`](MEMORY_LIST.md) | List remembered items with optional kind and tag filtering. |
| `winctl-mcp` | [`memory.reindex`](MEMORY_REINDEX.md) | Rebuild memory FTS5 and sqlite-vec indexes for the local memory database. |
| `winctl-mcp` | [`uia.snapshot`](UIA_SNAPSHOT.md) | Capture a UI Automation tree for a bound window with element roles, names, automation IDs, bounds, state, hierarchy, and stable element references. |
| `winctl-mcp` | [`uia.find`](UIA_FIND.md) | Find UI Automation elements in a fresh bound-window snapshot by semantic selector fields. |
| `winctl-mcp` | [`uia.resolve`](UIA_RESOLVE.md) | Revalidate a UI Automation element reference path against the current bound window snapshot. |
| `winctl-mcp` | [`uia.invoke`](UIA_INVOKE.md) | Invoke a revalidated UI Automation element with safe fallback diagnostics. |
| `winctl-mcp` | [`uia.set_value`](UIA_SET_VALUE.md) | Set text for a revalidated UI Automation element with before/after state diagnostics. |
| `winctl-mcp` | [`uia.get_value`](UIA_GET_VALUE.md) | Read value-like UI Automation properties from a revalidated element snapshot. |
| `winctl-mcp` | [`uia.toggle`](UIA_TOGGLE.md) | Toggle a revalidated UI Automation element when safe fallback semantics are available. |
| `winctl-mcp` | [`uia.expand_collapse`](UIA_EXPAND_COLLAPSE.md) | Expand or collapse a revalidated UI Automation element when supported. |
| `winctl-mcp` | [`uia.select`](UIA_SELECT.md) | Select a revalidated UI Automation element with strict target resolution. |
| `winctl-mcp` | [`uia.set_focus`](UIA_SET_FOCUS.md) | Focus a revalidated UI Automation element with strict target resolution. |
| `winctl-mcp` | [`uia.range_value`](UIA_RANGE_VALUE.md) | Set a numeric value on a revalidated UI Automation range element when supported. |
| `winctl-mcp` | [`uia.scroll_into_view`](UIA_SCROLL_INTO_VIEW.md) | Scroll a revalidated UI Automation element into view when supported. |
| `winctl-mcp` | [`uia.wait_for_element`](UIA_WAIT_FOR_ELEMENT.md) | Wait for a UI Automation selector or element reference to resolve in fresh snapshots. |
| `winctl-mcp` | [`web.cdp.list_targets`](WEB_CDP_LIST_TARGETS.md) | List local Chrome DevTools Protocol targets from a debugger HTTP endpoint. |
| `winctl-mcp` | [`web.cdp.evaluate`](WEB_CDP_EVALUATE.md) | Prepare a CDP JavaScript evaluation request and return target diagnostics. |
| `winctl-mcp` | [`web.dom.snapshot`](WEB_DOM_SNAPSHOT.md) | Return CDP DOM snapshot provider diagnostics for a debugger target. |
| `winctl-mcp` | [`web.network.events`](WEB_NETWORK_EVENTS.md) | Return CDP network-event provider diagnostics for a debugger target. |
| `winctl-mcp` | [`web.a11y.snapshot`](WEB_A11Y_SNAPSHOT.md) | Return accessibility-tree provider diagnostics for a browser or WebView target. |
| `winctl-mcp` | [`web.style.inspect`](WEB_STYLE_INSPECT.md) | Return style-inspection provider diagnostics for a browser or WebView target. |
| `winctl-mcp` | [`windows.list`](WINDOWS_LIST.md) | List visible and discoverable top-level Windows windows with HWND, PID, executable, class, title, and virtual desktop geometry. |
| `winctl-mcp` | [`windows.find`](WINDOWS_FIND.md) | Find windows matching a selector and return scored diagnostics without binding or controlling them. |
| `winctl-mcp` | [`windows.bind`](WINDOWS_BIND.md) | Bind one strict window target by stable identity before any control action. |
| `winctl-mcp` | [`windows.describe`](WINDOWS_DESCRIBE.md) | Describe an existing bound window record by `bound_id`. |
| `winctl-mcp` | [`windows.focus`](WINDOWS_FOCUS.md) | Focus a previously bound window after revalidating HWND, PID, and executable identity. |
| `winctl-mcp` | [`windows.window_from_point`](WINDOWS_WINDOW_FROM_POINT.md) | Resolve a screen point to top-level and child window diagnostics, optionally relative to a bound target. |
| `winctl-mcp` | [`windows.monitors`](WINDOWS_MONITORS.md) | List monitor geometry, DPI scale, primary monitor flag, and total virtual desktop bounds. |
| `winctl-mcp` | [`windows.wait_for_state`](WINDOWS_WAIT_FOR_STATE.md) | Wait for a bound window to satisfy state, foreground, title, or class conditions after identity revalidation. |
| `winctl-mcp` | [`windows.wait_for_window`](WINDOWS_WAIT_FOR_WINDOW.md) | Wait for visible top-level window candidates owned by a PID or MCP launch ID without title-only selection. |
| `winctl-mcp` | [`windows.move`](WINDOWS_MOVE.md) | Move a bound window after revalidating HWND, PID, and executable identity. |
| `winctl-mcp` | [`windows.resize`](WINDOWS_RESIZE.md) | Resize a bound window after revalidating HWND, PID, and executable identity. |
| `winctl-mcp` | [`windows.minimize`](WINDOWS_MINIMIZE.md) | Minimize a bound window after revalidating stable identity. |
| `winctl-mcp` | [`windows.maximize`](WINDOWS_MAXIMIZE.md) | Maximize a bound window after revalidating stable identity. |
| `winctl-mcp` | [`windows.restore`](WINDOWS_RESTORE.md) | Restore a bound window after revalidating stable identity. |
| `winctl-mcp` | [`windows.close`](WINDOWS_CLOSE.md) | Post `WM_CLOSE` to a bound window after revalidating stable identity. |
| `winctl-mcp` | [`windows.foreground_diagnostics`](WINDOWS_FOREGROUND_DIAGNOSTICS.md) | Return foreground and replay diagnostics for a bound window after identity revalidation. |
| `winctl-mcp` | [`windows.for_process`](WINDOWS_FOR_PROCESS.md) | List visible top-level windows for a PID or MCP launch ID, with explicit child-process policy. |
| `winctl-mcp` | [`process.launch`](PROCESS_LAUNCH.md) | Launch a Windows executable via `CreateProcessW` and optionally wait for visible PID-owned window candidates. |
| `winctl-mcp` | [`process.list`](PROCESS_LIST.md) | List Windows process metadata, with optional top-level window candidates and MCP-launched process markers. |
| `winctl-mcp` | [`process.describe`](PROCESS_DESCRIBE.md) | Describe one process with executable metadata, tracked launch status, children, and top-level windows. |
| `winctl-mcp` | [`process.diagnostics`](PROCESS_DIAGNOSTICS.md) | Return additional process diagnostics with optional windows and child-process metadata. |
| `winctl-mcp` | [`process.kill`](PROCESS_KILL.md) | Terminate only a process launched and tracked by this MCP server session. |
| `winctl-mcp` | [`process.metrics`](PROCESS_METRICS.md) | Return process metric diagnostics and platform availability for resource counters. |
| `winctl-mcp` | [`process.wait_for_exit`](PROCESS_WAIT_FOR_EXIT.md) | Wait for a process identified by PID or MCP launch ID to exit and return lifecycle timing metadata. |
| `winctl-mcp` | [`recorder.start`](RECORDER_START.md) | Start a local recording session that can be exported as a macro manifest. |
| `winctl-mcp` | [`recorder.record_step`](RECORDER_RECORD_STEP.md) | Append one recorded MCP tool step with optional target, note, and replay metadata. |
| `winctl-mcp` | [`recorder.stop`](RECORDER_STOP.md) | Stop the active recording session and return its macro manifest. |
| `winctl-mcp` | [`recorder.export_manifest`](RECORDER_EXPORT_MANIFEST.md) | Export the active or completed recording session as a macro manifest. |
| `winctl-mcp` | [`recorder.state`](RECORDER_STATE.md) | Return active and completed local recording sessions. |
| `winctl-mcp` | [`input.click`](INPUT_CLICK.md) | Click a bound window coordinate after identity revalidation and window-from-point preflight. |
| `winctl-mcp` | [`input.mouse_move`](INPUT_MOUSE_MOVE.md) | Move the mouse to a bound window coordinate after identity revalidation and point preflight. |
| `winctl-mcp` | [`input.double_click`](INPUT_DOUBLE_CLICK.md) | Double-click a bound window coordinate after identity revalidation and point preflight. |
| `winctl-mcp` | [`input.drag`](INPUT_DRAG.md) | Drag between two bound window coordinates after identity revalidation and point preflight. |
| `winctl-mcp` | [`input.scroll`](INPUT_SCROLL.md) | Scroll at a bound window coordinate after identity revalidation and point preflight. |
| `winctl-mcp` | [`input.key_down`](INPUT_KEY_DOWN.md) | Focus a bound window and dispatch a virtual key-down event after identity revalidation. |
| `winctl-mcp` | [`input.key_up`](INPUT_KEY_UP.md) | Focus a bound window and dispatch a virtual key-up event after identity revalidation. |
| `winctl-mcp` | [`input.shortcut`](INPUT_SHORTCUT.md) | Focus a bound window and dispatch a virtual-key shortcut after identity revalidation. |
| `winctl-mcp` | [`input.delay`](INPUT_DELAY.md) | Wait for a bounded number of milliseconds and return timing metadata for replay manifests. |
| `winctl-mcp` | [`input.type_text`](INPUT_TYPE_TEXT.md) | Focus a bound window and type Unicode text with `SendInput` after identity revalidation. |
| `winctl-mcp` | [`macro.validate`](MACRO_VALIDATE.md) | Validate a winctl macro manifest version, tool names, target identity requirements, and coordinate fallback metadata. |
| `winctl-mcp` | [`macro.dry_run`](MACRO_DRY_RUN.md) | Build a dry-run plan for a macro manifest without performing mutating UI actions. |
| `winctl-mcp` | [`macro.run`](MACRO_RUN.md) | Execute a macro manifest through the existing MCP tool implementations with target revalidation before control actions. |
| `winctl-mcp` | [`macro.run_step`](MACRO_RUN_STEP.md) | Execute one macro step by ID for stepwise debugging. |
| `winctl-mcp` | [`macro.abort`](MACRO_ABORT.md) | Request a safe abort for an active macro run. |
| `winctl-mcp` | [`macro.list`](MACRO_LIST.md) | List session-promoted macros and memory-backed macro items. |
| `winctl-mcp` | [`macro.get`](MACRO_GET.md) | Get a promoted macro manifest by session macro ID or memory item ID. |
| `winctl-mcp` | [`macro.promote`](MACRO_PROMOTE.md) | Promote an approved macro manifest into the session registry and optionally explicit memory storage. |
| `winctl-mcp` | [`macro.export_result`](MACRO_EXPORT_RESULT.md) | Export a structured macro run result and artifact metadata by run ID. |
| `winctl-mcp` | [`capture.ocr_region`](CAPTURE_OCR_REGION.md) | Return OCR-region diagnostics for an image or bound window. |
| `winctl-mcp` | [`capture.read_text`](CAPTURE_READ_TEXT.md) | Extract readable text from a bound window using UI Automation. |
| `winctl-mcp` | [`capture.compare_baseline`](CAPTURE_COMPARE_BASELINE.md) | Compare an actual image against a baseline and write an optional diff artifact. |
| `winctl-mcp` | [`capture.screenshot_window`](CAPTURE_SCREENSHOT_WINDOW.md) | Capture a screenshot of a bound window and return exact virtual desktop region metadata. |
| `winctl-mcp` | [`capture.screenshot_display`](CAPTURE_SCREENSHOT_DISPLAY.md) | Capture a screenshot of a display by zero-based monitor index and return exact virtual desktop region metadata. |
| `winctl-mcp` | [`capture.wait_for_window_image_change`](CAPTURE_WAIT_FOR_WINDOW_IMAGE_CHANGE.md) | Poll bound-window screenshots until the image bytes change, returning replay-safe capture diagnostics. |

## Related Docs

- [Macro manifest](MACRO_MANIFEST.md)
- [Configuration](CONFIGURATION.md)
- [Dashboard](DASHBOARD.md)
- [Tray controller](TRAY_CONTROLLER.md)
- [Recorder](RECORDER.md)
- [Test manifest](TEST_MANIFEST.md)
- [Distribution](DISTRIBUTION.md)
- [Client configs](CLIENT_CONFIGS.md)
- [Directory layout](DIRECTORY_LAYOUT.md)
- [Troubleshooting](TROUBLESHOOTING.md)
- [MiniLM embeddings](MINILM_EMBEDDINGS.md)
- [Memory backup](MEMORY_BACKUP.md)
