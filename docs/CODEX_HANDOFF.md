# winctl-mcp — Implementation Handoff

**Audience:** a coding agent (Codex) continuing implementation.
**Date:** 2026-05-31
**Branch audited:** `feat/more_features` (Round 2; Round 1 was `f62d517`)

---

## 0. Status update — Round 2 (updated 2026-05-31)

Commit `ce36fb5 "replace phase provider stubs with native implementations"`
addressed most of the Tier A/B stubs from this handoff. **Verified against source
and confirmed to compile for Windows targets** (full workspace, no errors).
Every native path is `cfg(windows)`-gated with `cfg(not(windows))` fallbacks, so the
Linux build still passes.

Follow-up commits added the Windows runtime UIA integration test gate, made public
Windows releases MSVC-first, wired process-tree termination, wired macro
checkpoint assertions, and completed the native control notification surfaces.

| Handoff item | Round-2 result | Evidence |
| --- | --- | --- |
| A1 Real UIA patterns | **DONE** | `current_pattern::<IUIAutomationInvokePattern>()` → `.Invoke()`, `ValuePattern.SetValue/CurrentValue`, `TogglePattern.Toggle`, `SelectionItemPattern.Select`, `ExpandCollapse`, `RangeValue`, `ScrollItem` in `winctl/src/uia.rs`. Coordinate fallback now a returned hint, not silent. |
| A2 Global emergency-stop hotkey | **DONE** | `RegisterHotKey(None, …, MOD_CONTROL\|MOD_ALT\|MOD_NOREPEAT, VK_ESCAPE)` + `WM_HOTKEY` loop in `control.rs`. |
| A2 Native toast | **DONE** | `control.rs` uses WinRT `ToastNotificationManager` before the first sensitive control action in a session, includes target PID/HWND context, and gives the human a Ctrl+Alt+Esc countdown window to cancel before control proceeds. |
| A2 Active-control overlay/border | **DONE** | `control.rs` shows a click-through, topmost layered overlay around the validated target HWND during active control and clears it on cooldown/revoke. The overlay thread acknowledges show/hide so responses do not report queued work as visible state. |
| B1 OCR backend | **DONE** | `assertions.rs` crops region → tries in-box `Windows.Media.Ocr` first on Windows → falls back to `tesseract` TSV parsing when available. |
| B2 `process.metrics` | **DONE** | `GetProcessMemoryInfo`, `GetProcessHandleCount`, `GetGuiResources`, `GetProcessTimes` in `winctl/src/process.rs`. |
| B3 `crash_report` Event Log + WER | **DONE** | Uses native Event Log APIs (`EvtQuery`/`EvtNext`/`EvtRender`) for structured Application error entries; scans `%LOCALAPPDATA%\CrashDumps` + WER `ReportQueue`/`ReportArchive` in `diagnostics.rs`. |
| C1 CDP WebSocket | **DONE** | `tokio-tungstenite` `connect_async` → `Runtime.evaluate`, `DOMSnapshot.captureSnapshot`, `Accessibility.getFullAXTree`, `CSS.getComputedStyleForNode` in `web.rs`. |
| R2-1 Windows runtime integration | **DONE** | Added Windows-native MCP integration coverage that launches `winctl-test-target`, binds by PID/HWND, exercises `uia.invoke/set_value/get_value/toggle/select/expand_collapse/range_value/scroll_into_view/set_focus`, reads `process.metrics`, and verifies the Ctrl+Alt+Esc emergency-stop hotkey. Verified locally with `cargo test --workspace --target x86_64-pc-windows-msvc --test windows_runtime -- --nocapture`; GNU compatibility was also checked from Windows cargo. Added a `windows-latest` CI job as the backstop. |
| R2-6 `process.kill` tree termination | **DONE** | `process.kill kill_tree=true` now snapshots the current process tree, validates the tracked root identity, terminates descendants deepest-first, terminates the root last, and still refuses untracked PIDs/launch IDs. Unit coverage verifies descendant collection and existing ownership rejection paths. |
| R2-7 Macro/test checkpoint assertions | **DONE** | `macro.assert_image_checkpoint` now calls the existing baseline comparison provider and fails the macro on visual mismatch. `macro.assert_text_checkpoint` now compares literal text, OCR output, or `capture.read_text` output and fails the macro on missing expected text. Unit coverage verifies both the successful image checkpoint path and failing text checkpoint path. |

