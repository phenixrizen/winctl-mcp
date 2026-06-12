# CLAUDE.md

Canonical agent guidance for this repository lives in `AGENTS.md`. Read it and follow it for
every task — project intent, crate layout, dashboard stack, operational rules, and especially
the **Commit and versioning conventions**.

Key reminders (full detail in `AGENTS.md`):
- Use Conventional Commits (`feat:`, `fix:`, `docs:`, `ci:`, `chore:`, …) — one-line subject,
  no parenthetical scope, no `Co-Authored-By:` trailer.
- Commit types drive automated SemVer releases in CI (`feat` → minor, `fix`/`perf` → patch,
  `!`/`BREAKING CHANGE:` → major; `docs`/`chore`/`ci`/`test`/`style`/`refactor`/`build` → no
  release). Pick the type for its release effect.
- Do NOT bump `version` in `Cargo.toml`; tags `vX.Y.Z` are the source of truth.

@AGENTS.md
