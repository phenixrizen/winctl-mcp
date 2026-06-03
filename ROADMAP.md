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

**Audit comments:** Implemented as the baseline server/tool surface. The foundation now includes Streamable HTTP, stdio compatibility, config/logging, process ownership, capture paths, and native tray/dashboard runtime pieces. `process.kill` now supports guarded tree termination for tracked launches.

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

**Audit comments:** Implemented through `app.launch`, process/window ownership metadata, `windows.for_process`, and window move/resize/minimize/maximize/restore/close/foreground diagnostics. `process.kill kill_tree` now terminates descendants deepest-first for launches tracked by the current server session.

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

**Audit comments:** Implemented guarded clipboard, filesystem, artifact export, registry, notifications, and process diagnostics tools. `notifications.list` uses the Windows `UserNotificationListener` provider when running on Windows and reports explicit listener access state for denied or unspecified consent. Destructive actions remain policy-gated.

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

**Status:** Complete.

**Audit comments:** Implemented the versioned test manifest layer with validation, dry-run, run, export result, and macro-manifest integration. `macro.assert_image_checkpoint` now calls the visual baseline comparison provider, and `macro.assert_text_checkpoint` now checks literal text, OCR output, or `capture.read_text` output. Assertion failures return `ok=false` so macro/test runs fail correctly.

- Define a versioned YAML or JSON manifest for UI tests.
- Include app launch, binding strategy, preconditions, actions, waits, assertions, screenshots, cleanup, and artifact paths.
- Add CLI and MCP commands to validate, dry-run, run, and export replay results.
- Store enough metadata to replay across monitor layouts and diagnose target mismatches.

## Phase 11: Desktop Control Notification and Consent

**Status:** Complete.

**Audit comments:** Implemented `control.state`, `control.arm`, `control.consent`, `control.notify`, `control.revoke`, and `control.emergency_stop`, plus dashboard control indicators, sensitive-tool gate checks, a Windows `Ctrl+Alt+Esc` global emergency-stop hotkey, a WinRT native toast before the first sensitive control action in a session, and a click-through topmost overlay around the validated target HWND while control is active. Notification, toast, overlay, countdown, revoke, and emergency-stop events are recorded in both the fast in-memory event stream and an append-only JSONL audit log under the capture/state directory. `control.state` exposes recent persisted audit entries and any audit-log write/read diagnostics.

**Verification:** A Windows runtime integration test now verifies the first-action notification path, native toast attempt, overlay show acknowledgment, overlay clear event, global Ctrl+Alt+Esc emergency-stop hotkey, and durable audit-log persistence across MCP server restart against live MCP server sessions.

- Add tray and dashboard control-state indicators for idle, armed, warning, controlling, blocked, and revoked states.
- Show a native Windows toast before the first focus/click/type/control action in a session, including target app, PID/HWND, requesting client/session when available, and a short cancelable countdown.
- Add an active-control indicator while input is being injected, such as a tray active state and optional target-window overlay or border.
- Add an explicit consent gate for sensitive actions: typing, close/kill, filesystem mutation, registry mutation, clipboard write, macro/test replay, and future UIA actions.
- Add "allow once", "allow for this session", "deny", and "revoke session" decisions tied to exact process/window identity rather than title matching.
- Add a global emergency stop hotkey, tray "Stop control now" action, dashboard revoke button, and server-side kill switch that rejects future control calls until rearmed.
- Route control tools through a shared control-gate state machine: `idle -> armed -> warning -> controlling -> cooldown -> idle`, with fail-closed behavior when consent is missing or revoked.
- Log every notification, consent decision, countdown, cancellation, revocation, and emergency stop event.

## Phase 12: UI Automation Action Layer

**Status:** Complete.

**Audit comments:** Added revalidated action tools with before/after diagnostics, control-gate integration, wait support, and Windows COM providers for `InvokePattern`, `ValuePattern`, `TogglePattern`, `SelectionItemPattern`, `ExpandCollapsePattern`, `RangeValuePattern`, `ScrollItemPattern`, and `SetFocus`. Coordinate fallback is now returned as a hint rather than silently dispatched. `uia.toggle` supports idempotent `desired_state`, and `uia.select` supports `replace`, `add`, and `remove` modes.

