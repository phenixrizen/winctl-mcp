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

- Window enumeration, find, bind, describe, focus, monitor listing, and point diagnostics.
- Bound-window click, text input, window screenshots, and display screenshots.
- Process launch, list, describe, tracked kill, and wait-for-window flows.
- Streamable HTTP and stdio MCP transports with stderr/log-file logging.
- Self-test command, Windows integration harness, and WSL cross-build support.

## Phase 1: Interaction Coverage

- Add scroll, mouse move, drag, double-click, keyboard shortcuts, key down/up, and action delays.
- Add richer wait conditions for window state, foreground status, title/class changes, image checks, and process lifecycle.
- Preserve bound-target identity checks before every control action.
- Return replay-safe metadata for timing, coordinate space, resolved screen position, and preflight target.

## Phase 2: UI Automation Snapshot Layer

- Capture UI Automation trees with element names, roles, automation IDs, bounds, enabled/focused state, and hierarchy.
- Add stable element references that can be revalidated before element-targeted actions.
- Add element diagnostics for inaccessible, virtualized, offscreen, disabled, and duplicate elements.
- Keep screenshot-first capture available for visual workflows and fallback diagnostics.

## Phase 3: App and Window Management

- Add richer app launch modes for packaged apps, Start Menu entries, protocol handlers, and working-directory presets.
- Add window move, resize, minimize, maximize, restore, close, and foreground diagnostics.
- Add multi-window app handling with explicit candidate lists and child-process window policy.
- Keep PID, HWND, executable, and launch-session identity at the center of every control decision.

## Phase 4: Browser-Aware Automation

- Add browser state capture for Chrome, Edge, and Firefox.
- Track browser process, profile, window, and tab identity separately.
- Prefer explicit browser/session identity over tab-title matching.
- Support browser-focused assertions, navigation waits, screenshot checkpoints, and content extraction.

## Phase 5: System Utility Tools

- Add guarded clipboard read/write tools.
- Add filesystem read, list, search, copy, move, delete, and artifact export with allowlist policy.
- Add notification inspection, registry read/list/write/delete with opt-in configuration, and additional process diagnostics.
- Keep destructive operations disabled or denied unless explicitly configured.

## Phase 6: Network and Content Tools

- Add guarded fetch and scrape utilities with scheme restrictions, timeout limits, response-size limits, redirect controls, and private-network protections.
- Keep network tools separate from local Windows UI control tools.
- Log request intent, destination, response metadata, and truncation/blocked reasons.

## Phase 7: Security and Configuration

- Add a first-class config file for transports, auth, tool exposure, logging, filesystem roots, and destructive-tool policy.
- Add tool allowlists/denylists, auth token management, optional TLS, CORS policy, and IP allowlists.
- Add startup diagnostics that explain which tools are exposed and why.
- Preserve loopback-only HTTP defaults.

## Phase 8: Distribution and Client Setup

- Add packaged releases with checksums and version metadata.
- Add installer/update flow for the Windows binary.
- Add MCP client configuration examples for Codex, Claude Desktop, and other compatible clients.
- Add troubleshooting scripts for transport startup, Windows permissions, capture availability, and target discovery.

## Phase 9: Recorder and Replay UI

- Add a local UI for inspecting windows, binding targets, recording interactions, and previewing screenshots.
- Record click offsets, coordinate spaces, typing, shortcuts, waits, screenshots, and target identity.
- Let users edit recorded steps before saving.
- Generate stable replay manifests instead of opaque scripts.

## Phase 10: Test Manifest Standard

- Define a versioned YAML or JSON manifest for UI tests.
- Include app launch, binding strategy, preconditions, actions, waits, assertions, screenshots, cleanup, and artifact paths.
- Add CLI and MCP commands to validate, dry-run, run, and export replay results.
- Store enough metadata to replay across monitor layouts and diagnose target mismatches.

## Not Planned Without Further Design

- Unguarded arbitrary shell execution.
- Title-only control actions.
- Kill-by-name or kill-by-title.
- Unauthenticated non-loopback control APIs.
- Broad filesystem, registry, network, or process mutation without explicit policy controls.
