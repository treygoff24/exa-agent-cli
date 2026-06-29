# Autonomous Run Ledger

Status: Wave 1C complete after native + GLM review; local commit pending.
Created: 2026-06-29.
Plan: [`docs/v2/autonomous-implementation-plan.md`](../docs/v2/autonomous-implementation-plan.md).

This file is the mutable ledger for the long `/goal` implementation run. Parent
Codex owns updates.

## Baseline

- Current observed git state: implementation branch
  `codex/autonomous-v1-implementation`; baseline scaffold committed as
  `70ac1ad`.
- Current verified checks:
  - `cargo test --workspace --locked`
  - `cargo xtask ci`
  - `cargo xtask vendor-spec --check`
- Delegate availability verified with non-printing `delegate --json describe` and
  `delegate --json models`.
- Delegate Cursor Composer work mode verified after local command collision fix;
  smoke artifact: `work/delegate-cursor-composer-smoke.md`.
- Live smoke credential available from
  `~/.config/exa-agent-cli/credentials.json` (fingerprint `64c321d8ab24`; do
  not print full key).
- Implementation started. Wave 1A expanded the typed parser surface and
  not-implemented envelope routing. Wave 1B added request merge/redaction spine.
  Wave 1C added local auth, non-secret config, doctor, and contract/error
  hardening surfaces.

## Pre-run checklist

- [x] Resolve baseline git state: commit current scaffold or confirm dirty tree
      as the intended baseline.
- [x] Confirm Delegate lanes with `delegate --json describe`.
- [x] Confirm model roster with `delegate --json models`.
- [x] Confirm Delegate Cursor Composer work-mode smoke.
- [x] Confirm whether live smoke may use `EXA_API_KEY`.
- [x] Run `cargo test --workspace --locked`.
- [x] Confirm local commit baseline. After that, parent Codex should commit
      coherent passing checkpoints locally and never push unless Trey asks.

## Wave ledger

| Wave | Status | Implementation lanes | Native review | GLM review | Gate |
|---|---|---|---|---|---|
| 0 Baseline/spec audit | complete | parent + native map | n/a | n/a | `cargo xtask vendor-spec --check` pass |
| 1A Registry/parser/envelope | complete | Delegate Cursor Composer + parent integration | native reviewer found `raw --query` preview omission; fixed; re-review clean | GLM review clean; P3 redaction hardening fixed | `cargo xtask ci` pass |
| 1B Request/redaction/body merge | complete | native redaction lane + Delegate Cursor Composer request lane + parent integration | native found `--set` numeric-index panic/OOM risk; fixed; re-review clean | GLM review clean; P3 raw query redaction fixed; narrow re-review clean | `cargo xtask ci` pass |
| 1C Auth/config/doctor | complete | native auth lane + Delegate Cursor Composer config/doctor lane + parent integration | native found secret-env config, stdin flag, doctor warning/check, URL, and logout issues; fixed; re-reviews clean | GLM found error-code contract drift and empty config path guard; fixed; narrow re-review clean | `cargo xtask ci` pass; non-printing stored-credential smokes pass |
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
| 1A | native | medium | `raw --query` parsed but omitted from dry-run/print-request preview | fixed | Added ordered query preview array plus black-box output test. |
| 1A | GLM | P3 | Debug redaction missed service-key style header names and command payloads could leak via `Cli` debug | fixed | Broadened secret-name matcher; custom `Cli` debug now prints command path, not command arg payloads. |
| 1B | native | high | numeric `--set` path segments could overflow or allocate unbounded arrays | fixed | Added array-index cap, checked length, and CLI/request regressions. |
| 1B | GLM | P3 | secret-ish raw `--query` values leaked in dry-run preview | fixed | Redact query values by shared secret-name predicate; added black-box regression. |
| 1C | native | high | `profiles.*.*_key_env` accepted arbitrary values, allowing plaintext key storage in config | fixed | Env indirection values now must be env-var identifiers and reject API-shaped values; added config + CLI regressions. |
| 1C | native | medium | `--api-key-stdin`/`--service-key-stdin` combinations could conflict or consume stdin surprisingly | fixed | Added Clap conflicts between explicit/stdin and dual stdin secret flags; parser regressions added. |
| 1C | native | medium | `doctor` treated warn findings as healthy and silently accepted unknown `--check` IDs | fixed | Warnings now produce `findings`/exit 1; unknown checks return structured `invalid_value`; regressions added. |
| 1C | native | medium | Base URL validation accepted malformed `https://` strings; logout ignored deletion failure | fixed | Shared stricter URL validator with doctor; credential deletion errors now propagate. |
| 1C | native | low | Unused `Config::path_json` helper | fixed | Deleted unused helper. |
| 1C | GLM | P2 | Error-code dictionary advertised `partial_success` instead of contract `partial_batch` and missed three contract codes | fixed | Added `partial_batch`, `upstream_malformed`, `concurrency_limit`, `idempotency_conflict`; capabilities regressions added. |
| 1C | GLM | P3 | `config_path()` used empty `EXA_AGENT_CONFIG`/`XDG_CONFIG_HOME` instead of falling through | fixed | Mirrored credentials path empty-env guards; regression added. |
| 1C | parent | high | Real stored API-key string was accidentally used as a UUID redaction test fixture | fixed | Replaced with synthetic UUID fixture; non-printing secret scan now passes. |