**Verification:** A Windows runtime integration test now launches `winctl-test-target`, binds by returned PID/HWND, and exercises `uia.invoke`, `uia.set_value`, `uia.get_value`, idempotent `uia.toggle`, `uia.select` replace/add/remove, `uia.expand_collapse`, `uia.range_value`, `uia.scroll_into_view`, and `uia.set_focus` through the MCP HTTP transport with behavior assertions.

- Promote the read-only UIA snapshot layer into a strict element-action layer that operates on revalidated element references instead of pixel coordinates.
- Add `uia.invoke`, `uia.set_value`, `uia.get_value`, `uia.toggle`, `uia.expand_collapse`, `uia.select`, `uia.set_focus`, `uia.range_value`, and `uia.scroll_into_view`, each mapped to a specific UI Automation control pattern.
- Resolve every action against a fresh bound-window snapshot, verify the element supports the requested pattern, and fail closed on stale references, unsupported patterns, disabled, or offscreen elements.
- Return before/after element state, the pattern used, and a coordinate-fallback hint (element bounds) for visual replay.
- Add `uia.wait_for_element` to wait for a selector to appear, become enabled, or change value before acting.
- See `docs/UIA_ACTIONS_DESIGN.md` for tool signatures and pattern mappings.

## Phase 13: Assertion and Visual Verification Tools

**Status:** Complete.

**Audit comments:** Added `assert.element`, `assert.text_visible`, `assert.pixel_color`, `assert.window_count`, `assert.clipboard`, `capture.read_text`, `capture.ocr_region`, and `capture.compare_baseline`. `capture.ocr_region` now crops the requested region, runs the in-box `Windows.Media.Ocr` provider first on Windows, and falls back to direct Tesseract TSV parsing with word bounding boxes when available. The visual/text providers are wired into macro/test checkpoint assertions.

**Verification:** A Windows runtime integration test now OCRs known fixture text from `winctl-test-target` and asserts `capture.ocr_region` uses the `windows_media_ocr` provider.

- Add first-class assertion tools that return structured pass/fail: `assert.element` (exists/enabled/value/name), `assert.text_visible`, `assert.pixel_color`, and `assert.window_count`.
- Add `capture.ocr_region` and `capture.read_text` for text extraction from custom-rendered, canvas, or GDI-drawn UIs that the UIA tree cannot expose.
- Add `capture.compare_baseline` for golden-image visual regression with configurable tolerance and a saved diff-image artifact.
- Add explicit tools to assert clipboard contents after UI "Copy" actions.
- Integrate assertions and visual checks as first-class step kinds in the test manifest standard.

## Phase 14: Build, Diagnostics, and Test Reporting

**Status:** Complete.

**Audit comments:** Added direct allowlisted `build.run`, `process.metrics`, `diagnostics.crash_report`, `test.report_export`, `dialogs.list`, `dialogs.invoke_button`, `capture.video_start`, and `capture.video_stop`. `build.run` now terminates timed-out child processes, `process.metrics` uses native Windows CPU/memory/handle/GDI/USER counters, and `diagnostics.crash_report` queries recent Application errors through the native Event Log API (`EvtQuery`/`EvtNext`/`EvtRender`) plus WER paths. Native dialog handling enumerates foreground dialog buttons and invokes explicit Button/SplitButton elements through UI Automation only; UAC secure-desktop prompts are detected and reported as not automatable. Run-video capture records bound windows or displays into GIF replay artifacts and can be attached automatically to macro/test run results.

**Verification:** A Windows runtime integration test now deliberately crashes `winctl-test-target` and asserts `diagnostics.crash_report` returns a structured `wevtapi` Application error entry for the crashed process.

**Verification:** A Windows runtime integration test now asserts `process.metrics` returns nonzero working-set and handle counters for a launched test process.

**Verification:** A Windows runtime integration test now launches a target-owned MessageBox, finds its foreground `#32770` dialog and `OK` button through `dialogs.list`, and closes it with `dialogs.invoke_button` using `InvokePattern`.

