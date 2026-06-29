# Autonomous Run Ledger

Status: Wave 2C complete; next wave is 3A agent/research lifecycle.
Created: 2026-06-29.
Plan: [`docs/v2/autonomous-implementation-plan.md`](../docs/v2/autonomous-implementation-plan.md).

This file is the mutable ledger for the long `/goal` implementation run. Parent
Codex owns updates.

## Baseline

- Current observed git state: implementation branch
  `codex/autonomous-v1-implementation`; baseline scaffold committed as
  `70ac1ad`; latest committed checkpoint `b771a5b`.
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
  hardening surfaces. Wave 1D added raw transport/offline self-description.
  Wave 2A adds typed `search`/`contents`, `/contents` chunking, and Phase 2
  gate changes. Wave 2B adds typed `answer`/`context`/`similar`, streaming
  SSE envelope shaping, context query validation, and strengthened Phase 2
  streaming gates. Wave 2C adds typed `team info`, legacy research
  create/list/get routing, research cursor auto-pagination, and the `ask`/
  `fetch` macro aliases with dry-run expansion metadata kept out of live data.

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
| 1D Raw/search/goldens | complete | Delegate Cursor Composer + parent integration | findings fixed; narrow re-review found final error-context redaction issue; fixed | findings fixed; re-review no blocking findings | `cargo xtask phase-gate 1` pass; `cargo xtask ci` pass |
| 2A Search/contents | complete in working tree | native request/chunk lane + Delegate Cursor Composer executor lane + parent integration | native found no-op `--chunk-size`, then chunked error-context gaps; fixed; final approval clean | GLM found clippy/context P3s; fixed; final approval clean | `cargo xtask phase-gate 2` pass; `cargo clippy --workspace --locked -- -D warnings` pass |
| 2B Answer/context/similar/streaming | complete in working tree | native answer/context/similar lane + Delegate Cursor Composer stream lane + parent integration | native found stream output-mode, terminal data shape, helper-only test, then context override bypass; fixed; final approval clean | GLM found context length guard and generic deprecation warning P3s; fixed; final approval clean | `cargo xtask ci` pass; `cargo xtask phase-gate 2` pass; branch-wide secret scan pass |
| 2C Team/macros/deprecations | complete in working tree | native baseline macro lane + Delegate Cursor Composer team/research lane + parent integration | initial/re-review findings fixed; final narrow re-review clean with only low-risk notes | initial findings fixed; final GLM re-review clean with accepted P3 residuals | `cargo xtask phase-gate 2` pass; `cargo xtask ci` pass; branch-wide secret scan pass |
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
| 1D | native checklist | high | `raw` lacked live transport/response envelope, `--raw` exact bytes, managed-header refusal, and offline schema/robot-doc commands | fixed | Added ureq/rustls transport seam, success response envelope, exact byte passthrough, auth header refusal, schema/robot-doc dispatch, and tests/gate. |
| 1D | native review | high | Documented `raw GET /search --body @q.json` path was parsed but rejected by live transport | fixed | Used ureq `force_send_body()` for GET/DELETE/OPTIONS with body; added GET-body raw test and phase-gate dry-run command. |
| 1D | native review | medium | Raw live error envelopes lacked operation/request context | fixed | Raw dispatch now wraps errors with method/path/requestId/correlationId; no-credential regression asserts the context. |
| 1D | native review | medium | Forbidden managed auth headers were accepted during `--dry-run --print-request` | fixed | Raw dispatch validates user headers before dry-run success; dry-run regression added. |
| 1D | native review | medium | Raw query preview redacted by key name only, not secret-shaped values | fixed | Non-secret query values now pass through shared `scrub_text`; UUID-shaped query regression added. |
| 1D | GLM review | P1 | `--retry-after` defaulted off despite contract default-on | fixed | Clap default is now true; parser regression asserts it. |
| 1D | GLM review | P1 | Body requests always appended `Content-Type: application/json`, overriding custom raw content types | fixed | Transport now adds default JSON content type only if the user did not provide one; regression added for custom content type preservation. |
| 1D | GLM review | P2 | `OPTIONS` was considered idempotent but unsupported by transport | fixed | Added OPTIONS transport path and retry-model regression. |
| 1D | GLM review | P2 | `emit_raw` ignored write errors and relied on drop flush | fixed | Raw writer now writes+flushes and maps failures to structured `interrupted`; no-newline writer regression added. |
| 1D | GLM review | P2 | 409s could not surface `idempotency_conflict` | fixed | 409 body heuristic now emits `idempotency_conflict` when idempotency is named; regression added. |
| 1D | GLM review | P3 | Phase 1 gate smoke commands only checked exit status | fixed | `xtask phase-gate 1` now parses stdout JSON and checks expected schema for each smoke command. |
| 1D | GLM review | P3 | `robot-docs errors` rebuilt full capabilities just to get error codes | fixed | Extracted `error_codes_json()` helper and reused it. |
| 1D | native re-review | medium | Raw error/context paths and malformed header/query validation could echo user-supplied secret material | fixed | Error context method/path/correlation are scrubbed and malformed header/query errors no longer echo raw input; regressions added. |
| 1D | GLM re-review | P3 | Error envelopes are pretty-printed even under `--compact`; upstream request ids, HTTP-date Retry-After, and backoff tuning remain limited | accepted for now | Non-blocking residuals; stderr remains parseable JSON, upstream IDs/backoff tuning are later transport hardening work. |
| 1D | parent | high | The amended Wave 1C commit plus a stale Delegate worktree branch still made the real stored key reachable in branch history | fixed | Amended `src/doctor.rs` fixture to a synthetic UUID and removed `cursor-7` via Delegate manager; branch-wide non-printing secret scan passes. |
| 1D | parent | medium | Supplied live API key returns 401 with both `x-api-key` and Bearer probes | open | Offline gates pass; final live smoke remains blocked until Trey provides a valid key or account access is restored. |
| 2A | native review | medium | `contents --chunk-size` was accepted but ignored and >100 inputs were not guarded | fixed | Implemented final-body `urls`/`ids` chunking, 1..100 guard, >100 rejection with suggested command, and per-chunk dry/live dispatch. |
| 2A | native re-review | medium | Chunked preflight/auth errors lost typed operation context; chunk live errors were stderr/abort rather than per-chunk NDJSON | fixed | Added chunk error-context wrapper and emitted chunk execution errors as NDJSON error envelopes before fail-fast exit; added missing-credential context regression. |
| 2A | GLM review | P3 | New typed live helper had a clippy `needless_borrow` and chunk/build validation errors lacked operation context | fixed | Removed needless borrow, added `with_typed_error_context` around `search`/`contents` build/validation, and asserted >100 guard context. |
| 2A | GLM/native residual | low | Single-chunk `--chunk-size` uses compact NDJSON shape; all-error `/contents` statuses exit 0; chunk transport errors fail fast | accepted for now | Matches current batch contract or is explicit fail-fast behavior; all-error promotion deferred until a future `--fail-on-url-error` decision. |
| 2B | native review | high | Streamed typed output keyed only on `--ndjson`, so `--format ndjson`, `EXA_OUTPUT=ndjson`, and default piped stream output missed event NDJSON | fixed | Added stream-specific output-mode resolution with explicit/env/piped defaults and integrated fake-transport tests. |
| 2B | native review | high | Terminal streamed `answer` response exposed raw SSE chunk arrays instead of upstream-shaped `data.answer`/`data.citations` | fixed | Added `terminal_stream_data` to prefer final answer objects and concatenate OpenAI-style delta chunks; stream tests now assert terminal response shape. |
| 2B | native review | medium | Stream shape test manually duplicated helper code rather than exercising the typed live path | fixed | Made typed live execution generic over the transport trait and added no-network fake-transport coverage for stream dispatch. |
| 2B | GLM review | P3 | `context` query length guard required by commands contract was missing | fixed | Added 2000-character local validation and positional/body/set CLI regressions with `/context` error context. |
| 2B | GLM review | P3 | Deprecated warning helper hardcoded findSimilar prose for every future deprecated operation | fixed | Scoped findSimilar replacement warning to `findSimilar`/`similar`; added generic fallback and unit coverage. |
| 2B | native re-review | medium | `/context` length validation initially ran before `--body`/`--set` request overrides, making it bypassable | fixed | Moved validation to the final merged request body and added regressions for positional, `--body`, and `--set` overlong queries. |
| 2C | native review | medium | Typed GET dry-run previews emitted `{}` bodies while live GETs omit empty bodies | fixed | Added `typed_wire_body` so GET/DELETE dry-run previews show `body:null`; added team/research regressions. |
| 2C | native/GLM review | medium | `fetch` macro expansion omitted the documented contents defaults and `ask`/`fetch` expansion strings were not shell-safe | fixed | `fetch` now expands to `contents ... --text --summary-query 'Summarize the page'`; macro `expandsTo` strings use shell quoting and apostrophe regressions. |
| 2C | GLM review | high | Live `ask`/`fetch` responses could include macro-only `expandsTo` metadata, changing `dataHash` from the upstream response | fixed | Live macro dispatch no longer passes expansion metadata into typed execution; black-box local-server tests assert upstream data/hash purity for both macros. |
| 2C | native/GLM review | medium | `research list --all` and pagination flags either drifted from the documented contract or accepted inert flags | fixed | Implemented live cursor pagination for `--all` with `--max-pages`/`--page-delay`, rejected orphaned pagination flags, and added fake-transport/unit plus CLI regressions. |
| 2C | native review | low | Local HTTP regression helper could fail on `WouldBlock` before request bytes arrived | fixed | Helper now retries nonblocking reads up to a bounded deadline. |
| 2C | GLM review | P3 | `data_with_expands_to` had an unreachable non-object fallback and fetch lacked a live no-pollution regression | fixed | Removed unreachable branch and added the fetch local-server no-pollution test. |
| 2C | GLM final review | P3 | Dry-run macro metadata exposes both `expandsTo` and `expands_to`; `--text` booleans are simple presence flags; `research create --stream` errors because legacy create is non-streaming | accepted for now | Duplicate key keeps camel/snake compatibility; boolean flag shape matches existing CLI convention; create-stream refusal is structured and tested. |

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
| 2026-06-29 | `cargo test --test transport --test cli --test redaction --test doctor --locked` | pass | Wave 1D focused raw/offline self-description tests |
| 2026-06-29 | `cargo xtask phase-gate 1` | pass | Strengthened Phase 1 gate: tests plus capabilities/schema/robot-docs/raw/search dry-runs |
| 2026-06-29 | `cargo xtask ci` | pass | Wave 1D pre-review gate; fmt, clippy, tests |
| 2026-06-29 | `cargo test --test cli --test transport --locked` | pass | After native review fixes |
| 2026-06-29 | `cargo xtask ci` | pass | After native review fixes |
| 2026-06-29 | `cargo test --test cli --test transport --locked` | pass | After GLM review fixes |
| 2026-06-29 | `cargo xtask phase-gate 1` | pass | After GLM review fixes; smoke commands assert schemas |
| 2026-06-29 | `cargo xtask ci` | pass | After GLM review fixes |
| 2026-06-29 | `cargo test --test cli --test transport --locked` | pass | After native narrow re-review error-redaction fix |
| 2026-06-29 | `cargo xtask phase-gate 1` | pass | After native narrow re-review error-redaction fix |
| 2026-06-29 | `cargo xtask ci` | pass | After native narrow re-review error-redaction fix |
| 2026-06-29 | non-printing branch-wide secret scan | pass | Stored API key absent from tracked files, diff, reachable branch commits after commit amend and Delegate worktree removal |
| 2026-06-29 | non-printing live auth probe | blocked | Stored API key produced structured `reauth_required`/401 for `/websets/v0/teams/me`; output did not contain secret |
| 2026-06-29 | `cargo test --test cli --test request --test transport --locked` | pass | Wave 2A targeted typed search/contents/request/transport tests before review fixes |
| 2026-06-29 | `cargo xtask phase-gate 2` | pass | Wave 2A pre-review gate; includes `search` and `contents` dry-run smokes |
| 2026-06-29 | `cargo test --test cli --test redaction --locked` | pass | After native/GLM Wave 2A review fixes |
| 2026-06-29 | `cargo clippy --workspace --locked -- -D warnings` | pass | After GLM P3 fix; no clippy warnings |
| 2026-06-29 | `cargo xtask phase-gate 2` | pass | Wave 2A final gate after native + GLM review-fix loop |
| 2026-06-29 | `cargo xtask ci` | pass | Wave 2A final pre-commit gate |
| 2026-06-29 | non-printing branch-wide secret scan | pass | Stored API key absent from tracked files, diff, and reachable branch commits |
| 2026-06-29 | `delegate worktree remove cursor-8 --discard-uncommitted --force-branch` | pass | Integrated Cursor worktree cleaned via Delegate manager |
| 2026-06-29 | `cargo test --test request`; `cargo test --test transport streaming_ndjson_shape_from_canned_sse -- --exact`; `cargo test --test cli dry_run` | pass | Wave 2B focused typed request/dry-run/stream checks before review |
| 2026-06-29 | `cargo xtask ci`; `cargo xtask phase-gate 2` | pass | Wave 2B pre-review gate after answer/context/similar + streaming integration |
| 2026-06-29 | `cargo xtask ci`; `cargo xtask phase-gate 2` | pass | After native + GLM Wave 2B streaming/context/deprecation fixes |
| 2026-06-29 | `cargo test --test cli context_rejects_queries_over_two_thousand_chars -- --exact`; `cargo xtask ci`; `cargo xtask phase-gate 2` | pass | After final native re-review context override-bypass fix |
| 2026-06-29 | native final Wave 2B re-review | pass | Clean; verified positional/body/set context length errors and direct probes |
| 2026-06-29 | Delegate GLM final Wave 2B re-review (`droid-18`) | pass | Clean; verified all five reported findings fixed; ran full validation in safe worktree |
| 2026-06-29 | `delegate worktree remove cursor-9 --discard-uncommitted --force-branch` | pass | Integrated Cursor worktree cleaned via Delegate manager |
| 2026-06-29 | non-printing tracked/diff/history secret scan | pass | Stored API key absent from tracked files, working diff, and reachable branch blobs |
| 2026-06-29 | focused Wave 2C tests (`research_accepts_all...`, macro live no-pollution, `paginated_research...`, clippy) | pass | After native/GLM review fixes for macro purity, pagination, and local-server stability |
| 2026-06-29 | native Wave 2C final re-review (`019f15c2-c566-7a72-be07-bb0e74124c4a`) | pass | No blocking findings; low-risk NDJSON warning repetition and Content-Length-only helper notes accepted |
| 2026-06-29 | Delegate GLM final Wave 2C re-review (`droid-22`) | pass | No blocking findings; accepted P3 residuals documented above |
| 2026-06-29 | `cargo xtask phase-gate 2` | pass | Wave 2C final gate; includes team/research dry-runs and full workspace tests |
| 2026-06-29 | `cargo xtask ci` | pass | Wave 2C final gate; fmt, clippy, tests |
| 2026-06-29 | non-printing tracked/diff/history secret scan | pass | Stored API key fingerprint `927c` absent from tracked files, working diff, and reachable branch blobs |
| 2026-06-29 | `delegate worktree remove cursor-10 --discard-uncommitted --force-branch` | pass | Integrated Cursor worktree cleaned via Delegate manager; no Delegate worktrees remain present |