## Gate log

| Date | Command | Result | Notes |
|---|---|---|---|
| 2026-06-29 | `cargo test --workspace --locked` | pass | 6 registry tests |
| 2026-06-29 | `cargo xtask ci` | pass | fmt, clippy, tests |
| 2026-06-29 | `delegate --json describe`; `delegate --json models` | pass | output suppressed |
| 2026-06-29 | `delegate cursor work` | pass | wrote `work/delegate-cursor-composer-smoke.md` |
| 2026-06-29 | `cargo xtask vendor-spec --check` | pass | Wave 0 embedded spec audit |
| 2026-06-29 | `cargo test --test cli --locked` | pass | 20 parser/output tests after review fix |
| 2026-06-29 | `cargo xtask ci` | pass | Wave 1A final gate after native + GLM fixes |
| 2026-06-29 | `cargo test --test request --test redaction --test cli --locked` | pass | Wave 1B focused tests after integration |
| 2026-06-29 | `cargo xtask ci` | pass | Wave 1B final gate after native + GLM fixes |
| 2026-06-29 | `cargo test --test cli --test redaction --test auth --test doctor --test config --locked` | pass | Wave 1C focused auth/config/doctor/redaction tests before review fixes |
| 2026-06-29 | `cargo xtask ci` | pass | Wave 1C gate before native/GLM review-fix loop |
| 2026-06-29 | `cargo test --test cli --test config --test doctor --test auth --test redaction --locked` | pass | After native Wave 1C fixes |
| 2026-06-29 | `cargo xtask ci` | pass | After native Wave 1C fixes |
| 2026-06-29 | `cargo test --test config --test registry --test cli --test doctor --locked` | pass | After GLM Wave 1C contract/path fixes |
| 2026-06-29 | `cargo xtask ci` | pass | Wave 1C final gate after native + GLM fixes |
| 2026-06-29 | non-printing `auth status` stored-credential smoke | pass | API env removed in subprocess; authenticated from credentials file; output did not contain secret |
| 2026-06-29 | non-printing `doctor --check key.present` smoke | pass | Stored credential seen; output did not contain secret |
| 2026-06-29 | `delegate worktree remove cursor-6 --discard-uncommitted --force-branch` | pass | Integrated Cursor worktree cleaned via Delegate manager |
| 2026-06-29 | non-printing tracked-file/diff secret scan | pass | Stored API key absent from tracked files and diff after fixture replacement |
| 2026-06-29 | `cargo xtask ci` | pass | Wave 1C final gate after secret fixture replacement |

## Local commit log

| Date | Commit | Scope | Checks |
|---|---|---|---|
| 2026-06-29 | `70ac1ad` | Baseline scaffold + autonomous plan | `cargo xtask ci` |
| 2026-06-29 | `4a323df` | Wave 1A parser contract surface | `cargo xtask ci`; native review clean; GLM review clean |
| 2026-06-29 | `c226444` | Wave 1B request merge and redaction spine | `cargo xtask ci`; native review clean; GLM review clean |
| 2026-06-29 | this commit | Wave 1C auth, config, and doctor surfaces | `cargo xtask ci`; native review clean; GLM review clean; credential smokes pass |

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
