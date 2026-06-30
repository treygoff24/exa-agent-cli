# Autonomous Run Ledger

Status: Wave 6 complete; all final validation gates pass and this commit records
the closeout ledger.
Created: 2026-06-29.
Plan: [`docs/v2/autonomous-implementation-plan.md`](../docs/v2/autonomous-implementation-plan.md).

This file is the mutable ledger for the long `/goal` implementation run. Parent
Codex owns updates.

## Baseline

- Current observed git state: implementation branch
  `codex/autonomous-v1-implementation`; baseline scaffold committed as
  `70ac1ad`; latest committed checkpoint before Wave 6 was `e0e4c3f`.
- Current verified checks:
  - `cargo test --workspace --locked`
  - `cargo xtask ci`
  - `cargo xtask vendor-spec --check`
- Delegate availability verified with non-printing `delegate --json describe` and
  `delegate --json models`.
- Delegate Cursor Composer work mode verified after local command collision fix;
  smoke artifact: `work/delegate-cursor-composer-smoke.md`.
- Current implementation/review lane policy: use native Codex subagents,
  Delegate Cursor Composer, and Delegate Grok Composer for implementation;
  use Delegate Cursor/Grok safe review as the non-native small-wave review lane
  and reserve Claude code reviews for larger wave/phase reviews. GLM/Droid is no
  longer a required per-wave review lane.
- Live smoke credential available from
  `~/.config/exa-agent-cli/credentials.json` (last4 `927c`; do not print full
  key). Final read-only live smoke against `search` and `contents` passes
  within the configured budget; account/team Websets admin probe still returns
  upstream access gating and is not part of the release-blocking smoke.
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
  Wave 3A adds the Agent run lifecycle (`agent run` / `agent runs
  create|list|get|events|cancel|delete`), rich Agent create request fields,
  event replay-vs-pagination validation, destructive delete confirmation, and
  contract-shaped pending-run JSONL recovery records. Wave 3B adds true
  blocking SSE streaming, raw/NDJSON/human progressive stream output, SIGINT
  interruption with resume metadata, Last-Event-ID replay, Agent pagination
  goldens, and Phase 3 gate coverage. Wave 4A adds the top-level monitor
  command family (`monitor create|list|get|update|delete|trigger|batch|runs`),
  monitor list filter-preserving pagination, one-time webhook secret capture,
  batch/delete safety guards, and Phase 4 monitor gate coverage. Wave 4B adds
  Websets core/items/searches/enrichments/imports dispatch, OpenAPI-aligned
  preview/search/import request shaping, destructive Websets safety guards,
  filter-preserving Websets pagination, and Phase 4 Websets gate coverage.
  Wave 4C adds Websets monitors/events/webhooks/attempts dispatch, Websets
  monitor body validation, event/webhook-attempt filter-preserving pagination,
  webhook secret capture, pending-run recovery and raw-output safety for
  secret-capturing creates, and Phase 4 Websets closeout. Wave 5 adds gated
  admin keys create/list/get/update/delete/usage, service-key/auth-host
  separation, selected-profile admin base URLs, API-only `--base-url`
  behavior, admin usage date validation, registry-backed envelope metadata,
  admin create idempotency/pending-run coverage, and Phase 5 gate coverage.
  Wave 6 adds the committed ergonomics harness, search intent-mistake
  correction, typed search filter/category validation, `/contents` shape guard,
  Websets reciprocal `--count`/`--num-results` teaching, `xtask ergonomics`,
  and read-only cost-capped live smoke.

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