**Round-2 items R2-1 through R2-7 are complete.** Remaining work is the longer-tail
Phase 14/16 backlog called out below.

1. **R2-1 — Windows runtime CI + integration tests: DONE.** The repository now has
   a live Windows MCP integration test that starts the HTTP server, launches
   `winctl-test-target`, binds by returned PID/HWND, exercises the native UIA COM
   action paths, samples `process.metrics`, and verifies Ctrl+Alt+Esc revokes the
   control gate. `.github/workflows/ci.yml` has a `windows-latest` backstop job.
2. **R2-2 — Native toast + active-control overlay: DONE.** First sensitive
   control actions now emit a native WinRT toast, wait through a short
   Ctrl+Alt+Esc-cancelable countdown, then show a click-through topmost overlay
   around the validated target HWND while the action is active. The Windows
   runtime integration test asserts the toast attempt, overlay show
   acknowledgment, overlay clear event, and global emergency-stop path.
3. **R2-3 — In-box OCR provider: DONE.** `capture.ocr_region` now uses
   `Windows.Media.Ocr.OcrEngine` on Windows, keeps Tesseract as a fallback, and
   reports per-provider diagnostics when no provider succeeds. The Windows runtime
   integration test OCRs known `winctl-test-target` text and asserts the
   `windows_media_ocr` provider was used.
4. **R2-4 — Harden `crash_report`: DONE.** `diagnostics.crash_report` now uses
   native Event Log APIs instead of shelling `wevtutil.exe`, renders event XML into
   structured fields, and keeps the existing WER discovery. The Windows runtime
   integration test deliberately crashes `winctl-test-target` and asserts a
   structured `wevtapi` Application error entry is returned.
5. **R2-5 — UIA refinements: DONE.** `uia.toggle` now supports idempotent
   `desired_state` values (`off`, `on`, `indeterminate`) and reports
   `toggle_count`; `uia.select` now supports `replace`, `add`, and `remove` modes
   mapped to `Select`, `AddToSelection`, and `RemoveFromSelection`. The Windows
   runtime integration test asserts no-op desired-state toggle, desired-state
   toggle, and selection add/remove behavior.
6. **R2-6 — `process.kill` `kill_tree` support: DONE.** The server now terminates
   the tracked process tree deepest-first while preserving the launch-session
   ownership gate and root identity validation.
7. **R2-7 — Macro/test checkpoint assertion wiring: DONE.** Manifests can now
   run visual and text checkpoints directly. Image checkpoints call
   `capture.compare_baseline`; text checkpoints compare literal text, OCR output,
   or `capture.read_text` output. Assertion failures return `ok=false`, so the
   macro/test run fails instead of silently recording a provider result.
8. **Still open from earlier:** Phase 14 native dialog/UAC handling + run-video
   capture; Phase 16 dashboard visual-diff viewer, manifest quick-launch, and
   toast/completion alerts; durable (append-only) consent audit log instead of the
   in-memory 200-event ring. `notifications.list` remains a low-priority provider
   stub.

Task A2 is now complete. The cross-cutting notes in §4 below remain current.
Tasks A1, B1, B2, B3, C1 are now DONE except as qualified above.

---

## 1. Context

`winctl-mcp` is a strict, identity-validated Windows automation MCP server (Rust
Cargo workspace, despite the `go/src/...` path). Recent commits added Phases
11–16: a desktop-control consent gate, a UIA action layer, assertion/visual
tools, build/diagnostics tools, and web (CDP) introspection.

