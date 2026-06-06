# assert.element

[Back to tool index](INDEX.md)

Agent name: `winctl-mcp`

Description: Assert UI Automation element presence and rich state predicates against a revalidated bound window.

## Inputs

- `bound_id`: bound window ID.
- `element_ref` or `selector`: target element.
- `expect`: `present` or `absent`; `exists: false` is treated as absent.
- `negate`: invert the final result.
- `timeout_ms`, `poll_interval_ms`: poll until the assertion passes or times out.
- `exists`, `enabled`, `focused`, `checked`, `selected`, `expanded`: optional state assertions.
- `name`, `name_contains`, `name_regex`: accessible-name assertions.
- `value`, `value_contains`, `value_regex`, `editable`, `readonly`: ValuePattern assertions. Compared values are redacted in results.
- `role`, `control_type_id`, `offscreen`, `bounds`, `bounds_tolerance`: semantic and geometry assertions.
- `count`: exact number of matching elements. Pattern-state predicates require a single element and should be asserted separately from count.
- `max_depth`, `max_elements`: optional snapshot limits.

## Notes

Read-only; no armed control session is needed. The response follows the shared assertion contract: `ok`, `passed`, `negated`, `expected`, `actual`, `predicate`, `target`, `elapsed_ms`, and `diagnostics`. Assertion failures return `ok: false`.