| Wave | Status | Implementation lanes | Native review | Second-lane review | Gate |
|---|---|---|---|---|---|
| 0 Baseline/spec audit | complete | parent + native map | n/a | n/a | `cargo xtask vendor-spec --check` pass |
| 1A Registry/parser/envelope | complete | Delegate Cursor Composer + parent integration | native reviewer found `raw --query` preview omission; fixed; re-review clean | GLM review clean; P3 redaction hardening fixed | `cargo xtask ci` pass |
| 1B Request/redaction/body merge | complete | native redaction lane + Delegate Cursor Composer request lane + parent integration | native found `--set` numeric-index panic/OOM risk; fixed; re-review clean | GLM review clean; P3 raw query redaction fixed; narrow re-review clean | `cargo xtask ci` pass |
| 1C Auth/config/doctor | complete | native auth lane + Delegate Cursor Composer config/doctor lane + parent integration | native found secret-env config, stdin flag, doctor warning/check, URL, and logout issues; fixed; re-reviews clean | GLM found error-code contract drift and empty config path guard; fixed; narrow re-review clean | `cargo xtask ci` pass; non-printing stored-credential smokes pass |
| 1D Raw/search/goldens | complete | Delegate Cursor Composer + parent integration | findings fixed; narrow re-review found final error-context redaction issue; fixed | findings fixed; re-review no blocking findings | `cargo xtask phase-gate 1` pass; `cargo xtask ci` pass |
| 2A Search/contents | complete in working tree | native request/chunk lane + Delegate Cursor Composer executor lane + parent integration | native found no-op `--chunk-size`, then chunked error-context gaps; fixed; final approval clean | GLM found clippy/context P3s; fixed; final approval clean | `cargo xtask phase-gate 2` pass; `cargo clippy --workspace --locked -- -D warnings` pass |
| 2B Answer/context/similar/streaming | complete in working tree | native answer/context/similar lane + Delegate Cursor Composer stream lane + parent integration | native found stream output-mode, terminal data shape, helper-only test, then context override bypass; fixed; final approval clean | GLM found context length guard and generic deprecation warning P3s; fixed; final approval clean | `cargo xtask ci` pass; `cargo xtask phase-gate 2` pass; branch-wide secret scan pass |
| 2C Team/macros/deprecations | complete | native baseline macro lane + Delegate Cursor Composer team/research lane + parent integration | initial/re-review findings fixed; final narrow re-review clean with only low-risk notes | initial findings fixed; final GLM re-review clean with accepted P3 residuals | `cargo xtask phase-gate 2` pass; `cargo xtask ci` pass; branch-wide secret scan pass |
| 3A Agent/research lifecycle | complete | native pending-run helper lane + Delegate Cursor Composer Agent lifecycle lane + parent integration | initial findings fixed; final/final-final re-reviews clean | GLM found pending schema P1 and P3 dead-input cleanup; fixed; final-final GLM clean | `cargo xtask ci` pass; `cargo xtask phase-gate 3` pass; branch-wide secret scan pass |
| 3B SSE/SIGINT/pagination | complete | native SSE/interrupt lane + Delegate Cursor Composer mechanical gate/test lane + parent integration | native found Phase 3 lock gaps, partial-id resume bug, NDJSON write-failure gap; fixed; final re-review clean | Delegate Cursor safe found callback/write-failure resume bug; fixed; Delegate Grok safe returned only a progress line and was replaced per updated lane policy | `cargo xtask phase-gate 3` pass; `cargo xtask ci` pass; branch-wide secret scan pass |
| 4A Monitors | complete | native checklist + Delegate Cursor Composer implementation + parent integration | found/fixed secret-output preflight, final-path reservation, post-write deletion, missing-secret, and narrow refactor issues; final reviews clean | Delegate Grok safe found/fixed non-boolean batch `dry_run` bypass and confirmed final fixes; one post-CI Grok run returned only progress output | `cargo xtask phase-gate 4` pass; `cargo xtask ci` pass; branch-wide secret scan pass |
| 4B Websets core | complete in working tree | native checklist + Delegate Cursor Composer implementation + parent integration; Grok xtask lane no-op | found/fixed preview `search=true`, imports update fixture, item pagination test gap, and count range validation; final re-review approved | Delegate Grok safe review returned only progress output; recorded as partial/no-op under current lane policy | `cargo xtask phase-gate 4` pass; `cargo xtask ci` pass; tracked/diff secret scan pass |
| 4C Websets webhooks/events/closeout | complete | native checklist + Delegate Cursor Composer implementation + parent integration; Grok review lanes no-op/partial | found/fixed monitor create `behavior.type` validation; final re-reviews approved | Claude phase review found/fixed secret-create pending-run recovery, raw rejection, and pagination test gaps; Delegate Grok returned progress/cancelled output | `cargo xtask phase-gate 4` pass; `cargo xtask ci` pass; tracked/diff secret scan pass |
| 5 Admin keys | complete in working tree | native checklist + Delegate Cursor Composer implementation + parent integration; Grok review attempts inconclusive | found/fixed profile admin URL, date validation, envelope metadata, reciprocal service/API shape guard, date-only boundary; final re-review approved | Delegate Grok safe reviews (`grok-14`, `grok-15`) returned progress-only; Delegate Cursor safe fallback (`cursor-18`) approved | `cargo xtask phase-gate 5` pass; `cargo xtask ci` pass; tracked/diff secret scan pass |
| 6 Ergonomics/smoke/release | complete in working tree | parent integration plus Delegate Cursor Composer implementation/review support; Delegate Grok Composer review attempts; native Codex review | native findings for negative/bare mistake flags and bare `--num-results` fixed; final native issue covered by regression tests | Delegate Cursor findings fixed; Delegate Grok final run progress-only; Claude large-wave review found only P3s, fixed/accepted | `cargo xtask phase-gate 6` pass; `cargo xtask ci` pass; `cargo xtask smoke --budget 0.05` pass |

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
| 3A | native review | medium | `agent run` / `agent runs create` initially omitted documented create fields (`outputSchema`, `input`, `previousRunId`, `metadata`, `dataSources`) | fixed | Added Agent create flags, overlay mappings, JSON parsing helpers, data-source max-5 validation, request/CLI regressions, and Phase 3 gate coverage. |
| 3A | native/GLM review | medium | `agent runs events --last-event-id` was accepted without `--stream`, and stream mode could mix cursor pagination flags | fixed | Added `validate_agent_runs_events_mode` to reject replay-without-stream and stream+pagination combinations; added CLI regressions. |
| 3A | GLM review | P1 | Pending-run JSONL record used helper field names (`operation`, `path`, `createdAt`, nested `suggestedCommand`) instead of frozen `exa.cli.pending_run.v1` fields | fixed | Record now serializes `attemptedAt`, `operationId`, `apiPath`, `requestId`, `idempotencyKey`, and `recoveryCommand`; docs and tests updated. |
| 3A | GLM review | P3 | Pending-run helper still accepted unused method/correlation/request-body inputs after schema correction | fixed | Removed dead inputs, renamed public helper fields to contract names, and asserted exact pending-record keys. |
| 3B | native review | medium | Phase 3 gate initially omitted pending-run and paginated-list lock coverage | fixed | Added `golden_pending_run_record`, `golden_paginated_all_ndjson`, and Phase 3 gate entries for both plus stalled-stream SIGINT. |
| 3B | native review | high | Resume metadata could advance to a partial `id:` line before the SSE frame was completed/emitted | fixed | `SseDecoder` only advances `last_event_id` on frame flush, and live transport reports the last fully emitted frame id. |
| 3B | Delegate Cursor safe | high | Callback/raw stream write failures after a prior frame could drop `details.lastEventId` | fixed | Wrapped interrupted callback errors with the previous emitted event id and added `send_sse_callback_error_reports_previous_emitted_event_id`. |
| 3B | native review | high | NDJSON streaming wrote with infallible `println!`, so broken stdout could bypass the callback-error wrapper | fixed | Added fallible `write_ndjson`, streaming NDJSON writer helper, and output-boundary regression for write failure. |
| 3B | parent | medium | Human TTY `--stream` path was not progressive despite contracts §8 | fixed | Added minimal progressive human frame writes with fallible error mapping and helper regressions. |
| 3B | Delegate Cursor safe | low | Interrupted-stream envelope lock is an integration assertion, not an insta snapshot; SIGINT test is Unix-only | accepted for now | The black-box test asserts exit 12 and stderr envelope `details.lastEventId`; this local run targets macOS/Unix. Cross-platform signal harness can be added with CI platform work. |
| 4A | native checklist | high | Top-level `monitor` parser existed but dispatch was unwired; generated monitor registry fields were empty, so custom request builders were required | fixed | Added `Command::Monitor` dispatch and monitor-specific builders for create/update/list/batch/runs while keeping Websets monitor paths separate. |
| 4A | native review | medium | `monitor create --secret-output` originally detected output-file failures only after `POST /monitors`, risking one-time `webhookSecret` loss | fixed | Reserved the final target path before auth/network with `create_new`; added bad-parent/existing-target no-POST regressions. |
| 4A | native re-review | medium | Temp-file reservation did not reserve the final pathname before POST | fixed | Replaced temp+rename with final-path `create_new` reservation and explicit existing-target refusal. |
| 4A | native re-review | high | A post-write sync/chmod failure could delete a successfully written one-time secret | fixed | Marked the reservation committed immediately after `write_all` succeeds and removed redundant post-create/post-write chmod paths. |
| 4A | native review | medium | `--secret-output` could silently succeed if upstream omitted a string `webhookSecret` | fixed | Missing/non-string `webhookSecret` now returns `upstream_malformed`, leaves stdout empty, and removes the reserved file; regression added. |
| 4A | Delegate Grok safe | high | Non-boolean batch `dry_run` values such as `"false"` bypassed live confirmation because `as_bool()` returned `None` | fixed | Batch shape validation now rejects non-boolean `dry_run`/`dryRun` as `invalid_value`; regression added. |
| 4A | Delegate Grok safe | low | `monitor update <id>` with no patch fields could send an empty live PATCH | fixed | Empty monitor update bodies are rejected locally with `missing_required_argument`; regression added. |
| 4A | CI/clippy | medium | New helper signatures exceeded clippy's `too_many_arguments` threshold under `cargo xtask ci` | fixed | Grouped monitor update fields and pagination execution parameters into small structs; post-CI native review found no regression. |
| 4A | Delegate Grok safe | low | Windows lacks Unix `0600` hardening for `--secret-output`; wrong-type `webhookSecret` and camelCase `dryRun` alias have no separate black-box cases | accepted for now | Current Unix mode is tested on macOS; wrong-type and alias paths share the same validation helper paths as covered cases. |
| 4B | native checklist | high | Websets parser existed but dispatch was unwired; generated Websets registry fields were empty, so custom builders were required | fixed | Added `Command::Websets` dispatch and custom builders for core/items/searches/enrichments/imports while keeping monitors/events/webhooks deferred to Wave 4C. |
| 4B | native review | medium | `websets preview --count` built `search.count` but omitted the upstream `search=true` activation query parameter | fixed | Preview now adds `search=true` when the final merged body contains numeric `search.count`; tests assert item-preview and decomposition-only modes. |
| 4B | native review | low | Imports update test used non-OpenAPI `status=completed` fixture | fixed | Switched test/suggestion to OpenAPI-supported `title`; implementation remains body-first for advanced `--body`/`--set` payloads. |
| 4B | native review | low | `websets items list --all --source-id` did not have filter-preserving pagination coverage | fixed | Added local two-page HTTP regression asserting `sourceId` remains on every cursor request. |
| 4B | native re-review | low | Websets `--count` flags accepted OpenAPI-invalid ranges | fixed | Added clap range parsers for create/searches count `>=1` and preview count `1..=10`; invalid-count regressions assert structured `invalid_value`. |
| 4B | Delegate Grok safe | low | Wave 4B Grok review returned only progress output | accepted for now | Native re-review approved after fixes and all local gates passed; progress-only Grok result recorded under current lane policy. |
| 4C | native review | medium | `websets monitors create --body` accepted missing/empty `behavior.type` even though the final body contract requires it | fixed | Removed implicit default fallback from final-body validation and added missing/empty `behavior.type` regressions. |
| 4C | Claude phase review | medium | Secret-capturing create paths bypassed pending-run recovery on ambiguous POST failures | fixed | Wrapped both `monitor create` and `websets webhooks create` custom live paths with `maybe_record_pending_run_on_create_failure`; added pending-run CLI regressions. |
| 4C | Claude phase review | low | Secret-capturing create paths silently ignored live `--raw` | fixed | Reject live `--raw` with `invalid_flag_combination` for `monitor create` and `websets webhooks create`; regressions added. |
| 4C | Claude phase review | low | Events and webhook-attempt `--all` static filters lacked direct across-page regressions | fixed | Added local two-page tests for events `types`/`createdAfter` and attempts `eventType`/`successful`. |
| 4C | Delegate Grok safe | low | Grok review lanes returned progress-only/cancelled output after tool errors | accepted for now | Native and Claude reviews found/fixed actionable issues; final native re-review and gates pass. |
| 5 | native review | high | `--profile` selected service credential profile but not profile-specific `admin_base_url` | fixed | Added selected-profile-aware admin/base URL resolution and local-server regression. |
| 5 | Delegate Grok review | high | `--base-url` overrode admin host despite docs defining it as API-only | fixed | Service namespace now ignores `--base-url`; admin host uses `EXA_ADMIN_BASE_URL`/profile/default; regression added. |
| 5 | native review | medium | Usage date validation allowed future/old `end-date` values | fixed | Validates each supplied date against future and last-180-days guard; regressions added. |
| 5 | native review | low | Success envelopes omitted registry operation metadata for admin ops | fixed | `ResponseEnvelopeArgs` now accepts operation metadata; typed/admin envelopes emit `operationId`/`source`/`sourceVersion`; raw stays generic. |
| 5 | Delegate Grok review | medium | Admin create no-retry/idempotency wire coverage was missing | fixed | Added CLI preview and lib live tests for unkeyed no-retry/pending-run and keyed retry/header. |
| 5 | native re-review | medium | Obvious service-shaped keys in `EXA_API_KEY` were accepted for API commands | fixed | Added conservative service-key shape guard, typed/raw API rejection, auth-status warning, and regressions. |
| 5 | native re-review | low | Date-only 180-day boundary depended on current time of day | fixed | Lower lookback boundary now floors to UTC midnight; midday regression added. |
| 5 | Delegate Grok safe | low | Grok safe review runs returned progress-only output despite successful exit | accepted for now | Native final re-review and Delegate Cursor safe fallback approved; issue recorded as harness/output limitation. |
| 6 | native review | P2 | Hidden search/Websets mistake flags with negative values were parsed by Clap before intent validation | fixed | Added `allow_negative_numbers`/optional missing values to hidden mistake flags and regressions for negative/bare `--limit`, `--count`, and `websets create --num-results`. |
| 6 | native final review | P1 | Bare real `search --num-results` still lost `/search` operation context | fixed | Made `--num-results` accept a missing value and route through `validate_search_num_results`; added regression. |
| 6 | Delegate Cursor safe | P2 | Known `search --filter` values could suggest broken `--set` fallback or two-hop category fixes | fixed | Mapped known filter keys to typed flags, canonicalized `category=person` to `people`, and changed unknown/no-`=` filters to schema discovery. |
| 6 | Delegate Cursor safe | P2 | `--body`/`--set numResults=0` bypassed `--num-results` range validation | fixed | Added final merged-body `numResults` validation and regressions for `--set`, `--body`, negative, and bare flag cases. |
| 6 | Claude large-wave review | P3 | Live smoke and checklist language still called the smoke "non-paid" and filter fallback could be non-paste-ready | fixed | Reworded smoke docs to read-only/low-cost/cost-capped and fixed no-`=` filter fallback to schema discovery. |
| 6 | Delegate Grok safe | low | Final Grok review returned only progress output despite successful exit | accepted for now | Native, Cursor, and Claude reviews produced actionable findings; all are fixed or explicitly accepted with tests/gates passing. |
| 6 | Claude/Cursor residual | P3 | Static score-floor test and duplicated category lists are not ideal long-term ergonomics instrumentation | accepted for v1 | Score floor is documented as a tripwire, not measurement; category-list dedupe is low-risk follow-up after release gates. |

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
| 2026-06-29 | focused Wave 3A tests (`agent_runs`, Agent request mapping, pending record) | pass | After native/GLM review fixes for create fields, event mode validation, and pending schema |
| 2026-06-29 | `cargo xtask ci` | pass | Wave 3A final gate after pending schema and P3 cleanup |
| 2026-06-29 | `cargo xtask phase-gate 3` | pass | Agent lifecycle dry-run smokes plus full workspace tests; includes rich Agent create request fields |
| 2026-06-29 | native Wave 3A final-final re-review (`019f15d6-7be3-7d13-99a0-9911339fe075`) | pass | No blocking findings after pending schema cleanup |
| 2026-06-29 | Delegate GLM Wave 3A final-final re-review (`droid-26`) | pass | No blocking findings; confirmed pending cleanup and dispatch signatures |
| 2026-06-29 | non-printing tracked/diff/history secret scan | pass | Stored API key fingerprint `927c` absent from tracked files, working diff, and reachable branch blobs |
| 2026-06-29 | `delegate worktree remove cursor-11 --discard-uncommitted --force-branch` | pass | Integrated Cursor worktree cleaned via Delegate manager; no Delegate worktrees remain present |
| 2026-06-30 | focused Wave 3B stream tests | pass | `stream_event_*`, callback resume-token, stream decoder, stalled SIGINT, and streaming NDJSON transport checks |
| 2026-06-30 | Delegate Grok safe Wave 3B review (`grok`) | partial | Returned only a progress line; replaced by Delegate Cursor safe review under updated lane policy |
| 2026-06-30 | Delegate Cursor safe Wave 3B review (`cursor-13`) | findings fixed | Found callback/raw stream write-failure resume-token gap; fixed and regression-covered |
| 2026-06-30 | native Wave 3B reviews (`019f1611...`, `019f1619...`, `019f161a...`) | pass | Found/fixed NDJSON write-failure gap; final narrow re-reviews clean |
| 2026-06-30 | `cargo xtask phase-gate 3` | pass | Full workspace tests plus Agent dry-run smokes, pending-run, paginated-list, and stalled-SIGINT checks |
| 2026-06-30 | `cargo xtask ci` | pass | fmt, clippy, and full workspace tests after all Wave 3B review fixes |
| 2026-06-30 | non-printing tracked/diff secret scan | pass | Stored API key fingerprint `927c` absent from tracked files and working diff |
| 2026-06-30 | `delegate worktree list` | pass | All Delegate worktrees are removed; none present with unreviewed edits |
| 2026-06-30 | Delegate Cursor Composer worktree (`cursor-14`) | pass | Implemented initial Wave 4A monitor diff in isolated worktree; parent reviewed and integrated into source checkout |
| 2026-06-30 | Delegate Grok Composer worktree (`grok-2`) | no-op | Implementation lane returned no source changes |
| 2026-06-30 | native Wave 4A checklist (`019f1624...`) | pass | Produced monitor contract/request-shape checklist that guided implementation and tests |
| 2026-06-30 | native Wave 4A reviews (`019f162e...`, `019f1631...`, `019f1635...`, `019f1638...`, `019f163c...`) | findings fixed | Found and verified secret-output safety fixes: preflight before POST, final-path reservation, post-write preservation, missing-secret hard error, and chmod cleanup |
| 2026-06-30 | Delegate Grok Wave 4A safe reviews (`grok-5`, `grok-6`, `grok-7`) | findings fixed | First run progress-only; later runs found batch `dry_run` bypass and empty-update gap, then confirmed no actionable findings after fixes |
| 2026-06-30 | `cargo fmt --check && git diff --check` | pass | Wave 4A formatting and whitespace gate after final fixes |
| 2026-06-30 | `cargo test --test cli monitor_create -- --nocapture` | pass | 8 monitor-create tests including secret-output capture, preflight, existing-target, stdout refusal, and missing-secret hard error |
| 2026-06-30 | `cargo test --test cli monitor -- --nocapture` | pass | 16 monitor tests covering create/list/get/update/delete/batch/runs and filter-preserving pagination |
| 2026-06-30 | `cargo xtask phase-gate 4` | pass | Full workspace tests plus monitor dry-run smokes and 16-test monitor slice |
| 2026-06-30 | `cargo xtask ci` | fail then pass | Initial fail on clippy `too_many_arguments`; refactored helper parameter structs and reran successfully |
| 2026-06-30 | native post-CI refactor review (`019f163f...`) | pass | No behavior/lifetime regressions from clippy-driven refactor |
| 2026-06-30 | Delegate Grok post-CI refactor review (`grok-8`) | partial | Returned only a progress line; no actionable signal |
| 2026-06-30 | non-printing tracked/diff secret scan | pass | Stored API key prefix/suffix absent from tracked files, working diff, and cached diff |
| 2026-06-30 | `delegate worktree remove grok-2`; `delegate worktree remove cursor-14 --discard-uncommitted --force-branch` | pass | Completed Wave 4A Delegate worktrees removed through Delegate manager after source integration and review |
| 2026-06-30 | native Wave 4B checklist (`019f1644...`) | pass | Identified Websets core/items/searches/enrichments/imports contract map, Wave 4C deferrals, safety gates, and OpenAPI traps |
| 2026-06-30 | Delegate Cursor Composer worktree (`cursor-15`) | pass | Implemented initial Wave 4B Websets diff in isolated worktree; parent reviewed, corrected, and integrated into source checkout |
| 2026-06-30 | Delegate Grok Composer worktree (`grok-9`) | no-op | Xtask lane returned no source changes; parent wired Phase 4 Websets smokes directly |
| 2026-06-30 | native Wave 4B reviews (`019f164d...`) | findings fixed | Found preview `search=true`, imports fixture, items pagination test gap, and count range validation; final re-review approved |
| 2026-06-30 | Delegate Grok Wave 4B safe review (`grok-10`) | partial | Returned only progress output; no actionable findings |
| 2026-06-30 | `cargo fmt --check && git diff --check && cargo test --test cli websets -- --nocapture` | pass | 10 Websets CLI tests covering create/preview/list/items/searches/enrichments/imports and deferred monitors |
| 2026-06-30 | `cargo xtask phase-gate 4` | pass | Full workspace tests plus monitor and Websets dry-run smokes; monitor slice 17 tests, Websets slice 10 tests |
| 2026-06-30 | `cargo xtask ci` | pass | Wave 4B final gate: fmt, clippy, full workspace tests |
| 2026-06-30 | non-printing tracked/diff secret scan | pass | Stored API key absent from tracked files and working diff |
| 2026-06-30 | native Wave 4C checklist (`019f1657...`) | pass | Mapped monitors/events/webhooks/attempts contract, secret field, query names, and pagination/safety traps |
| 2026-06-30 | Delegate Cursor Composer worktree (`cursor-16`) | pass | Implemented initial Wave 4C Websets monitors/events/webhooks diff in isolated worktree; parent reviewed, hardened, and integrated |
| 2026-06-30 | Delegate Grok Wave 4C safe reviews (`grok-11`, `grok-12`) | partial | Returned progress-only/cancelled output after tool errors; no actionable review signal |
| 2026-06-30 | native Wave 4C reviews (`019f1660...`) | findings fixed | Found/fixed monitor create `behavior.type` validation; final re-reviews approved Claude-finding fixes |
| 2026-06-30 | Claude Wave 4C phase review (`claude`) | findings fixed | Found/fixed pending-run recovery and raw rejection for secret-capturing creates plus direct pagination test gaps |
| 2026-06-30 | `cargo test --test cli monitor_create_ -- --nocapture` | pass | 10 monitor-create tests including pending-run and raw-rejection regressions |
| 2026-06-30 | `cargo test --test cli websets_ -- --nocapture` | pass | 20 Websets tests covering Wave 4C monitors/events/webhooks/attempts, secret capture, pending-run, and static-filter pagination |
| 2026-06-30 | `cargo xtask phase-gate 4` | pass | Full workspace tests plus monitor/Websets Phase 4 smokes; monitor slice 21 tests, Websets slice 20 tests |
| 2026-06-30 | `cargo xtask ci` | fail then pass | Initial clippy `nonminimal_bool` failure fixed; final fmt, clippy, and full workspace tests pass |
| 2026-06-30 | `delegate worktree remove cursor-16 --discard-uncommitted` | pass | Integrated Cursor worktree removed through Delegate manager; no present Delegate worktrees |
| 2026-06-30 | native Wave 5 checklist (`019f1670...`) | pass | Mapped admin-key service namespace, host, date, delete, idempotency, and safety traps |
| 2026-06-30 | Delegate Cursor Composer worktree (`cursor-17`) | pass | Implemented initial Wave 5 admin-key diff in isolated worktree; parent reviewed, corrected, and integrated |
| 2026-06-30 | Delegate Grok Wave 5 safe reviews (`grok-13`, `grok-14`, `grok-15`) | partial | `grok-13` produced actionable admin host/idempotency findings; later re-reviews returned only progress output |
| 2026-06-30 | native Wave 5 reviews (`019f167a...`) | findings fixed then pass | Found profile admin URL, usage date, envelope metadata, reciprocal service/API shape, and date-only boundary issues; final follow-up approved |
| 2026-06-30 | Delegate Cursor Wave 5 safe fallback (`cursor-18`) | pass | Approved with no findings after Grok re-review output was inconclusive |
| 2026-06-30 | `cargo test --test cli -- --nocapture` | pass | 112 CLI tests including Wave 5 admin and service-shaped API key regressions |
| 2026-06-30 | `cargo xtask ci` | pass | fmt, clippy, full workspace tests after Wave 5 final fixes |
| 2026-06-30 | `cargo xtask phase-gate 5` | pass | Full workspace tests plus admin dry-run smokes and admin CLI slice |
| 2026-06-30 | non-printing tracked/diff secret scan | pass | Stored API key absent from tracked files and working diff |
| 2026-06-30 | `delegate worktree remove cursor-17 --discard-uncommitted` | pass | Integrated Cursor worktree removed through Delegate manager after source integration and review |
| 2026-06-30 | Wave 6 native reviews (`019f169f...`, `019f16a4...`, `019f16a9...`) | findings fixed | Found and verified hidden mistake value parsing, filter mapping, category suggestions, bare `--num-results`, and merged-body `numResults` validation issues |
| 2026-06-30 | Delegate Cursor Wave 6 safe reviews (`cursor-20`, `cursor-21`, `cursor-22`) | findings fixed | Found filter fallback/category suggestion and `numResults` validation gaps; all fixed with regressions |
| 2026-06-30 | Delegate Grok Wave 6 safe reviews (`grok-17`, `grok-18`, `grok-19`) | partial | Returned progress-only output despite successful exits; no actionable signal |
| 2026-06-30 | Claude Wave 6 large-wave review (`claude-2`) | pass with P3s fixed/accepted | No P0/P1/P2 blockers; fixed no-`=` filter fallback and smoke wording; accepted static score/category-list P3s |
| 2026-06-30 | `cargo test --test ergonomics -- --nocapture` | pass | 41 intent/robot-docs/score-floor tests after final review fixes |
| 2026-06-30 | `cargo xtask phase-gate 6` | pass | Full workspace tests, ergonomics, self-description smokes, final dry-run smokes |
| 2026-06-30 | `cargo xtask ci` | pass | fmt, clippy, full workspace tests after Wave 6 final fixes |
| 2026-06-30 | `cargo xtask smoke --budget 0.05` | pass | Real Exa key from credentials file; read-only `search` and `contents`; total reported cost $0.000000 |
| 2026-06-30 | `for n in 1 2 3 4 5 6; do cargo xtask phase-gate "$n"; done` | pass | Final all-phase gate sweep after review fixes |
| 2026-06-30 | `cargo xtask ci`; local CLI JSON smokes; `git diff --check` | pass | Final fmt, clippy, full tests, `capabilities`, `search --dry-run --print-request`, and raw `/websets/v0/teams/me` dry-run schema checks |
| 2026-06-30 | `cargo xtask smoke --budget 0.05` | pass | Final live read-only smoke with stored key; total reported cost $0.000000 |
| 2026-06-30 | `delegate worktree remove cursor-19 --discard-uncommitted --force-branch`; `delegate worktree remove grok-16` | pass | Removed final Delegate-managed worktrees; `delegate worktree list` reports none present |
| 2026-06-30 | non-printing tracked/diff secret scan | pass | Stored API key fingerprint `927c` absent from tracked files and working/staged diffs |

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
| 2026-06-29 | `fa09849` | Wave 2C typed `team info`, legacy research create/list/get with cursor pagination, `ask`/`fetch` macro aliases, and Phase 2 team/research gate | `cargo xtask ci`; `cargo xtask phase-gate 2`; native + GLM final reviews clean; branch-wide secret scan pass |
| 2026-06-29 | `45d1860` | Wave 3A Agent run lifecycle, rich Agent create body fields, event replay validation, delete confirmation, and pending-run JSONL recovery contract | `cargo xtask ci`; `cargo xtask phase-gate 3`; native + GLM final-final reviews clean; branch-wide secret scan pass |
| 2026-06-30 | `b707df5` | Wave 3B blocking SSE streaming, SIGINT/resume metadata, raw/NDJSON/human progressive stream output, Agent pagination and pending-run locks, and updated lane policy | `cargo xtask phase-gate 3`; `cargo xtask ci`; native + Delegate Cursor reviews clean; branch-wide secret scan pass |
| 2026-06-30 | `5ca5f18` | Wave 4A top-level monitor command family, webhook secret capture, filter-preserving monitor pagination, batch/delete safety, and Phase 4 monitor gate | `cargo xtask phase-gate 4`; `cargo xtask ci`; native + Delegate Grok reviews clean/recorded; branch-wide secret scan pass |
| 2026-06-30 | `498e34d` | Wave 4B Websets core/items/searches/enrichments/imports, preview activation, Websets pagination, import validation, safety gates, and Phase 4 Websets gate | `cargo xtask phase-gate 4`; `cargo xtask ci`; native review approved; Delegate Grok progress-only recorded; tracked/diff secret scan pass |
| 2026-06-30 | `ea92d46` | Wave 4C Websets monitors/events/webhooks/attempts, webhook secret capture, secret-create pending-run recovery/raw safety, and Phase 4 closeout | `cargo xtask phase-gate 4`; `cargo xtask ci`; native + Claude reviews clean; Delegate Grok partial/no-op recorded; tracked/diff secret scan pass |
| 2026-06-30 | `e0e4c3f` | Wave 5 gated admin keys, service-key/admin-host separation, usage validation, admin idempotency/pending-run safety, registry-backed operation metadata, and Phase 5 gate | `cargo xtask ci`; `cargo xtask phase-gate 5`; native final review clean; Delegate Cursor safe review clean; Delegate Grok partial recorded; tracked/diff secret scan pass |
| 2026-06-30 | this commit | Wave 6 ergonomics harness, intent-mistake teaching, search filter/category validation, read-only live smoke, and release closeout | `cargo xtask phase-gate 6`; `cargo xtask ci`; `cargo xtask smoke --budget 0.05`; native/Cursor/Claude review findings fixed; Delegate Grok partial recorded |

## Final completion checklist

- [x] `cargo xtask ci`
- [x] `cargo xtask phase-gate 1`
- [x] `cargo xtask phase-gate 2`
- [x] `cargo xtask phase-gate 3`
- [x] `cargo xtask phase-gate 4`
- [x] `cargo xtask phase-gate 5`
- [x] `cargo xtask phase-gate 6`
- [x] `cargo xtask smoke --budget "$EXA_E2E_BUDGET"` with real `EXA_API_KEY`
- [x] `cargo run -- capabilities --compact`
- [x] `cargo run -- search "test query" --dry-run --print-request --compact`
- [x] `cargo run -- raw GET /websets/v0/teams/me --dry-run --print-request --compact`
- [x] final native review complete with findings fixed/accepted
- [x] final required native/second-lane review complete with findings fixed/accepted
- [x] final large-wave Claude review complete with findings fixed/accepted
- [x] no unreviewed Delegate worktrees
- [x] final git tree clean or intentional artifacts documented