This document captures an audit of that work and the remaining tasks needed to
make it production-grade. **The orchestration/contract layer is real and
well-built; the Windows-native providers behind many tools are stubs** that
return `"provider_enabled": false` / `"...not enabled in this build"`. The root
cause is the dev environment (WSL/Linux): every stub is Win32/COM/WinRT code that
can only be compiled and exercised on Windows. The core remaining work is
**implementing those native providers and verifying them on Windows.**

### Workspace layout (relevant crates)

```
crates/winctl/                  # core Windows automation library (Win32/COM)
crates/winctl-mcp-server/       # MCP server, tool handlers, control gate
  src/main.rs                   # tool registration + control-gate wiring
  src/tools/{control,uia,assertions,diagnostics,web,...}.rs
crates/winctl-tray/             # tray app (WebView2 dashboard, server control)
crates/winctl-memory/           # SQLite memory store
crates/winctl-macro/            # macro/test manifest engine
```

---

## 2. Current state — what is REAL vs STUB

> **Note:** the verdict table below reflects the **Round-1** audit (`f62d517`).
> Many STUB/PARTIAL rows were resolved in Round 2 (`ce36fb5`) — see §0 for the
> current status. Kept here for the per-tool mechanism detail and the control-gate
> enforcement summary, which are unchanged.

The control gate is genuinely enforced: **29 sensitive tools** route through the
single choke-point `tools::control::with_control_gate(...)` in `main.rs`
(all 9 `input.*`, every `uia.*` action, `process.kill`, registry/filesystem
mutations, `clipboard.write`, `windows.focus/close`, `macro.run`, `macro.run_step`,
`test.run`). Preflight rejects when emergency-stopped, revoked, expired, or
consent is missing. This part needs no rework.

| Tool | Verdict | Mechanism / gap |
| --- | --- | --- |
| `control.state/arm/consent/notify/revoke/emergency_stop` | **REAL** | In-memory state machine; consent tied to process/window identity |
| `control.notify` native toast | **STUB** | `control.rs` returns `native_toast.provider_enabled=false`; no WinRT toast |
| Global emergency-stop **hotkey** | **MISSING** | No `RegisterHotKey`; only a tool call / dashboard button |
| Active-control on-screen **indicator/overlay** | **MISSING** | No overlay window while input is injected |
| `uia.invoke` / `toggle` / `select` / `set_focus` | **PARTIAL** | Strict revalidation **then center-coordinate click** — NOT COM patterns |
| `uia.set_value` | **PARTIAL** | Focus + type fallback; no `ValuePattern.SetValue` |
| `uia.get_value` | **PARTIAL** | Reads snapshot props; no `ValuePattern` read |
| `uia.expand_collapse` / `range_value` / `scroll_into_view` | **STUB** | Fail closed: `uia_pattern_not_available` |
| `uia.wait_for_element` | **REAL** | Fresh-snapshot polling with timeout |
| `assert.element/text_visible/pixel_color/window_count/clipboard` | **REAL** | — |
| `capture.read_text` | **REAL** | Reads UIA text properties (NOT OCR — see naming note) |
| `capture.compare_baseline` | **REAL** | Pixel diff + red-marked diff artifact |
| `capture.ocr_region` | **STUB** | `ocr_provider_unavailable`; no OCR backend |
| `build.run` | **REAL** | Allowlist `cargo/dotnet/msbuild/cmake/ctest`; captures stdout/stderr; parses errors |
| `test.report_export` | **REAL** | JSON / JUnit XML / HTML |
| `process.metrics` | **STUB** | Returns process identity only; all counters `null` |
| `diagnostics.crash_report` | **PARTIAL** | Process/window/screenshot real; Event Log + WER empty stubs |
| `web.cdp.list_targets` | **REAL** | HTTP REST discovery over loopback only |
| `web.cdp.evaluate` | **STUB** | `cdp_websocket_not_implemented` |
| `web.dom.snapshot / network.events / a11y.snapshot / style.inspect` | **STUB** | WebSocket CDP not implemented |

---

## 3. Tasks, prioritized