## Local commit log

| Date | Commit | Scope | Checks |
|---|---|---|---|
| 2026-06-29 | `70ac1ad` | Baseline scaffold + autonomous plan | `cargo xtask ci` |
| 2026-06-29 | `4a323df` | Wave 1A parser contract surface | `cargo xtask ci`; native review clean; GLM review clean |
| 2026-06-29 | `c226444` | Wave 1B request merge and redaction spine | `cargo xtask ci`; native review clean; GLM review clean |
| 2026-06-29 | `a3dc0dd` | Wave 1C auth, config, and doctor surfaces | `cargo xtask ci`; native review clean; GLM review clean; credential smokes pass; real-key fixture purged by amend |
| 2026-06-29 | `d420fe0` | Wave 1D raw transport, response envelope, offline schema/robot-docs, and Phase 1 gate | `cargo xtask phase-gate 1`; `cargo xtask ci`; native + GLM reviews/re-reviews; branch-wide secret scan pass; live smoke blocked by credential 401 |
| 2026-06-29 | `6f12e42` | Wave 2A typed `search`/`contents`, `/contents` chunking, redaction suggestion fix, and Phase 2 gate | `cargo xtask phase-gate 2`; `cargo clippy --workspace --locked -- -D warnings`; native + GLM final reviews clean |
| 2026-06-29 | `b771a5b` | Wave 2B typed `answer`/`context`/`similar`, SSE stream envelope shaping, context query validation, and Phase 2 stream gate | `cargo xtask ci`; `cargo xtask phase-gate 2`; native + GLM final reviews clean; branch-wide secret scan pass |
| 2026-06-29 | this commit | Wave 2C typed `team info`, legacy research create/list/get with cursor pagination, `ask`/`fetch` macro aliases, and Phase 2 team/research gate | `cargo xtask ci`; `cargo xtask phase-gate 2`; native + GLM final reviews clean; branch-wide secret scan pass |

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
- [ ] `cargo run -- raw GET /websets/v0/teams/me --dry-run --print-request --json`
- [ ] final native review clean
- [ ] final GLM review clean
- [ ] no unreviewed Delegate worktrees
- [ ] final git tree clean or intentional artifacts documented
