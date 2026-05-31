# Roadmap

`winctl-mcp` is a strict Windows MCP control server for known app targets. The roadmap expands the tool surface while preserving target identity validation, safe process ownership, and auditable diagnostics.

## Guiding Principles

- Prefer strict identity over fuzzy matching.
- Fail closed when targets are ambiguous, stale, inaccessible, or unverifiable.
- Keep control and capture implementations transport-independent.
- Keep HTTP loopback-first; require authentication for exposed listeners.
- Log every control, capture, process, and replay action.
- Guard destructive tools with explicit ownership, allowlists, or opt-in configuration.

## Phase 0: Current Foundation

**Status:** Complete.

**Audit comments:** Implemented as the baseline server/tool surface. The foundation now includes Streamable HTTP, stdio compatibility, config/logging, process ownership, capture paths, and native tray/dashboard runtime pieces. `process.kill` supports tracked-process termination, but the advertised `kill_tree` option remains follow-up work.

- Window enumeration, find, bind, describe, focus, monitor listing, and point diagnostics.
- Bound-window click, text input, window screenshots, and display screenshots.
- Process launch, list, describe, tracked kill, and wait-for-window flows.
- Streamable HTTP and stdio MCP transports with stderr/log-file logging.
- Self-test command, Windows integration harness, and WSL cross-build support.

## Phase 1: Interaction Coverage

**Status:** Complete.

**Audit comments:** Implemented through the `input.*`, `windows.wait_for_state`, `process.wait_for_exit`, and capture wait/image-change tools. Continued tuning belongs in reliability work, not this phase.

- Add scroll, mouse move, drag, double-click, keyboard shortcuts, key down/up, and action delays.
- Add richer wait conditions for window state, foreground status, title/class changes, image checks, and process lifecycle.
- Preserve bound-target identity checks before every control action.
- Return replay-safe metadata for timing, coordinate space, resolved screen position, and preflight target.

## Phase 2: UI Automation Snapshot Layer

**Status:** Complete.

**Audit comments:** Implemented as read-only `uia.snapshot`, `uia.find`, and `uia.resolve`. The action layer remains intentionally separate and is tracked later.

- Capture UI Automation trees with element names, roles, automation IDs, bounds, enabled/focused state, and hierarchy.
- Add stable element references that can be revalidated before element-targeted actions.
- Add element diagnostics for inaccessible, virtualized, offscreen, disabled, and duplicate elements.
- Keep screenshot-first capture available for visual workflows and fallback diagnostics.

## Phase 3: App and Window Management

**Status:** Complete.

**Audit comments:** Implemented through `app.launch`, process/window ownership metadata, `windows.for_process`, and window move/resize/minimize/maximize/restore/close/foreground diagnostics. Child-process tree termination for tracked launches remains open behind the guarded `process.kill kill_tree` option.

- Add richer app launch modes for packaged apps, Start Menu entries, protocol handlers, and working-directory presets.
- Add window move, resize, minimize, maximize, restore, close, and foreground diagnostics.
- Add multi-window app handling with explicit candidate lists and child-process window policy.
- Keep PID, HWND, executable, and launch-session identity at the center of every control decision.

## Phase 4: Browser-Aware Automation

**Status:** Complete for first-pass browser-aware automation.

**Audit comments:** Implemented browser list/describe/wait/assert/screenshot/content-hint tools with window/process identity. Deep DOM/CDP/WebDriver automation is intentionally deferred to a later phase.

- Add browser state capture for Chrome, Edge, and Firefox.
- Track browser process, profile, window, and tab identity separately.
- Prefer explicit browser/session identity over tab-title matching.
- Support browser-focused assertions, navigation waits, screenshot checkpoints, and content extraction.

## Phase 5: System Utility Tools

**Status:** Complete.

**Audit comments:** Implemented guarded clipboard, filesystem, artifact export, registry, notifications, and process diagnostics tools. Destructive actions remain policy-gated.

- Add guarded clipboard read/write tools.
- Add filesystem read, list, search, copy, move, delete, and artifact export with allowlist policy.
- Add notification inspection, registry read/list/write/delete with opt-in configuration, and additional process diagnostics.
- Keep destructive operations disabled or denied unless explicitly configured.

## Phase 6: Network and Content Tools

**Status:** Complete.

**Audit comments:** Implemented `network.fetch` and `network.scrape` with timeout, size, redirect, scheme, and private-network policy controls.

- Add guarded fetch and scrape utilities with scheme restrictions, timeout limits, response-size limits, redirect controls, and private-network protections.
- Keep network tools separate from local Windows UI control tools.
- Log request intent, destination, response metadata, and truncation/blocked reasons.

## Phase 7: Security and Configuration

**Status:** Complete.

**Audit comments:** Implemented TOML config, auth tokens, transport settings, logging paths, filesystem roots, tool policy, dashboard auth, startup diagnostics, and loopback-first HTTP behavior.

