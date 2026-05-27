# PLANNING.md

## Current state
The repository has initial workspace/module scaffolding and partial tool flow skeletons. Core Win32 implementations and MCP runtime wiring remain incomplete.

## Next steps (priority ordered)

1. **Implement real window enumeration (`windows.list`) in `winctl`**
   - Use Win32 `EnumWindows`.
   - Capture HWND, PID/TID, title, class, rect, visibility, minimized, cloaked, foreground.
   - Resolve process name and full executable path.

2. **Implement robust selection + binding (`windows.find`, `windows.bind`)**
   - Keep scoring for diagnostics, but enforce strict ambiguity failures for weak selectors.
   - Add explicit bind errors for: no match, ambiguous match, stale/invalid target.
   - Store bind record with identity tuple and bind timestamp.

3. **Implement bound identity revalidation for every control action**
   - Before focus/click/type/screenshot_window:
     - Re-fetch HWND metadata.
     - Verify HWND still exists and maps to expected PID + executable.
   - Fail closed on mismatch.

4. **Implement monitor + virtual desktop geometry (`windows.monitors`)**
   - Enumerate all displays with virtual desktop coordinates (including negative X/Y).
   - Include DPI scale and primary monitor flag.
   - Return total virtual desktop bounds.

5. **Implement `windows.window_from_point` preflight diagnostics**
   - Resolve final screen point from requested coordinate space.
   - Use `WindowFromPoint`/`ChildWindowFromPointEx` style APIs.
   - Return top-level + child HWND identity fields (PID/process/exe/title/class).
   - Return whether resolved window belongs to bound target (or allowed child process policy).

6. **Implement input actions (`windows.focus`, `input.click`, `input.type_text`)**
   - Focus bound window using Win32 focus/foreground strategy with retries/timeouts.
   - `input.click` must optionally hard-fail if preflight is outside bound window.
   - `input.type_text` should support unicode text input via `SendInput`.

7. **Implement capture actions (`capture.screenshot_window`, `capture.screenshot_display`)**
   - Use `windows-capture` for image acquisition.
   - Always return exact `region_virtual_desktop` for each screenshot.
   - Ensure display screenshot and window screenshot metadata are unambiguous.

8. **Wire real MCP tool server (`rmcp`)**
   - Register first-milestone tools:
     - `windows.list`, `windows.find`, `windows.bind`, `windows.describe`, `windows.focus`, `windows.window_from_point`, `windows.monitors`
     - `input.click`, `input.type_text`
     - `capture.screenshot_window`, `capture.screenshot_display`
   - Add schemas + structured errors suitable for agent use.

9. **Add tracing and auditability**
   - Structured logs for bind/focus/click/type/capture + preflight resolution details.
   - Include identifiers and coordinates in logs (without leaking sensitive text when avoidable).

10. **Add Windows integration harness/tests**
   - Harness should exercise:
     - Real target app window (Betty-like test app)
     - Windows Terminal with confusing similar title
     - Browser tab with similar title
     - Optional WebView child window
   - Validate key acceptance criteria:
     - Title-only bind ambiguity
     - PID/HWND/process binds succeed
     - Click preflight catches wrong target
     - Screenshot region coordinates are explicit

11. **Packaging + runbook**
   - Add scripts/docs for running server on Windows host from WSL-built artifacts.
   - Document minimum required permissions and troubleshooting steps.

## Immediate sprint recommendation
- Complete steps 1, 2, 4, and 5 first.
- Then step 6 (`input.click` with strict preflight).
- Then step 7 for screenshot reliability.
- Only after these are stable, finalize step 8 and broader harness work.
