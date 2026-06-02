# UI Automation Action Layer — Design

[Back to tool index](INDEX.md)

Status: **Proposed (Roadmap Phase 11)**

## Goal

Today the UIA layer is read-only: `uia.snapshot`, `uia.find`, and `uia.resolve`
describe the tree but cannot act on it. Every interaction therefore routes
through coordinate input (`input.click`, `input.type_text`), which is fragile
across DPI changes, themes, scroll position, and layout shifts.

This phase adds an **action layer** that drives elements through their UI
Automation control patterns — the same mechanism assistive technologies and
Microsoft's own test tooling use. Actions target a revalidated `element_ref`
rather than a pixel, so "click the *Save* button" stays correct when the button
moves.

## Design invariants

Each action tool follows the existing `uia.*` contract in
`crates/winctl-mcp-server/src/tools/uia.rs`:

1. **Revalidate the bound window** via `state.revalidate_bound_window(&bound_id)`
   (HWND + PID + executable identity) before touching the tree.
2. **Take a fresh snapshot** (`ui_automation_snapshot`) — never act on a cached
   tree. Element positions and pattern availability can change between calls.
3. **Resolve the `element_ref`** (`resolve_ui_element_ref`) against that fresh
   snapshot. A stale or missing reference fails closed.
4. **Verify the element supports the requested control pattern** and is enabled
   and on-screen. Unsupported pattern / disabled / offscreen all fail closed
   with an explicit reason — never silently fall back to a coordinate click.
5. **Perform the pattern action**, then return structured
   `{"ok": true, "before": ..., "after": ..., "pattern": ..., "bounds": ...}`.
   `bounds` is included as a **coordinate-fallback hint** so a caller can record
   a visual-replay step, but the tool itself does not click coordinates.

All actions are control actions, gated by the same control-action policy as
`input.*` (not the filesystem/registry "mutation" flags). Every action is logged
with `bound_id`, `element_ref`, pattern, and outcome.

## Tools and control-pattern mapping

| Tool | UI Automation pattern | Targets | Notes |
| --- | --- | --- | --- |
| `uia.invoke` | `IUIAutomationInvokePattern::Invoke` | buttons, links, menu items | Single, non-stateful activation. |
| `uia.set_value` | `IUIAutomationValuePattern::SetValue` (fallback `LegacyIAccessiblePattern`) | edit fields, spin boxes, combo edits | Rejects if `IsReadOnly`. Prefer over `type_text` for atomic, locale-safe entry. |
| `uia.get_value` | `ValuePattern::CurrentValue` → `TextPattern` → `Name` | any value-bearing element | Read-only; used by assertions and verification. |
| `uia.toggle` | `IUIAutomationTogglePattern::Toggle` | checkboxes, toggle buttons | Returns pre/post `ToggleState`; supports an optional desired-state target to make it idempotent. |
| `uia.expand_collapse` | `IUIAutomationExpandCollapsePattern::Expand` / `Collapse` | tree items, combo boxes, menus, expanders | `action`: `expand` \| `collapse` \| `toggle`. |
| `uia.select` | `IUIAutomationSelectionItemPattern::Select` / `AddToSelection` / `RemoveFromSelection` | list items, radio buttons, tabs, grid cells | `mode`: `replace` \| `add` \| `remove`. |
| `uia.set_focus` | `IUIAutomationElement::SetFocus` | any focusable element | Useful before keyboard-driven steps. |
| `uia.range_value` | `IUIAutomationRangeValuePattern::SetValue` | sliders, progress, scrollbars | Clamps to `Minimum`/`Maximum`; returns the achieved value. |
| `uia.scroll_into_view` | `IUIAutomationScrollItemPattern::ScrollIntoView` | items inside scrollable containers | Brings an offscreen element on-screen before another action. |
| `uia.wait_for_element` | snapshot polling (no pattern) | a `UiElementSelector` | Wait for appear / enabled / value-equals / value-changed, with `timeout_ms` and `poll_interval_ms`. |

## Request shapes

All action tools accept either an `element_ref` (preferred, from a prior
snapshot/find) **or** an inline `UiElementSelector` that is resolved against the
fresh snapshot. `UiElementSelector` already exists in `crates/winctl/src/uia.rs`
(`element_ref`, `name`, `role`, `automation_id`, `class_name`, `text_contains`,
`include_offscreen`) and is reused unchanged.

```jsonc
// uia.invoke
{ "bound_id": "...", "element_ref": "..." }

// uia.set_value
{ "bound_id": "...", "selector": { "automation_id": "username" }, "value": "alice" }

// uia.toggle  (idempotent form)
{ "bound_id": "...", "element_ref": "...", "desired_state": "on" }

// uia.expand_collapse
{ "bound_id": "...", "element_ref": "...", "action": "expand" }

// uia.select
{ "bound_id": "...", "selector": { "name": "Settings", "role": "tab_item" }, "mode": "replace" }

// uia.range_value
{ "bound_id": "...", "element_ref": "...", "value": 0.75 }

// uia.wait_for_element
{ "bound_id": "...", "selector": { "automation_id": "spinner", "include_offscreen": false },
  "condition": "disappears", "timeout_ms": 5000, "poll_interval_ms": 150 }
```

## Response shape

```jsonc
{
  "ok": true,
  "pattern": "toggle",
  "element": { "element_ref": "...", "automation_id": "remember_me", "role": "check_box" },
  "before": { "toggle_state": "off" },
  "after":  { "toggle_state": "on" },
  "bounds": { "x": 412, "y": 588, "width": 18, "height": 18 },   // coordinate-fallback hint
  "diagnostics": []
}
```

Failure is the standard `{"ok": false, "error": {...}}` with a `code` that
distinguishes the closed-fail reasons so callers and the macro engine can branch:

- `bound_window_revalidation_failed`
- `element_ref_stale` / `element_not_found`
- `pattern_not_supported` (include the element's available patterns in the error)
- `element_disabled`
- `element_offscreen` (suggest `uia.scroll_into_view`)
- `value_read_only`

## Implementation notes

- Pattern objects are obtained from `IUIAutomationElement::GetCurrentPattern(<PATTERN_ID>)`;
  a null result means "not supported" → fail closed. This requires a live COM
  element handle, so the snapshot resolver in `winctl/src/uia.rs` needs to return
  (or be able to re-acquire) the `IUIAutomationElement` for a resolved `path`,
  not just the serialized `UiElementInfo`. Re-walking the cached `path: Vec<usize>`
  from the automation root on a fresh snapshot is the safest way to get a live
  handle that matches the validated reference.
- COM is already initialized `COINIT_MULTITHREADED` for the snapshot path; reuse it.
- Keep actions **single-pattern and explicit**. Do not compose (e.g. expand +
  select) inside one tool — the macro/recorder layer composes steps, and atomic
  tools keep replay manifests legible and diagnosable.

## Why this is the highest-leverage addition

- Converts the existing read-only tree into reliable, layout-independent control.
- Removes the largest source of flaky GUI tests (pixel math under DPI/theme drift).
- Composes directly into the recorder, macro manifest, and test manifest systems
  that already exist — each action is a clean, revalidated, replay-safe step.
- Pairs with Phase 12 (`assert.*`, OCR, visual diff) to close the
  act → verify loop for GUI application testing.