- Add a first-class config file for transports, auth, tool exposure, logging, filesystem roots, and destructive-tool policy.
- Add tool allowlists/denylists, auth token management, optional TLS, CORS policy, and IP allowlists.
- Add startup diagnostics that explain which tools are exposed and why.
- Preserve loopback-only HTTP defaults.

## Phase 8: Distribution and Client Setup

**Status:** Complete.

**Audit comments:** Implemented packaged release directory, MSI workflow, checksums, install/update scripts, client config examples, CI/release automation, and troubleshooting scripts.

- Add packaged releases with checksums and version metadata.
- Add installer/update flow for the Windows binary.
- Add MCP client configuration examples for Codex, Claude Desktop, and other compatible clients.
- Add troubleshooting scripts for transport startup, Windows permissions, capture availability, and target discovery.

## Phase 9: Recorder and Replay UI

**Status:** Complete for recorder/replay foundation.

**Audit comments:** Implemented recorder MCP tools, macro promotion/list/get/run/dry-run/export, manifest-backed replay, and the native Vue dashboard/tray surface. Richer visual recording UX remains future dashboard work.

- Add a local UI for inspecting windows, binding targets, recording interactions, and previewing screenshots.
- Record click offsets, coordinate spaces, typing, shortcuts, waits, screenshots, and target identity.
- Let users edit recorded steps before saving.
- Generate stable replay manifests instead of opaque scripts.

## Phase 10: Test Manifest Standard

**Status:** Complete for manifest foundation; checkpoint assertion wiring remains open.

**Audit comments:** Implemented the versioned test manifest layer with validation, dry-run, run, export result, and macro-manifest integration. `macro.assert_image_checkpoint` and `macro.assert_text_checkpoint` are still runner stubs and need to call the existing visual/text assertion providers.

- Define a versioned YAML or JSON manifest for UI tests.
- Include app launch, binding strategy, preconditions, actions, waits, assertions, screenshots, cleanup, and artifact paths.
- Add CLI and MCP commands to validate, dry-run, run, and export replay results.
- Store enough metadata to replay across monitor layouts and diagnose target mismatches.

## Phase 11: Desktop Control Notification and Consent

**Status:** Partially complete.

**Audit comments:** Implemented `control.state`, `control.arm`, `control.consent`, `control.notify`, `control.revoke`, and `control.emergency_stop`, plus dashboard control indicators, sensitive-tool gate checks, and a Windows `Ctrl+Alt+Esc` global emergency-stop hotkey. Native Windows toast and target-window overlay/border indicators remain open provider work.

**Verification:** A Windows runtime integration test now verifies the global Ctrl+Alt+Esc emergency-stop hotkey against a live MCP server session.

- Add tray and dashboard control-state indicators for idle, armed, warning, controlling, blocked, and revoked states.
- Show a native Windows toast before the first focus/click/type/control action in a session, including target app, PID/HWND, requesting client/session when available, and a short cancelable countdown.
- Add an active-control indicator while input is being injected, such as a tray active state and optional target-window overlay or border.
- Add an explicit consent gate for sensitive actions: typing, close/kill, filesystem mutation, registry mutation, clipboard write, macro/test replay, and future UIA actions.
- Add "allow once", "allow for this session", "deny", and "revoke session" decisions tied to exact process/window identity rather than title matching.
- Add a global emergency stop hotkey, tray "Stop control now" action, dashboard revoke button, and server-side kill switch that rejects future control calls until rearmed.
- Route control tools through a shared control-gate state machine: `idle -> armed -> warning -> controlling -> cooldown -> idle`, with fail-closed behavior when consent is missing or revoked.
- Log every notification, consent decision, countdown, cancellation, revocation, and emergency stop event.

## Phase 12: UI Automation Action Layer

**Status:** Complete for core direct UI Automation patterns.

**Audit comments:** Added revalidated action tools with before/after diagnostics, control-gate integration, wait support, and Windows COM providers for `InvokePattern`, `ValuePattern`, `TogglePattern`, `SelectionItemPattern`, `ExpandCollapsePattern`, `RangeValuePattern`, `ScrollItemPattern`, and `SetFocus`. Coordinate fallback is now returned as a hint rather than silently dispatched. Desired-state toggle and add/remove selection modes remain future refinements.

**Verification:** A Windows runtime integration test now launches `winctl-test-target`, binds by returned PID/HWND, and exercises `uia.invoke`, `uia.set_value`, `uia.get_value`, `uia.toggle`, `uia.select`, `uia.expand_collapse`, `uia.range_value`, `uia.scroll_into_view`, and `uia.set_focus` through the MCP HTTP transport with behavior assertions.