**Verification:** A Windows runtime integration test now records display 0 with `capture.video_start`, stops it with `capture.video_stop`, and asserts a GIF artifact exists with multiple captured frames.

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

**Status:** Complete.

**Audit comments:** Added dashboard control/inspection surfaces, authenticated UIA snapshot and screenshot endpoints, capture-image viewing, visual diff browsing for failed baseline comparisons, run-video artifact viewing, a read-only saved manifest catalog with copyable run JSON, and tray shortcuts for recorder access/recording toggle. The dashboard now uses the full available window width, and dashboard tables use search, pagination, and wrapping cells instead of forcing page-level horizontal overflow. The tray now monitors loopback dashboard state and shows native notification-area alerts when macro/test runs finish as passed, failed, or aborted.

- Add a live view of the UI Automation tree and real-time screenshot feeds to the `/dashboard` for debugging active bindings.
- Add visual diff viewers to the dashboard for inspecting `capture.compare_baseline` failures.
- Expand the `winctl-tray` app with quick-launch shortcuts for common test manifests and macro suites.
- Add desktop notifications and tray alerts for test suite completions or failures.
- Add a quick-toggle in the tray app to enter/exit recording mode.

## Phase 17: Dashboard Docs and Live Observability

**Status:** In progress — docs tab and structured dashboard surfaces shipped.