Each task lists the target files, the concrete Windows APIs to use, and acceptance
criteria. Preserve existing invariants: strict identity revalidation before every
control action, fail-closed on ambiguity, log every action, keep the JSON contract
(`{"ok": bool, ...}`) stable so stubs upgrade transparently.

### TIER A — close the credibility gaps

#### A1. Real UIA control patterns
**Files:** `crates/winctl/src/uia.rs` (provider), `crates/winctl-mcp-server/src/tools/uia.rs` (handlers).

Today `uia.invoke/toggle/select/set_focus` secretly center-click and
`expand_collapse/range_value/scroll_into_view` fail closed. Replace with real
UI Automation control patterns obtained via
`IUIAutomationElement::GetCurrentPattern(<PATTERN_ID>)`:

| Tool | Pattern | Call |
| --- | --- | --- |
| `uia.invoke` | `IUIAutomationInvokePattern` | `Invoke()` |
| `uia.set_value` | `IUIAutomationValuePattern` | `SetValue()` (reject if `CurrentIsReadOnly`) |
| `uia.get_value` | `IUIAutomationValuePattern` / `TextPattern` | `CurrentValue` |
| `uia.toggle` | `IUIAutomationTogglePattern` | `Toggle()`; honor optional `desired_state` for idempotency |
| `uia.select` | `IUIAutomationSelectionItemPattern` | `Select`/`AddToSelection`/`RemoveFromSelection` |
| `uia.expand_collapse` | `IUIAutomationExpandCollapsePattern` | `Expand`/`Collapse` |
| `uia.range_value` | `IUIAutomationRangeValuePattern` | `SetValue()` clamped to min/max |
| `uia.scroll_into_view` | `IUIAutomationScrollItemPattern` | `ScrollIntoView()` |
| `uia.set_focus` | `IUIAutomationElement` | `SetFocus()` |

**Implementation note:** `GetCurrentPattern` needs a **live** `IUIAutomationElement`.
The snapshot resolver currently returns serialized `UiElementInfo` only. Re-walk
the cached `path: Vec<usize>` from the automation root on a fresh snapshot to
re-acquire the live element that matches the validated `element_ref`. COM is
already initialized `COINIT_MULTITHREADED` on the snapshot path — reuse it.

**Behavior:** if a pattern is unsupported, fail closed with
`pattern_not_supported` and include the element's available patterns in the error.
Keep the coordinate-click path available only as an **explicit, named** fallback
(e.g. a `fallback: "coordinate"` opt-in), never silent. Return before/after state,
the pattern used, and element `bounds` as a coordinate-fallback hint.

**Acceptance:** against `winctl-test-target`, each tool drives the corresponding
control without mouse movement; read-only/disabled/unsupported elements fail
closed with the documented codes. See `docs/UIA_ACTIONS_DESIGN.md`.

#### A2. Human-facing control notification surface
**Files:** new provider in `crates/winctl/` (e.g. `notify.rs`, `overlay.rs`,
`hotkey.rs`); wire into `crates/winctl-mcp-server/src/tools/control.rs` event model;
optionally surface in `crates/winctl-tray`.

**Round-2 status:** Complete in `control.rs`. The implementation uses a WinRT
toast for the first sensitive control action, a Ctrl+Alt+Esc-cancelable countdown,
the global emergency-stop hotkey, and an acknowledged click-through topmost overlay
around the validated target HWND while active control is in progress.

The gate protects the *MCP client* and now gives a *human at the keyboard* an
explicit signal before and during control. The three pieces are all driven by the
existing control event model:

1. **Native toast** before the first control action of a session. Use WinRT
   `Windows.UI.Notifications.ToastNotificationManager` (in-box, no external dep).
   Include target app, PID/HWND, requesting client/session, and a short cancelable
   countdown. Replace the `native_toast.provider_enabled=false` stub in `control.rs`.
2. **Global emergency-stop hotkey** via `RegisterHotKey` (e.g. Ctrl+Alt+Esc) on a
   dedicated message-loop thread; on press, call the same path as
   `control.emergency_stop` (set `Revoked` + `emergency_stop_active`, reject future
   control until rearmed).