- Promote the read-only UIA snapshot layer into a strict element-action layer that operates on revalidated element references instead of pixel coordinates.
- Add `uia.invoke`, `uia.set_value`, `uia.get_value`, `uia.toggle`, `uia.expand_collapse`, `uia.select`, `uia.set_focus`, `uia.range_value`, and `uia.scroll_into_view`, each mapped to a specific UI Automation control pattern.
- Resolve every action against a fresh bound-window snapshot, verify the element supports the requested pattern, and fail closed on stale references, unsupported patterns, disabled, or offscreen elements.
- Return before/after element state, the pattern used, and a coordinate-fallback hint (element bounds) for visual replay.
- Add `uia.wait_for_element` to wait for a selector to appear, become enabled, or change value before acting.
- See `docs/UIA_ACTIONS_DESIGN.md` for tool signatures and pattern mappings.

## Phase 13: Assertion and Visual Verification Tools

**Status:** Mostly complete.

**Audit comments:** Added `assert.element`, `assert.text_visible`, `assert.pixel_color`, `assert.window_count`, `assert.clipboard`, `capture.read_text`, `capture.ocr_region`, and `capture.compare_baseline`. `capture.ocr_region` now crops the requested region and runs a direct Tesseract OCR provider with word bounding boxes; an in-box Windows.Media.Ocr provider remains future work for systems without Tesseract. The standalone visual/text providers still need to be wired into macro/test checkpoint assertions.

- Add first-class assertion tools that return structured pass/fail: `assert.element` (exists/enabled/value/name), `assert.text_visible`, `assert.pixel_color`, and `assert.window_count`.
- Add `capture.ocr_region` and `capture.read_text` for text extraction from custom-rendered, canvas, or GDI-drawn UIs that the UIA tree cannot expose.
- Add `capture.compare_baseline` for golden-image visual regression with configurable tolerance and a saved diff-image artifact.
- Add explicit tools to assert clipboard contents after UI "Copy" actions.
- Integrate assertions and visual checks as first-class step kinds in the test manifest standard.

## Phase 14: Build, Diagnostics, and Test Reporting

**Status:** Mostly complete.

**Audit comments:** Added direct allowlisted `build.run`, `process.metrics`, `diagnostics.crash_report`, and `test.report_export`. `build.run` now terminates timed-out child processes, `process.metrics` uses native Windows CPU/memory/handle/GDI/USER counters, and `diagnostics.crash_report` queries recent Application errors plus WER paths. Native dialog/UAC handling and run-video capture remain open work.

**Verification:** A Windows runtime integration test now asserts `process.metrics` returns nonzero working-set and handle counters for a launched test process.

- Add a policy-allowlisted `build.run` task runner (msbuild, dotnet, cargo, cmake) with structured error/warning parsing, distinct from unguarded shell execution.
- Capture launched-process stdout/stderr/log streams for the app under test and expose them as replay artifacts.
- Add crash and hang detection that auto-captures the last screenshot, Windows Event Log entries, WER dump paths, and hung-window status on failure.
- Add `process.metrics` for CPU, working set, handle counts, and GDI/USER object counts to catch resource leaks under repeated runs.
- Add native dialog and UAC prompt handling, optional run-video recording, and JUnit/HTML test-report export for CI.
- Add installer validation flows by asserting filesystem artifacts and registry keys created during the test run.

## Phase 15: Deep Web Automation & App Introspection

**Status:** Complete for local CDP WebSocket introspection.

**Audit comments:** Added `web.cdp.list_targets`, `web.cdp.evaluate`, `web.dom.snapshot`, `web.network.events`, `web.a11y.snapshot`, and `web.style.inspect`. Target discovery remains loopback-only and the WebSocket bridge now dispatches `Runtime.evaluate`, `DOMSnapshot.captureSnapshot`, `Network.enable`, `Accessibility.getFullAXTree`, and `CSS.getComputedStyleForNode`.

- Add a Chrome DevTools Protocol (CDP) or WebDriver bridge to allow evaluating JavaScript, inspecting DOM nodes, and intercepting network requests for web applications.
- Add Accessibility (a11y) tree validation tools tailored for web DOMs and embedded WebView2 containers.
- Expose visual tree styling metadata (e.g., margins, padding, colors) that the standard UIA tree often omits.

## Phase 16: Dashboard and System Tray Enhancements

**Status:** Partially complete.

**Audit comments:** Added dashboard control/inspection surfaces, authenticated UIA snapshot and screenshot endpoints, capture-image viewing, and tray shortcuts for recorder access/recording toggle. Visual diff browsing, manifest quick-launch catalogs, native toast alerts, and test completion monitoring remain follow-up UX work.

- Add a live view of the UI Automation tree and real-time screenshot feeds to the `/dashboard` for debugging active bindings.
- Add visual diff viewers to the dashboard for inspecting `capture.compare_baseline` failures.
- Expand the `winctl-tray` app with quick-launch shortcuts for common test manifests and macro suites.
- Add desktop notifications and tray alerts for test suite completions or failures.
- Add a quick-toggle in the tray app to enter/exit recording mode.

## Not Planned Without Further Design

- Unguarded arbitrary shell execution.
- Title-only control actions.
- Kill-by-name or kill-by-title.
- Unauthenticated non-loopback control APIs.
- Broad filesystem, registry, network, or process mutation without explicit policy controls.