**Audit comments:** Added a Docs tab serving every `docs/*.md` page, rendered to HTML at build time with comrak and served from `/dashboard/docs`; ```mermaid blocks render as live diagrams via a vendored, lazy-loaded Mermaid.js, with markdown styled to match the dashboard. Added orientation diagrams (driving loop, control-gate state machine, macro lifecycle). Replaced bare dashboard JSON dumps outside the Raw tab with structured screenshot details, visual-diff metadata, run-video metadata, memory/macro detail panels, and a searchable paginated UIA element table. Live all-tool activity feed, active-control Stop banner, auto-refresh screenshot/UIA overlay, richer run timeline, and live `process.metrics` gauges remain follow-up work.

- Add a top-menu Docs/Tools tab serving every tool doc from `docs/*.md`, rendered to HTML at build time with comrak (GFM tables, code fences; raw HTML disabled).
- Render ```mermaid blocks as live diagrams: convert `code.language-mermaid` into Mermaid nodes and run a vendored (embedded, not CDN) Mermaid.js, lazy-loaded on first use.
- Style rendered markdown to match the dashboard (slate theme, `winctl-table` overrides); add doc search and deep-links from tool names.
- Add a live activity/audit feed of tool calls and control events (tool, target, args summary, ok/fail, consent decisions) sourced from the control event model.
- Add an active-control banner with a Stop button wired to `control.revoke`/`control.emergency_stop` (the in-UI kill switch).
- Add auto-refreshing screenshot + UI Automation feeds for the active bound target, with element-bounds overlay and an interactive UIA tree explorer.
- Add a macro/test run timeline (per-step pass/fail + artifacts), a visual-diff viewer for `capture.compare_baseline` failures, and live `process.metrics` gauges.
- Keep the dashboard loopback-first and read-only except for the explicit Stop action.

## Phase 18: Macro Replay Target Resolution

**Status:** Implemented for the replay/runtime path; runtime preflight remains future work.

**Audit comments:** A replay failure exposed a model gap: the macro runtime can launch an app, bind the fresh window, and store `context.bound_id`, but target-bound steps are inconsistent. Some tools use the macro context through helper logic, while `input.*` and most `uia.*` steps deserialize raw args and therefore require `bound_id` to already be present. The fix should not be blanket injection of whatever `context.bound_id` happens to hold. The safer design is a first-class target resolver that converts explicit step target intent into a freshly revalidated `bound_id` immediately before dispatch.

**Claude review summary:** Keep targeting explicit and centrally resolved. Do not silently inject a current target into every target-bound tool. Recorded manifests should use target aliases or a declared current target instead of persisting runtime `bound_id` values. Resolution must fail closed when the target is missing, stale, ambiguous, or not yet established.

**Implementation notes:** Added `target: current` and `target: alias` manifest targets, alias-before-bind validation, a central macro pre-dispatch resolver that injects `bound_id` only after `AppState::revalidate_bound_window`, and runtime diagnostics for legacy `args.bound_id` or implicit-current fallback. The resolver now covers `windows.*`, `input.*` including `input.mouse_move`, UIA read/action/wait tools, bound capture tools, macro checkpoint assertions, and dialog tools in the macro tool table. Recorder output now defaults target-bound recorded steps to `target: current` when no explicit target is provided.

- Add a typed macro target model for replay intent: current target, named target alias, launched-process window, and explicit legacy bound-window target.
- Add a single pre-dispatch target resolver for all target-bound tools (`input.*`, `uia.*`, `windows.*`, `capture.*`, assertions, and dialogs). The resolver lowers `step.target` into `args.bound_id` only after revalidating HWND, PID, executable, and expected identity.
- Keep explicit `args.bound_id` and `${bound_id}` substitutions backward compatible, but route them through the same resolver/revalidation path and emit deprecation diagnostics when a manifest persists runtime-specific handles.
- Update recorder output to establish named target aliases during launch/bind and then record later steps against `target: current` or `target: alias`, not raw runtime `bound_id`.
- Add static validation that catches target-bound steps with no declared target, alias-before-bind, multiple current-target candidates, missing launch/bind prerequisites, and target-bound request schemas that would still fail after lowering.
- Add a read-only dry-run/preflight mode that launches/binds/revalidates/resolves targets but skips mutating input/UIA actions, returning a per-step target-resolution report before desktop control is armed.
- Preserve fail-closed behavior: no title-only target resolution, no implicit fallback to a stale previous binding, no guessing when multiple windows match, and no coordinate or pixel fallback unless explicitly requested and reported.
- Add integration coverage for launch -> bind -> target alias/current -> `input.*`/`uia.*` replay without manually editing `bound_id` into each step, plus negative tests for missing current target, stale HWND, PID recycle, and multi-window ambiguity.

## Phase 19: Human-Recorded Macros and Secrets Vault

**Status:** Complete.

**Audit comments:** Implemented DPAPI-backed `secret.*` tools, gated `macro.type_secret`, native recorder hooks/hotkeys, password-field redaction, launch/bind inference, dashboard Recorder review controls, and replay through the Phase 18 target resolver. Windows runtime coverage records `winctl-test-target`, asserts semantic click/text/secret steps with no plaintext in the raw manifest, simulates review by binding a stored `secret_ref`, and replays the manifest successfully. The first cut keeps dashboard review focused on scrub/bind/promote; richer assertion insertion remains future refinement.

Let a developer record a macro by performing the task themselves: global input hooks resolve the human's clicks and keystrokes into semantic UI Automation steps, and an encrypted, name-referenced secrets vault lets login and secure flows be recorded and replayed without the password ever entering the manifest or the model. Output is a standard `winctl.macro.v1` manifest the MCP replays, using the Phase 18 target model (launch establishes a named alias; later steps use `target: current`/`target: alias`, never raw runtime `bound_id`). Scope is trusted, own-machine developer use. Build order: secrets vault first (it is a dependency and independently useful), then the capture engine, then review/save UX.

- Add a DPAPI-encrypted secrets vault in the embedded SQLite DB with `secret.set`, `secret.list` (names/metadata only), and `secret.delete`. Provide NO model-facing `secret.get`; a server-internal resolver decrypts a secret only at the instant it is typed during replay. Plaintext is never returned to the model, written to a manifest, or logged. DPAPI ties secrets to the current Windows user.
- Add a `type_secret` macro step kind that resolves a `secret_ref` by name, decrypts server-side, and types it through the existing gated input path; gate it through the control consent gate like other input.
- Add an in-server capture engine: global `WH_MOUSE_LL` and `WH_KEYBOARD_LL` hooks on a dedicated message-loop thread (the Phase 11 hotkey-thread pattern), ignoring events whose target window belongs to the recorder's own tray, dashboard, or overlay.
- Add an event-to-step translator: coalesce mouse down/up into `click`, double-click timing into `double_click`, down/move/up into `drag`, wheel into `scroll`, character runs into `type_text`, and modifier combos into `shortcut`. Resolve each pointer event via UIA `ElementFromPoint` to a semantic target (automation id, name, role, class, owning-window identity, stable element reference) with coordinate-plus-screenshot fallback, emitting steps in the Phase 18 target model.
- Detect focused UIA password fields (`IsPassword`): buffer those keystrokes in memory only (never to disk, log, or model), emit a `type_secret` step with an unbound `secret_ref`, and let the user bind it to a named secret or create one from the buffered value during review.
- Handle launch with infer-and-confirm: derive a `launch`/`bind` step from the first interaction's process identity (executable path and command line) and establish a named target alias; support an optional explicit fresh launch for deterministic test macros, bind-only for already-running apps, and per-step identity for multi-app/multi-window sessions; let the user confirm or edit launch intent in review.
- Add session control via global hotkeys (reuse the Phase 11 `RegisterHotKey` infrastructure for start/stop/pause) and tray-toggle plus dashboard buttons, with a persistent on-screen RECORDING overlay (Phase 11 layered window) and tray state. Log every session start, stop, and pause to the audit log; the recorder is a passive observer and never injects input.
- Add a dashboard Recorder tab to review a stopped session: list captured steps, edit/reorder/delete, scrub text, insert waits and assertions, confirm launch/bind and bind-only choices, bind secret references, then save as a `winctl.macro.v1` manifest via `macro.promote`/memory for replay with `macro.run`.
- Add tests: unit coverage for the coalescer (down/up to click, double-click timing, drag detection, character-run merging) and the DPAPI vault round-trip (encrypt/decrypt; `secret.list` never returns plaintext); a Windows integration test that records against `winctl-test-target` (click a button, type into an edit, type into a password field) and asserts the manifest has a semantic click, a `type_text`, and a plaintext-free `type_secret`, then replays it with a stored secret.
- First cut keeps the minimum loop (hooks to coalescer to saved manifest to replay with secrets); the review/scrub UI and assertion insertion may follow as refinements.

## Phase 20: Server Observability — Connected Clients and Request History

**Status:** Complete.

**Audit comments:** Implemented in-memory server observability for connected MCP
clients and a bounded 500-entry tool request history. HTTP and stdio sessions
register connection identity, MCP `initialize` captures client name/version,
`tools/call` records duration/status/error code plus allowlist-redacted
summaries, `/dashboard/state` returns live `connected_clients` and
`recent_requests`, and the dashboard Observability tab renders searchable,
paginated Clients and Requests tables. Persistence remains intentionally
deferred per the phase scope.

Replace the dashboard's "connected client and request-history tracking are not enabled yet" placeholder with a real observability panel: which MCP clients are connected and a rolling history of tool calls. Each request is attributed to the client that made it, and entries carry only redacted metadata so typed text, secrets, and large blobs never reach the panel. This request-history ring is also the backend data source for the Phase 17 live activity feed.

- Give each per-session MCP server instance a `connection_id`. The HTTP service factory mints one handler per client session, so override rmcp `initialize` to capture the client's name/version against that connection, and deregister on the instance's `Drop`. If rmcp is found not to instantiate one handler per session, fall back to a transport-level task-local or `Mcp-Session-Id` header carried into the dispatch path.
- Add a connected-clients registry in `AppState`: `connection_id -> { name, version, transport, connected_at, last_seen, request_count, last_tool }`, updated on each request, pruned on disconnect/Drop with staleness fallback. Stdio runs as a single fixed connection.
- Add a bounded request-history ring (~500 entries) of `{ id, connection_id, tool_name, started_at, duration_ms, ok, error_code, summary }`, recorded centrally at the `run_blocking_tool` choke point (measure duration, read result `ok`/`error.code`, attribute via the instance `connection_id`).
- Add a `safe_summary(tool, args, result)` redactor built on an explicit allowlist of non-sensitive keys (e.g. `bound_id`, `pid`, `launch_id`, `x`/`y`, counts). It must never emit `text`, `value`, `secret_ref`, clipboard/memory text, or large blobs; an allowlist (not denylist) prevents future sensitive fields from leaking by default.
- Update `dashboard_state_json` to return real `connected_clients` and `recent_requests` and drop the placeholder warning.
- Add a dashboard observability panel: a Clients table (client, version, transport, connected-at, last-seen, request count, last tool) and a Requests table (time, client, tool, duration, ok/fail, redacted summary), reusing the existing search/paginate table components. Keep it polled on the existing refresh for v1; true streaming is deferred to the Phase 17 live feed.
- Keep both rings in-memory (lost on restart, consistent with the control-event ring); durable persistence is future work. The panel stays behind the existing dashboard auth.
- Add tests: a table-driven redactor test asserting `text`/`value`/`secret_ref` are never emitted for representative tools, ring-eviction behavior, and an integration test that connects an MCP client, issues several tool calls, and asserts the dashboard state shows the client (name/version) plus attributed, redacted request rows.

## Phase 21: Assertion and Require Primitives

**Status:** In progress — implementation complete; Windows capture verification blocked.

Audit comments: Implemented the shared assertion contract, rich element/text/pixel/window/clipboard/process/dialog/file/registry/visual assertions, macro manifest descriptors/dispatch/replay handling, docs, and unit/Windows runtime coverage. Deferred `assert.a11y` and `assert.timing` because this phase marks them deferrable and they need separate accessibility contrast/tab-order and timing-metadata designs. Full Windows workspace verification is still blocked by pre-existing Graphics Capture runtime failures (`0x80070490 Element not found`) in OCR/video capture tests outside the Phase 21 assertion surface.

Expand the assertion surface so a developer can fully describe "does my app work" in automated tests. Every assertion shares one model: a `negate`/`expect: present|absent` flag (so absence checks reuse the same tool), an optional `timeout_ms`/`poll_interval_ms` that turns the check into a require/wait-until (poll until it holds, fail on timeout — no separate wait tools), a uniform result `{ ok, passed, negated, expected, actual, predicate, target, elapsed_ms, diagnostics }`, non-mutating execution, targets resolved through the Phase 18 resolver, and a mirrored manifest assertion kind usable in both `preconditions` (fail-fast requires) and `assertions`.

- Extend `assert.element` into a rich matcher: exists, enabled, focused, checked (toggle state), selected, expanded, value (equals/contains/regex via ValuePattern), name, role/control type, editable/readonly, on/offscreen, bounds within tolerance, and count (N elements match the selector).
- Add `assert.window`: exists, foreground, minimized/maximized/normal, title/class (equals/contains/regex), size/position within tolerance, and responsive (not hung).
- Add `assert.process`: running/exited, exit code, responsive (not hung), resource ceilings (working-set/handle/GDI/USER via `process.metrics`), and no new crash/WER dump or Application Event Log error since a recorded marker.
- Add `assert.no_dialog` and `assert.dialog`: fail when an unexpected modal/error dialog is present, or assert a specific dialog with expected title/text/buttons (reusing the dialog tools).
- Add `assert.file` (exists/absent, content equals/contains/regex, size/hash, within allowlisted roots) and `assert.registry` (value exists/absent/equals/kind), reusing the existing filesystem and registry policy and read paths.
- Add `assert.visual_match`: element or region screenshot compared to a baseline with tolerance and a saved diff artifact, building on `capture.compare_baseline`; negation asserts no visual change.
- Add `assert.a11y`: accessible name present, keyboard-focusable, role assigned, contrast ratio above a threshold, and monotonic tab order. (Softest of the set; first candidate to defer if the phase grows too large.)
- Add `assert.timing`: a measured duration is within budget (action latency, window render after launch), realized as an assertion over recorded step timing metadata. (Also deferrable.)
- Extend `assert.text_visible` and `assert.pixel_color` with negate, timeout, regex, and explicit window/element scope.
- Add every new assertion as a manifest assertion kind, validated as non-mutating, and keep assertion failures returning `ok=false` so macro/test runs fail correctly.
- Add tests: per-predicate unit coverage (true/false/negated, wait-until success and timeout) and a Windows integration pass against `winctl-test-target` (toggle a checkbox then assert checked, close a window then assert it is absent, crash a process then assert it exited and is crash-free=false, write a file then assert it exists, etc.).

## Phase 22: Test Orchestration and Suite Runner

**Status:** Planned (follow-on to Phase 21).

Build the harness that runs the Phase 21 primitives at scale so the MCP can fully automate testing while an app is being developed.

- Add a test-suite concept that runs many `winctl.macro.v1`/test manifests in order, with shared setup/teardown fixtures, per-test tags, and selective runs by tag or name.
- Add retry and quarantine policy for flaky tests, with per-attempt artifacts and a final stable/flaky verdict.
- Add a watch mode that reruns a suite (or an affected subset) when a build output changes, closing the edit -> rebuild -> retest loop driven by the Phase 14 `build.run` tooling.
- Aggregate results into JUnit XML and HTML reports (extending `test.report_export`) with per-test status, timing, and linked artifacts (screenshots, diffs, crash reports) for CI.
- Surface suite progress and results in the dashboard, including the Phase 17 visual-diff viewer for failed `assert.visual_match` checkpoints.
- Keep orchestration loopback-first and behind the control consent gate for any suite that performs control actions.

## Phase 23: CDP Web Automation (Chrome, Edge, WebView2)

**Status:** Planned.

Turn the read-only CDP introspection from Phase 15 into full web automation across all Chromium surfaces: standalone Chrome/Edge and embedded WebView2. Introduce a protocol-agnostic web session abstraction (CDP backend now, designed to admit the WebDriver backend in Phase 24) so the tool surface is identical regardless of browser.

- Add a `WebSession` abstraction with a CDP backend and refactor the existing `web.*` tools onto it; add a session/target model where tools take a `web_session_id` (plus optional frame), per-backend capability flags, and an `unsupported_for_protocol` result where applicable.
- Add attach/launch tools: `web.launch` (Chrome/Edge with `--remote-debugging-port`, reusing process ownership), `web.attach` (a running debug endpoint or a WebView2 host — discover its CDP port and document the host opt-in via `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`), and `web.targets`/`web.close` (generalizing `web.cdp.list_targets`). Keep debug endpoints loopback-only.
- Add gated interaction tools: `web.navigate`/`reload`/`back`/`forward`; `web.click`/`type`/`set_value`/`press_key`/`select_option`/`hover`/`focus`/`scroll_into_view`/`upload_file` by selector; `web.wait_for` (selector, navigation, network-idle, response, or JS predicate). Typing supports the Phase 19 `secret_ref` so passwords never enter the manifest.
- Add DOM read tools: `web.query` (CSS/XPath to nodes with attributes, text, and box model) and `web.get_attribute`/`text`/`value`; keep the existing DOM, accessibility, and style snapshots.
- Add network interception and mocking: `web.network.intercept` rules that match (url/method/resource type) and block, fulfill (mock status/headers/body), modify the request, continue, or fault-inject (offline/abort/delay); plus `web.wait_for_response`/`network_idle` and throttling, over the CDP `Fetch`/`Network` domains. Gated.
- Add `web.console.events` to capture console messages and uncaught JS exceptions, so a test can catch a broken web app.
- Add `web.screenshot` (full-scrolling-page and per-element, emitted as an artifact for `assert.visual_match`) and `web.cookies.*`/`web.storage.*` for auth/session setup.
- Extend the Phase 21 assertion model with a web target (selector plus `web_session_id`) so `assert.element`/text/visual work against the DOM, and add `assert.web_console` (no errors). No parallel assertion set.
- Gate launch/attach/interaction/mocking through the control consent gate and policy; keep endpoints loopback-only; WebView2 requires host opt-in (cannot be forced).
- Add tests: unit coverage for interception-rule matching, selector-to-action mapping, and capability routing; an integration test that launches Chrome with a debug port, navigates a local page, clicks/types, asserts DOM state, mocks an API response and asserts the page reflects it, and captures a console error; plus a WebView2 attach test.

## Phase 24: WebDriver-BiDi Backend (Firefox)

**Status:** Planned (follow-on to Phase 23).

Add a second `WebSession` backend so the same web tools drive Firefox via WebDriver BiDi, with no change to the tool surface.

- Add a `BidiSession` backend behind the Phase 23 `WebSession` abstraction, with Firefox launch/session setup (geckodriver/BiDi) and session/target/frame parity.
- Map interaction, DOM read, and waits onto BiDi (`browsingContext`, `script`, `input`); set capability flags and return `unsupported_for_protocol` for any features BiDi cannot cover.
- Provide network interception/mocking and console capture via the BiDi `network` and `log` domains; document any parity gaps versus CDP.
- Make the Phase 21/23 web assertions and `web.screenshot` work against BiDi sessions through the same surfaces.
- Keep launch/attach/interaction loopback-only and gated, consistent with Phase 23.
- Add Firefox/BiDi parity integration tests mirroring the Phase 23 suite (navigate, click/type, assert DOM, mock a response, capture a console error) plus capability-flag tests for unsupported features.

## Phase 25: MCP Prompt Library

**Status:** Planned.

Add a bundled library of MCP prompts so any client (Codex, Claude Desktop) surfaces ready-made, parameterized winctl workflows as slash-commands and the agent stays on the rails the server expects (find -> bind -> arm -> act -> verify, secret handling, fail-closed re-bind). Prompts are read-only, never mutate, and never echo secret values; each one references the exact tools and guardrails so it reinforces the conventions.

- Enable the prompts capability (`ServerCapabilities::builder().enable_tools().enable_prompts()`) and add an rmcp `#[prompt_router]` in an isolated `src/prompts.rs` module. Each prompt declares a name, description, and documented arguments and returns guidance messages.
- Support two prompt flavors: static playbooks that interpolate arguments into a tight instruction template (pure, no Win32), and context-aware scaffolders whose handler calls existing read-only providers (`list_windows`, UIA snapshot, `server.config`, crash reports) to embed current state, failing soft to the static text when state is unavailable (for example off-Windows).
- Drive & inspect: `winctl.drive_app(target)` (safe find/bind/arm/act/verify playbook; context lists candidate windows) and `winctl.inspect_ui(target)` (bind, snapshot, summarize the tree).
- Record & replay: `winctl.record_macro(app, title)` (guide the Phase 19 human-input recording flow) and `winctl.replay_macro(macro)` (run with `secret_ref` resolution and summarize).
- Author & run tests: `winctl.write_ui_test(app, scenario)` (scaffold a test manifest, pulling the UIA tree to suggest selectors), `winctl.run_tests(manifest|suite)` (run and summarize pass/fail, surfacing diffs/crashes), and `winctl.automate_app_testing(build_target, app)` (the build -> launch -> bind -> test -> report loop).
- Debug, diagnose & safety: `winctl.debug_crash(pid|app)` (orchestrate crash report, metrics, Event Log, last screenshot; context reads recent crash reports), `winctl.diagnose_hang(pid|app)`, `winctl.take_control(target)` (arm/notify/overlay consent flow), `winctl.secrets_setup(name)` (store a login secret via DPAPI without echoing it), and `winctl.setup()` (`server.config` orientation: enabled tools, capture dir, policy flags).
- Ship each prompt's playbook text now even when it references a not-yet-built feature (recorder, suites, web); the context-aware calls into those tools degrade to static guidance until the relevant phase lands, keeping this phase independent.
- Add `docs/PROMPTS.md` plus per-prompt entries and an INDEX section, and include the prompt list in startup diagnostics.
- Add tests: each prompt renders for valid arguments, errors on missing required arguments, context-aware prompts fall back to static when state is unavailable, and prompt output never contains a secret value; an integration test that lists all prompts and renders each via `prompts/get`.

## Not Planned Without Further Design

- Unguarded arbitrary shell execution.
- Title-only control actions.
- Kill-by-name or kill-by-title.
- Unauthenticated non-loopback control APIs.
- Broad filesystem, registry, network, or process mutation without explicit policy controls.