3. **Active-control indicator** while input is injected: a borderless,
   click-through, always-on-top overlay/border around the target window (layered
   window: `WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST`), or at minimum a
   tray icon state change.

**Acceptance:** arming + first action raises a real toast; the hotkey halts control
from any foreground app; an indicator is visible during injection and clears on
`cooldown→idle`. Every notification/hotkey/overlay event is logged like existing
control events.

> This is the highest-leverage gap: it's the human half of the consent feature and
> is small and self-contained.

### TIER B — diagnostics & verification

#### B1. OCR backend for `capture.ocr_region`
**File:** `crates/winctl-mcp-server/src/tools/assertions.rs` (+ provider in `winctl`).
**Round-2 status:** Complete. The tool uses in-box `Windows.Media.Ocr.OcrEngine`
(`TryCreateFromUserProfileLanguages`) over the captured region bitmap on Windows,
falls back to Tesseract TSV parsing when available, and reports provider-level
diagnostics if no provider succeeds. The Windows runtime integration test verifies
recognized text from a `winctl-test-target` region.

#### B2. Real `process.metrics`
**File:** `crates/winctl-mcp-server/src/tools/diagnostics.rs`.
Populate the currently-`null` counters: `GetProcessMemoryInfo` (working set),
`GetProcessHandleCount`, `GetGuiResources` (GDI + USER object counts), and CPU% via
two `GetProcessTimes` samples over an interval. **Acceptance:** non-null counters
for a live PID; values track a deliberately leaking test process upward.

#### B3. `diagnostics.crash_report` — Event Log + WER
**File:** `crates/winctl-mcp-server/src/tools/diagnostics.rs`.
**Round-2 status:** Complete. `diagnostics.crash_report` uses `EvtQuery`,
`EvtNext`, and `EvtRender` against the Application log for recent Error/Critical
events, renders structured event fields from XML, filters by target process
identity where available, and discovers WER dumps under `%LOCALAPPDATA%\CrashDumps`
plus WER `ReportQueue`/`ReportArchive`. The Windows runtime integration test
deliberately crashes `winctl-test-target` and verifies a structured Event Log
entry is returned.

### TIER C — bigger lift

#### C1. CDP WebSocket layer
**File:** `crates/winctl-mcp-server/src/tools/web.rs`.
Discovery (`web.cdp.list_targets`) works; everything else is stubbed. Add a
WebSocket client, connect to a target's `webSocketDebuggerUrl`, and implement CDP
sessions for: `cdp.evaluate` (`Runtime.evaluate`), `web.dom.snapshot`
(`DOMSnapshot.captureSnapshot`), `web.network.events` (`Network.enable` + buffer),
`web.a11y.snapshot` (`Accessibility.getFullAXTree`), `web.style.inspect` (CSS).
Keep loopback-only and respect existing URL guards. **Acceptance:** `cdp.evaluate`
returns a JS expression result from a loopback Chrome target.

---

## 4. Cross-cutting

- **On-Windows CI is required and now present.** The `windows-latest` workflow job
  compiles the `windows`-crate paths and runs the integration harness
  (`winctl-test-target`). Keep expanding this job as new native providers are
  added.
- **Consent state is in-memory only** (`control.rs` state + a 200-entry
  `VecDeque<ControlEvent>`), lost on restart. Fail-closed is correct for security,
  but for an auditable consent feature consider a durable, append-only audit log.
- **Naming footgun:** `capture.read_text` reads UIA text properties, not pixels.
  Adjacent to `capture.ocr_region` it reads like OCR. Consider renaming (e.g.
  `capture.read_ui_text`) or documenting the distinction prominently.
- **No silent fallbacks.** Where a coordinate click substitutes for a UIA pattern,
  it must be opt-in and reflected in the response, so test authors aren't misled
  about reliability.

## 5. Suggested order

1. Keep broadening the Windows runtime integration test as each native provider is
   completed.
