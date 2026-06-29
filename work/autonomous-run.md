# Autonomous Run Ledger

Status: ready for implementation; baseline git decision and live smoke credentials
still pending.
Created: 2026-06-29.
Plan: [`docs/v2/autonomous-implementation-plan.md`](../docs/v2/autonomous-implementation-plan.md).

This file is the mutable ledger for the long `/goal` implementation run. Parent
Codex owns updates.

## Baseline

- Current observed git state: scaffold and docs are untracked.
- Current verified checks:
  - `cargo test --workspace --locked`
  - `cargo xtask ci`
- Delegate availability verified with non-printing `delegate --json describe` and
  `delegate --json models`.
- Delegate Cursor Composer work mode verified after local command collision fix;
  smoke artifact: `work/delegate-cursor-composer-smoke.md`.
- Implementation has not started.

## Pre-run checklist

- [ ] Resolve baseline git state: commit current scaffold or confirm dirty tree
      as the intended baseline.
- [x] Confirm Delegate lanes with `delegate --json describe`.
- [x] Confirm model roster with `delegate --json models`.
- [x] Confirm Delegate Cursor Composer work-mode smoke.
- [ ] Confirm whether live smoke may use `EXA_API_KEY`.
- [x] Run `cargo test --workspace --locked`.
- [ ] Confirm local commit baseline. After that, parent Codex should commit
      coherent passing checkpoints locally and never push unless Trey asks.

## Wave ledger

| Wave | Status | Implementation lanes | Native review | GLM review | Gate |
|---|---|---|---|---|---|
| 0 Baseline/spec audit | not started | - | - | - | - |
| 1A Registry/parser/envelope | not started | - | - | - | - |
| 1B Request/redaction/body merge | not started | - | - | - | - |
| 1C Auth/config/doctor | not started | - | - | - | - |
| 1D Raw/search/goldens | not started | - | - | - | - |
| 2A Search/contents | not started | - | - | - | - |
| 2B Answer/context/similar/streaming | not started | - | - | - | - |
| 2C Team/macros/deprecations | not started | - | - | - | - |
| 3A Agent/research lifecycle | not started | - | - | - | - |
| 3B SSE/SIGINT/pagination | not started | - | - | - | - |
| 4A Monitors | not started | - | - | - | - |
| 4B Websets core | not started | - | - | - | - |
| 4C Websets webhooks/events/closeout | not started | - | - | - | - |
| 5 Admin keys | not started | - | - | - | - |
| 6 Ergonomics/smoke/release | not started | - | - | - | - |

## Finding log

Record every review finding that is not immediately fixed.

| Wave | Reviewer | Severity | Finding | Disposition | Rationale / fix |
|---|---|---|---|---|---|

## Gate log

| Date | Command | Result | Notes |
|---|---|---|---|
| 2026-06-29 | `cargo test --workspace --locked` | pass | 6 registry tests |
| 2026-06-29 | `cargo xtask ci` | pass | fmt, clippy, tests |
| 2026-06-29 | `delegate --json describe`; `delegate --json models` | pass | output suppressed |
| 2026-06-29 | `delegate cursor work` | pass | wrote `work/delegate-cursor-composer-smoke.md` |

## Local commit log

No implementation commits yet.

| Date | Commit | Scope | Checks |
|---|---|---|---|

## Final completion checklist

- [ ] `cargo xtask ci`
- [ ] `cargo xtask phase-gate 1`
- [ ] `cargo xtask phase-gate 2`
- [ ] `cargo xtask phase-gate 3`
- [ ] `cargo xtask phase-gate 4`
- [ ] `cargo xtask phase-gate 5`
- [ ] `cargo xtask phase-gate 6`
- [ ] `cargo xtask smoke --budget "$EXA_E2E_BUDGET"` with real `EXA_API_KEY`
- [ ] `cargo run -- capabilities --json`
- [ ] `cargo run -- search "test query" --dry-run --print-request --json`
- [ ] `cargo run -- raw GET /v0/teams/me --dry-run --print-request --json`
- [ ] final native review clean
- [ ] final GLM review clean
- [ ] no unreviewed Delegate worktrees
- [ ] final git tree clean or intentional artifacts documented
