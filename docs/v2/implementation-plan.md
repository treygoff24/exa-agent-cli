# v2 Implementation Plan

Date: 2026-06-29
Status: build/test/release plan for the `exa-agent` CLI. Implements [`decisions.md`](decisions.md) (D1–D22) and [`contracts.md`](contracts.md). Where this plan and a decision disagree, the decision wins. This doc owns phasing, testing, CI, and release; it references command internals and module layout only at a high level (see `architecture.md` / `commands.md`). **Reviewed** by the `plan-reviewer` subagent and Codex (work-mode); their reviews are in [`reviews/`](reviews/) and their findings are folded in below.

## How to read this

Seven phases: **Phase 0** (a real pre-build gate — has a blocker) then **Phases 1–6**, all six of which ship in the v1 release. Each build phase lists deliverables, acceptance, the **gate** that proves it, and the golden snapshots it locks. The v1 shape from `decisions.md`: **`raw` moves into Phase 1**, **admin keeps its own phase but is in-scope for v1**, **doctor is read-only**, and the **preset/macro registry is deferred**.

## Canonical gates

One aggregate command means "green," identical locally and in CI. Run the wave-boundary gate before starting the next phase.

```bash
cargo xtask ci              # offline, deterministic: fmt, clippy --all-targets --all-features -D warnings,
                            #   cargo doc --no-deps --all-features, cargo deny check, cargo audit,
                            #   unit/integration/goldens (compare-only), actionlint when present
cargo xtask phase-gate N    # only the tests + snapshots required for Phase N (the phase's named tests below)
cargo xtask smoke --budget "$EXA_E2E_BUDGET"   # opt-in live Exa; never part of `ci`
```

`cargo xtask ci` is the single source of truth for "is the build green," and it is **offline** (`EXA_E2E` unset → the smoke suite is `#[ignore]`d). `clippy` runs `--all-targets --all-features` so the keyring-gated path and the `xtask`/test code are linted too; `cargo deny check` + `cargo audit` (committed `deny.toml`) scan for RUSTSEC advisories and license/ban policy on the graph that ships to every agent over TLS (a custom `cargo tree -i` ban-list enforces the *architecture* decisions — D14/D21 — but does not scan for CVEs, so both are needed). Every phase's **Gate:** line below names `phase-gate N`; the named tests it runs are enumerated in the [Invariant regression matrix](#invariant-regression-matrix).

For a long autonomous `/goal` run, use [`autonomous-implementation-plan.md`](autonomous-implementation-plan.md) as the execution overlay. It preserves the phases below but adds wave decomposition, native subagent and Delegate Cursor implementation lanes, mandatory native + GLM review gates, and final local completion criteria.

---

## Phase 0 — Pre-build gate (one blocker, then confirmations)

D1–D22 are locked. Phase 0 is not a coding task; it proves the inputs the build depends on are real before Phase 1 starts.

**🚧 BLOCKER — vendor the canonical spec(s) before any Phase-1 golden is frozen.** The Phase-1 `capabilities --json`, success-envelope (`operation.source`/`sourceVersion`), and `embeddedSpecSha256` goldens are derived from the spec, so the wrong spec poisons them permanently. Per D22 (verified 2026-06-29):
- Fetch the live consolidated **`https://exa.ai/docs/exa-spec.json`** (verified `info.title = "Exa Public API"`, `info.version = "2.0.0"`, containing `/search`, `/contents`, `/answer`, `/findSimilar`, `/agent/runs`(+sub), `/monitors`(+sub), `/research/v1`, `/v0/websets`+items/searches/enrichments/imports/events/webhooks/monitors, `/v0/teams/me`) → commit to `openapi/exa-openapi.json`.
- Fetch the **Team-Management** spec (`admin-api.exa.ai`, the only source of `/api-keys`) → commit to `openapi/team-management.json`.
- **Do not** vendor the stale partial specs already sitting in `work/research/` (Search 1.2.0, Websets 0, Team-Management 1.0.0) — they are the wrong title/version/path-set and would mis-key the goldens.
- Record the docs-only surfaces that have no OpenAPI path and must be **overlay-defined** or raw-only: `/context` (overlay-defined typed command), `/chat/completions` + `/responses` (raw-only, D16).

**Confirmations (all resolved by decisions — listed so an implementer doesn't re-litigate):**
- TLS / static-link: `ureq` + rustls, no OpenSSL, no async runtime (D14).
- Keyring vs musl-static: keyring is a default feature, **off** for musl release artifacts (env-first auth intact), on for macOS/Windows (D15).
- OpenAI-compat: not a v1 typed namespace; `raw` covers it (D16).

**Carry-over runtime validations (genuinely not blockers — `raw`/`--body`/`--set`/`schema refresh` cover them):** Websets export endpoints; Research `v1`/`v0` status; OpenAI `/responses` model names; whether 429 returns `Retry-After`; whether key-create returns a one-time secret; admin `rateLimit` semantics. Each is resolved in the phase that touches the surface, not up front.

**Gate:** `openapi/exa-openapi.json` and `openapi/team-management.json` exist, parse, and report the verified title/version; `cargo xtask vendor-spec --check` is clean.

---

## Phase 1 — Skeleton, contracts, `raw`, typed-spine proof, offline self-description

**This is the de-risking phase.** It proves **two** independent load-bearing paths before any typed API command depends on them:
- the **transport spine** — auth, retry classification, redaction, envelope serialization, error→exit mapping — driven end-to-end by `raw METHOD PATH --body`;
- the **typed/registry spine** — `OperationDef.cli_path` → clap leaf → named flags → `FieldDef` request assembly → registry safety/pagination/idempotency metadata — driven **offline** by one typed `--print-request --dry-run` command (so it needs no network and no Phase-2 work). *(Per Codex review: `raw` alone bypasses the typed path, so it gets its own Phase-1 proof.)*

**Deliverables**
- **Workspace scaffold.** One package `exa-agent-cli` (D2) producing `[[bin]] name = "exa-agent"`, split into a `lib` (`exa_agent`, all logic, so tests import it) plus a thin `main`. An `xtask` crate for build tooling (`vendor-spec`, `generate-registry`, `ci`, `phase-gate`, `smoke`, golden helpers). `Cargo.lock` committed; CI uses `--locked`.
- **Operation registry codegen (D9/D17/D22).** `build.rs` merges the two vendored specs + `overlay.toml` into a `registry.rs` static table (`operationId`, method, path, request fields, pagination style, streaming, deprecation, `dangerous`, `idempotency_sensitive`, `namespace`, `cli_path`) emitted into `OUT_DIR` — **not committed** (regenerated reproducibly from the committed inputs). `overlay.toml` may both annotate spec ops and **fully define** docs-only ops (`/context`). The embedded spec SHA-256 is computed at build time and surfaced in `capabilities --json`.
- **Parser skeleton (D13).** `clap` v4 derive; global flags per `contracts.md` §2 / D6; `ValueEnum` for fixed sets; clap's suggestion engine for did-you-mean; clap arg-conflict rules for local validation.
- **Transport layer (the spine).** One module owning credential resolution (env-first per D11), the `ureq` blocking client with rustls (D14; no `tokio`), the **retry/idempotency rule (D7) enforced here so no command can bypass it** — auto-retry only idempotent GETs / network / 429 (honoring `Retry-After`) / 5xx, and **never** a create-classified POST without `--idempotency-key`; redaction (D11/§12) on every sink; locally-generated `request.requestId` (ULID, `SOURCE_DATE_EPOCH`-honoring); timeouts; error classification → the §6 category enum. Stubbed in tests via the `Transport` trait.
- **Keyring behind a `Keyring` trait** (same seam as `Transport`) so `auth login` is testable with an in-memory fake and CI never hits a real daemon; musl build compiles a stub impl.
- **Output layer.** `std::io::IsTerminal` auto (D3); format precedence per §2; stdout-data / stderr-diagnostics (§1); ANSI auto-off on non-TTY/`NO_COLOR`/`CI`/`TERM=dumb`/`--no-color`/non-human format. Envelope serializers for `exa.cli.response.v1`/`exa.cli.error.v1` with field order fixed exactly.
- **`raw METHOD PATH --body|@file|-`** with `--set`, `--print-request`/`--dry-run` (short-circuit before transport — **no `--execute`**), `--raw`, and default (upstream JSON unwrapped under `data`). The transport-spine smoke target.
- **One typed `--print-request --dry-run` command** (`search`) that resolves through registry metadata and builds the expected body **offline** — the typed-spine proof.
- **Offline self-description (the full offline surface, not just `guide`):** `capabilities --json` (§13, incl. `describe` alias, populated `exitCodes`/`errorCodes`/`doctor`/`build`); `schema list/show/export/validate-input` + `schema refresh --check`; `robot-docs guide/commands/errors/examples/prompts`; `config list/get/set/unset/path/profiles`; `auth status`; `auth login`/`auth logout` (keyring-gated); `doctor` (read-only/offline, `--online` opt-in, `exa.cli.doctor.v1` with the linter exit dictionary 0/1/4, every finding names its fix — D8, no `--fix`/undo/backup).
- **Error taxonomy → exit codes (§6)** wired through `main` (thiserror enum → exit code; dictionary static + surfaced in `capabilities`).
- **The architecture §3 registry-consistency triplet, as named tests** (the registry lands here): every `cli_path` resolves to a real clap subcommand **and every clap subcommand has an `OperationDef`** (the reverse direction is what fails-by-construction for a mis-scheduled docs-only command); `idempotency_sensitive` set **equals** the contracts §7 create-POST list exactly; every `dangerous` op's handler requires `--yes`/`--confirm`.

**Acceptance**
- `exa-agent capabilities --json | jq` works offline; `schema list --json` and `robot-docs guide` are agent-usable with no web docs.
- The typed dry-run resolves through registry metadata, builds the expected request body, redacts the printed request, and emits the normal envelope **without network**.
- `exa-agent raw GET /search --body @q.json` round-trips against live Exa (smoke, behind `EXA_API_KEY`); `--raw` emits exact upstream bytes; `--print-request`/`--dry-run` emit the would-be request with the key redacted and never call the API.
- Unknown flag/subcommand → precise `suggestedCommand` + exit 1, **not clap's default exit 2**: clap errors are caught via `try_parse`, remapped to exit 1, rendered as `exa.cli.error.v1` with clap's suggestion mirrored into `error.details.didYouMean`; `--help`/`--version` still exit 0 to stdout (named test `parse_error_remapped_exit1_envelope`).
- Every `error.code` the binary can emit is a member of the published §5.1 dictionary (static test); `capabilities --json` carries the full `exitCodes`/`errorCodes` maps, not placeholders.
- `--json | jq` works with **no** `grep -v`; on error stdout is empty and the error envelope is on stderr.
- Injecting `--header 'Authorization: …'` is refused with exit 1 (D18) — pinned by a named test.
- The no-retry-on-create transport test passes (create attempted exactly once on ambiguous failure; GET 503 retried).

**Gate:** `cargo xtask phase-gate 1`.
**Locks (golden, §14):** `capabilities --json` (incl. populated `exitCodes`/`errorCodes`/`doctor`/`build`), the error-code dictionary (§5.1), `schema list --json`, `robot-docs guide`, success envelope (with `count`/`nextActions`/`dataHash`), error envelope, parse-error envelope (clap-remap), `not_authenticated` envelope (with `details.checked`), `doctor --json` report (§15), `--raw` passthrough, the exit-code table, and key-line `--help` assertions for the Phase-1 commands.

---

## Phase 2 — Core synchronous APIs

Typed commands over the Phase-1 spine: `search` (`/search`), `contents` (`/contents`), `answer` (`/answer`), `context` (**overlay-defined**, `/context` — D22), `similar` (deprecated `/findSimilar`). Plus `team info` (`/v0/teams/me`, read-only) and the two macros D12 keeps (`ask`, `fetch`, each returning `expands_to`).

**Deliverables**
- `search` with full filters, nested contents, output schema, `--stream`, `--print-request`; `--num-results 1..100`; not cursor-paginated → `--all` and `--limit` rejected with exit 1 + a `suggestedCommand` naming `--num-results` (§10, D20).
- `contents` with URL/ID input, `--chunk-size`, batch contract (§11): per-chunk NDJSON, `data.statuses[]` preserved, mixed batch exits **10**.
- `answer` with citations/text/output-schema/`--stream`.
- `context` as the first **overlay-defined** typed command — proves the registry's overlay-defined path end-to-end.
- `team info` — read-only account/limits.
- Streaming (§8) for `search`/`answer` where upstream supports SSE.
- **All deprecation behavior from `commands.md` §5 pinned** (not only `similar`): `--livecrawl`, `--context*`, legacy types — each emits the `warnings[]` entry, with a test.
- `costDollars` populated; `warnings[]` carries deprecations (never stdout prose).
- Resolves carry-over validations here: `Retry-After` on 429, `/contents` `statuses[]` shape.

**Acceptance**
- Contract tests compare the built request body for every flag against a recorded fixture (request-builder golden-pinned per command).
- `/contents` partial statuses → clean partial(10)/success(0).
- Live smoke (gated, budget-capped) covers `search`, `contents`, `answer`.

**Gate:** `cargo xtask phase-gate 2`.
**Locks (golden, §14):** the **streaming NDJSON path**. *(Additive, not part of the frozen §14 set: per-command request-body goldens for the core surfaces.)*

---

## Phase 3 — Async APIs

`agent runs create|list|get|events|cancel|delete` + the `agent run` macro; `research create|list|get` (compatibility). Where the **no-auto-retry-on-create rule (D7) is exercised for real**.

**Deliverables**
- Async lifecycle: create → poll/`get` → `events` (SSE) → structured output + grounding + cost; `cancel`/`delete` confirmation-gated (§9).
- SSE normalization + raw SSE; **a short read timeout (≤250 ms) so a stalled stream still honors SIGINT** → exit **12** + `exa.cli.error.v1` carrying the last `eventId` within ~1s (arch §5); `--last-event-id` resumes.
- **Pending-run records (D7/§7):** ambiguous create failure writes the append-only `exa.cli.pending_run.v1` JSONL record and sets `suggestedCommand` to the recovery. The only local persistence (D5 — JSONL append, not SQLite).
- `agent runs list` is the first **cursor-paginated** command → the uniform pagination model (§10).
- Resolves carry-over validations: Research `v1`/`v0` status; agent concurrency → rate_limit(6).

**Acceptance**
- Agents can create, poll, stream/replay, and retrieve structured output/grounding/cost.
- A **stalled** stream + SIGINT exits 12 within ~1s with last-event metadata (named test).
- The pending-run record + recovery `suggestedCommand` round-trips (create-then-ambiguous-failure test).

**Gate:** `cargo xtask phase-gate 3`.
**Locks (golden, §14):** the **paginated list** (`agent runs list --all --ndjson`), the **interrupted-stream exit-12 error envelope**, and the **pending-run record** (`exa.cli.pending_run.v1`).

---

## Phase 4 — Monitors & Websets

Top-level `monitor` family; `websets` core/items/searches/enrichments/imports/monitors/events/webhooks (all first-class in the spec per D22). `exports` only if runtime validation confirms the endpoints (else `raw` + a note in `DISCREPANCIES.md`).

**Deliverables**
- Full `websets` tree, all over the same registry + spine.
- Uniform cursor pagination (§10) across every list command.
- Destructive ops confirmation-gated (§9): deletes need `--yes`; high-blast-radius batch deletes need `--confirm <token>`; `monitor batch` defaults to dry-run; webset/enrichment `cancel` gated.
- The create-POSTs here (`websets create/searches/enrichments/imports/monitors/webhooks create`, `monitor create`) all carry `idempotency_sensitive` and are covered by the Phase-1 registry-consistency test and the Phase-3 no-retry behavior.
- `externalId` conflict → exit **8** with a recovery `suggestedCommand`.
- Webhook signature-verification guidance in `robot-docs`/`doctor`; webhook secrets redacted (§12).

**Acceptance**
- Every destructive verb refuses without confirmation → exit 9 (one test per verb).
- Cursor pagination uniform across the tree.
- `externalId` conflict → exit 8 with a helpful recovery command.

**Gate:** `cargo xtask phase-gate 4`.
**Locks:** *(additive)* per-command request-body goldens for the destructive verbs; the conflict(8) error envelope.

---

## Phase 5 — Admin / team management (gated, in v1)

D4: ships in v1, walled off. `admin keys create|list|get|update|delete|usage`, sourced from the Team-Management spec (D22).

**Deliverables**
- Separate credential `EXA_SERVICE_KEY` (never `EXA_API_KEY`), distinct keyring scope (`exa-agent:service:<profile>`, D15); host `EXA_ADMIN_BASE_URL` (default `https://admin-api.exa.ai/team-management`).
- The CLI **refuses** to cross a normal API key and a service key, with an actionable error (exit 2/3).
- `admin keys delete` requires `--confirm <key-id>` (confirm-by-id).
- `admin keys create` is `idempotency_sensitive` (no auto-retry without `--idempotency-key`).
- `usage` date validation + 180-day lookback guard.
- If key-create returns a one-time secret, show once + redact everywhere (§12).

**Acceptance**
- A normal `EXA_API_KEY` is never used as an admin key; the cross-use refusal test passes both directions.
- `delete` requires confirm-by-id.
- The no-retry test enumeration **includes `admin keys create`** (exercised here).

**Gate:** `cargo xtask phase-gate 5`.
**Locks:** *(additive)* the cross-use refusal envelopes; the `delete` safety-refusal envelope.

---

## Phase 6 — Agent-ergonomics polish pass

A **committed, CI-runnable** ergonomics gate — not a dependency on a local skill path. The `agent-ergonomics-and-intuitiveness-maximization-for-cli-tools` skill is an optional *local* audit aid; the release gate is a portable harness in-repo.

**Deliverables**
- `tests/ergonomics/` harness + `docs/v2/ergonomics-checklist.md` (the intent-mistake corpus as named tests): `--all` on `search`, top-level `text` on `search`, nested contents on `/contents`, bad category/filter combos, `--limit` on `search` — each must infer intent or refuse with a precise `suggestedCommand`.
- First-try simulations against the One Rule.
- Re-verify all §14 goldens after any fix.
- `robot-docs` completeness check (every command/error reachable).
- Live smoke as the final gate (non-paid subset; see policy below).

**Acceptance (falsifiable):** every intent-mistake test green; **no scored ergonomics dimension below a stated floor** (e.g. ≥ 700 on the rubric's worst dimension — set the number when the first audit runs); all §14 goldens green; non-paid smoke green within budget.

**Gate:** `cargo xtask ergonomics` + `cargo xtask phase-gate 6`.

**Deferred to post-v1:** the configurable preset/macro registry (D12); auto-spill enablement (ship `--output` in v1 — D10); `doctor --fix` + its chokepoint/backup/undo (D8 upgrade path); `openai` typed wrappers (raw covers v1 — D16).

---

## Command coverage

Every top-level namespace in `commands.md` has a phase, a gate, and a golden/test. This table is the completeness check (it is also enforced by the registry coverage test, layer 6).

| Namespace | Phase | Notes |
|---|---|---|
| `raw` | 1 | spine smoke target |
| `capabilities` / `schema` / `robot-docs` / `doctor` / `config` / `auth` | 1 | offline self-description (full sub-command set, not just `guide`) |
| `search` / `contents` / `answer` / `context` / `similar` | 2 | `context` is overlay-defined |
| `team info` | 2 | read-only |
| `agent` / `research` | 3 | create-POSTs; pending-run records |
| `monitor` / `websets` (+ all sub-resources) | 4 | confirmation-gating, cursor pagination |
| `admin keys` | 5 | gated, Team-Management spec |
| `ask` / `fetch` macros | 2 | thin expansions w/ `expands_to` |
| deprecations (`commands.md` §5) | 2 | each pinned with a `warnings[]` test |

---

## Testing strategy

Six layers, each a different oracle. The split that matters: **golden/contract tests use recorded upstream fixtures and never touch the network**; **the live behavior of real Exa is covered only by the smoke suite.** That keeps `cargo test` deterministic and offline while still honoring "test the real thing" where a fixture would lie.

The spine is built around a `Transport` trait so contract tests inject a fixture-replay backend while production uses the `ureq` backend — it stubs the *wire*, not the CLI's logic, so the envelope/format/exit-code behavior under test is the real code path. Keyring access goes through a parallel `Keyring` trait so daemon-touching paths are faked under test.

### Invariant regression matrix

Every load-bearing invariant maps to a **named** test (no invariant ships unpinned). `cargo xtask phase-gate N` runs the subset whose "Phase" matches.

| Invariant (source) | Named test | Phase |
|---|---|---|
| §14 goldens | `golden_capabilities_json`, `golden_schema_list_json`, `golden_robot_docs_guide`, `golden_success_envelope`, `golden_error_envelope`, `golden_raw_passthrough`, `golden_exit_code_table` | 1 |
| §14 streaming | `golden_streaming_ndjson` | 2 |
| §14 pagination | `golden_paginated_all_ndjson` | 3 |
| §14 pending-run (§7) | `golden_pending_run_record` | 3 |
| no-auto-retry-on-create (D7/§7) | `retry_create_post_matrix_no_unkeyed_retry` (iterates the registry `idempotency_sensitive` set) | 1 (logic) / 5 (admin case) |
| stdout/stderr split (§1) | `stdout_data_stderr_diagnostics_split` | 1 |
| exit-code dictionary (§6) | `exit_code_dictionary_all_categories` | 1 |
| redaction all sinks (§12) | `redaction_all_sinks_trace_suggested_command` | 1 |
| `--header` can't override auth (D18) | `header_override_authorization_refused_exit1` | 1 |
| determinism / field order (§12) | `deterministic_envelope_field_order_twice_serialize` | 1 |
| two-invocation determinism (§12) | `determinism_two_invocations_byte_identical` (spawn binary twice, `SOURCE_DATE_EPOCH` fixed, diff modulo volatile fields) | 1 |
| non-TTY / NO_COLOR (§1/§12) | `non_tty_no_color_no_ansi` | 1 |
| clap parse-error remap (§5/§6) | `parse_error_remapped_exit1_envelope` (exit 1 not 2; envelope on stderr; `--help`/`--version` exit 0) | 1 |
| error-code dictionary (§5.1) | `golden_error_code_dictionary` + `error_code_emitted_is_dictionary_member` (static) | 1 |
| per-category error hints (§5.1) | `golden_error_hints` (one fixture per §6 category asserting code + retryable + `suggestedCommand`) | 1 |
| `not_authenticated` ladder (§5.1) | `golden_not_authenticated_envelope` (incl. `details.checked`) | 1 |
| idempotency-key forwarded upstream (§7) | `idempotency_key_forwarded_as_header` (transport double asserts `Idempotency-Key` present on keyed retry) | 1 |
| doctor report + linter exit dict (§15) | `golden_doctor_report`, `doctor_exit_0_healthy_1_findings` | 1 |
| parser contract (`try_parse_from`) | `parser_contract_matrix` (`--num-results` 0/101 range reject, `ValueEnum` reject, defaults, conflicts, global-flag positioning, `--help`/`--version`) | 1 |
| envelope round-trip deserialize | `envelope_roundtrip_deserialize` (each `response`/`error`/`event`/`pending_run`/`doctor` fixture parses back into its struct) | 1 |
| `--help` key-lines | `golden_help_<cmd>` (assert key lines + an example present, not full layout) | 1 / per phase |
| binary provenance (build) | `capabilities_build_provenance` (commit/buildDate/target present) | 1 |
| `--body -` stdin TTY guard (§4) | `body_stdin_tty_refused_exit11` | 1 |
| stalled-keyring non-block (§8) | `keyring_read_nonblocking_falls_through` | 1 |
| registry↔clap both directions (arch §3) | `registry_clap_bidirectional_consistency` | 1 |
| `idempotency_sensitive` == §7 list (arch §3) | `registry_idempotency_matches_contract_create_list` | 1 |
| `dangerous` ⇒ requires confirm (arch §3) | `registry_dangerous_requires_confirmation` | 1 |
| SSE SIGINT on stalled stream (arch §5) | `sse_sigint_stalled_stream_exits_12_with_last_event_id` | 3 |
| registry codegen reproducible (D9) | `registry_codegen_reproducible` (xtask + `git diff`) | 1 / CI |

### 1. Unit tests (in-crate)
Pure logic, no I/O: error→exit mapping (every §6 category), `error.code` membership in the §5.1 dictionary, redaction, format/TTY precedence, clap arg-conflict validation, ULID determinism under `SOURCE_DATE_EPOCH`, SSE frame parsing, pagination accumulation, the create-vs-idempotent classifier, `BoolishValueParser`/placeholder-guard coercion.

**Parser tier (`tests/parser.rs`, `Cli::try_parse_from`).** The cheapest, fastest contract pins — several load-bearing: `--num-results` range rejection (0 and 101), every `ValueEnum` *accepted* (incl. mixed case via `ignore_case`) **and rejected**, default-value assertions (`--retry 2`), `conflicts_with`/`ArgGroup` violations, global-flag positioning before and after the subcommand, `arg_required_else_help` on bare invocation, and `--help`/`--version` exit semantics. Named `parser_contract_matrix`.

**Envelope deserialize tier.** Every recorded `exa.cli.response.v1`/`error.v1`/`event.v1`/`pending_run.v1`/`doctor.v1` fixture deserializes back into its Rust struct (and re-serializes byte-identically), so output that serializes but a typed consumer can't parse is caught. Named `envelope_roundtrip_deserialize`.

### 2. Contract / golden tests (`insta`) — locks §14
`insta` freezes the §14 surfaces. Non-deterministic fields are scrubbed via a shared `insta::Settings` Scrubber: `requestId`→`[REQID]`, `upstreamRequestId`→`[UPSTREAM_REQID]`, `durationMs`→`[DURATION]`, timestamps→`[TS]`, dynamic ids→`[ID]`. `embeddedSpecSha256` stays **unscrubbed** (its change is a real signal). CI runs compare-only (never auto-accept). A `PROVENANCE.md` records fixture origin.

### 3. CLI integration tests (`assert_cmd` + `predicates`)
Spawn the real binary, assert stdout/stderr/exit:
- **stdout/stderr separation:** success → stdout envelope-only; error → stdout empty, error envelope on stderr; `--json | jq` with no `grep -v`.
- **non-TTY / NO_COLOR:** piped stdout has zero ANSI; `NO_COLOR`/`CI`/`TERM=dumb`/`--no-color` plain. Assert no `\x1b[`.
- **deterministic ordering:** serialize twice → byte-identical; field order matches §4.
- **two-invocation determinism:** spawn the binary twice with `SOURCE_DATE_EPOCH` fixed → byte-identical stdout modulo the documented volatile fields (contracts §12) — distinct from the in-process serialize-twice unit test, catching process-level nondeterminism (locale, env, hashmap iteration).
- **did-you-mean / parse-error remap:** unknown flag/subcommand/enum → exit **1** (never clap's default 2) + `exa.cli.error.v1` on stderr with `details.didYouMean` set; `--help`/`--version` → exit 0 on stdout.
- **`--help` key-lines:** each command's `--help` contains its key argument lines + at least one example (assert presence, not full layout — Clap layout is intentionally not over-snapshotted).
- **redaction:** sentinel `EXA_API_KEY`, grep **all** sinks (stdout, stderr, `--trace` file, `suggestedCommand`); key never appears.
- **`--header` override refusal (D18):** `--header 'Authorization: x'` → exit 1, key never echoed.
- **`--body -` stdin guard:** `--body -` with a TTY stdin → exit 11 (`no_input`), never blocks.

### 4. The no-retry-on-create test (must-have, transport-level)
A `Transport` double simulates a post-send timeout (ambiguous: request sent, no confirmed response). The test **iterates the registry's `idempotency_sensitive` set** (so it can't drift from the contract) — currently `agent runs create`, `research create`, `websets create`, `websets searches/enrichments/imports/monitors/webhooks create`, `monitor create`, `admin keys create`:
- without `--idempotency-key` → exactly **one** attempt, exit non-zero, a `exa.cli.pending_run.v1` record written (asserting the atomic single-line `O_APPEND` write + flush), `error.retryable: false`, recovery `suggestedCommand` set.
- with `--idempotency-key` → retries permitted, **and the `Idempotency-Key` header is present on the wire** (the `Transport` double asserts it — a keyed retry that forgot to forward the key would double-bill exactly like an un-keyed one; contracts §7 / arch §5).
- idempotent GET on 503 → retried `--retry` times.
This pins D7 — the single most expensive rule to get wrong (double-billing) — and a sibling assertion checks the iterated set equals contracts §7 exactly.

### 5. Live smoke suite (real Exa, no mocks) — gated, budget-controlled, with an explicit flake policy
Synthesized from `testing-real-service-e2e-no-mocks`. `#[ignore]` unless `EXA_API_KEY` set **and** `EXA_E2E=1`. A `tests/smoke/manifest.toml` marks each test with `paid = true|false`, `max_cost_dollars`, and `quota_sensitive = true|false`. Policy:
- **Budget cap:** accumulate `costDollars.total`; abort over `EXA_E2E_BUDGET` (default a few cents). `paid = true` tests (create-heavy: agent/research/websets creates) run only under `EXA_E2E_PAID=1`.
- **429 is a controlled skip *only* for `quota_sensitive` tests**, and the test must log `httpStatus`, `Retry-After`, and the redacted command — so a rate-limit regression on a non-quota path still fails rather than silently skipping.
- **Release blocks on the non-paid subset only** (Phase 6). Never exercise `admin` with a live service key in CI.
Every production bug found becomes a fixture-backed regression test in layer 2/3.

### 6. Conformance / spec-drift (registry vs live spec)
Synthesized from `testing-conformance-harnesses`. The spec is the reference; the registry is the implementation under test.
- **Drift (CI, networked, off the hot path):** fetch the canonical spec(s), `xtask vendor-spec`, `git diff` the vendored inputs. Non-empty diff fails with the regen command. Mirrors `schema refresh --check`.
- **Coverage (offline):** every registry `operationId` maps to a command or is intentionally raw-only; gaps in `COVERAGE.md`, intentional divergences in `DISCREPANCIES.md` as **XFAIL** (not SKIP), each with a review date.

---

## CI

GitHub Actions, synthesized from `gh-actions`. Every workflow: `concurrency` group, explicit `timeout-minutes`, minimal `permissions` (elevated only where needed), third-party actions pinned to SHA, `Swatinem/rust-cache@v2`.

**`ci.yml`** — push to `main`, PRs, `workflow_dispatch`:
- `cargo xtask ci` (fmt, clippy `-D warnings`, `cargo test --workspace --locked` for layers 1–4 + golden compare-only, `actionlint`) — **offline** (`EXA_E2E` unset).
- **`cargo test --workspace --locked --no-default-features --features musl-set`** — so the keyring-off feature set that ships in the Linux static binaries is actually *tested*, not only compiled.
- **Static-binary dependency guards** (fail if a forbidden dep leaks into the shipping graph, enforcing D14/D21):
  ```bash
  cargo tree -e features -i tokio        --target x86_64-unknown-linux-musl && exit 1 || true
  cargo tree -e features -i openssl-sys  --target x86_64-unknown-linux-musl && exit 1 || true
  cargo tree -e features -i serde_norway -p exa-agent-cli && exit 1 || true   # YAML parser must stay in xtask only
  ```
- **cross-compile build matrix** for all five release targets (catch musl/Windows breakage on PRs, not at tag time). Native ARM runners, `cross` for musl.
- `insta` compare-only (never auto-accept in CI).

**`drift.yml`** — nightly + dispatch (networked): fetch canonical spec(s) → `xtask vendor-spec` → fail on `git diff`, surfacing the regen command.

**`smoke.yml`** — nightly + dispatch, gated on the `EXA_API_KEY` secret: live smoke with `EXA_E2E=1`, budget cap, non-paid subset by default. Not on every PR. Never `admin`.

**`release.yml`** — see below. Validate workflows locally: `actionlint .github/workflows/*.yml`.

---

## Release / distribution

Single static binary, cross-compiled, checksummed, GitHub Release + `cargo publish`. Synthesized from `release-preparations`, `gh-actions`/RELEASE-BUILD, `rust-crates-publishing`, with `dsr`/`rch` as the throttle fallback.

**Targets (D1).** Linux musl x86_64 + aarch64 (`cross`), macOS x86_64 + aarch64, Windows x86_64-msvc. *(Confirm the GitHub-hosted runner labels — e.g. `macos-14`, `ubuntu-24.04-arm`, `macos-13`/intel — against the current available set before `release.yml` lands; labels churn.)*

- **musl + rustls** so Linux artifacts statically link with no OpenSSL. Verify with `file target/<triple>/release/exa-agent | grep "statically linked"` — **not** `ldd` (false-positives on static binaries).
- **keyring feature off for musl** (env-first auth intact, D15): musl targets build `--no-default-features --features musl-set`.
- `strip` release binaries; `SOURCE_DATE_EPOCH` honored.

**`release.yml`** (on `push: tags: ['v*']`, `permissions: contents: write`, `concurrency` `cancel-in-progress: false`):
1. **build matrix** → `cargo build --release --locked --target …`, `cargo xtask gen-completions`/`gen-man`, package `exa-agent-v<ver>-<target>.tar.xz`/`.zip` (binary + completions + man page), per-artifact `.sha256`.
2. **release job** → merge artifacts, combined `SHA256SUMS.txt`, `install.sh` (cargo-dist-generated, checksum-verifying), `softprops/action-gh-release@v2` (`generate_release_notes: true`).
3. **publish-crates job** (`needs: release`, skip pre-releases): `cargo publish -p exa-agent-cli --locked`, gated on `CARGO_REGISTRY_TOKEN`.
4. *(optional)* minisign/cosign signing, SBOM via syft, SLSA provenance.

**`cargo publish` checklist (D2):**
- Tag `vX.Y.Z` ⇔ `Cargo.toml` `version` (CI asserts before publish).
- Metadata complete: `license`, `repository`, `readme`, `description`, `keywords`, `categories`, and a `[package.metadata.binstall]` block (asset name template → `cargo binstall` path).
- **The build inputs must be packaged** — `build.rs` regenerates the registry at build time, so `openapi/exa-openapi.json`, `openapi/team-management.json`, **and `openapi/overlay.toml`** are real package inputs. Verify with `cargo package --list` (add `include = [...]` if default ignores drop them). A missing `overlay.toml` makes the published crate fail to build.
- A `registry_codegen_reproducible` gate (`cargo xtask vendor-spec && cargo xtask generate-registry && git diff --exit-code openapi/`) runs before publish.
- `cargo audit` is a **hard gate** before the `publish-crates` job (a known CVE in a transitive dep otherwise ships to every consumer).
- Dry-run before tagging: `cargo publish -p exa-agent-cli --locked --dry-run`.

**Fallback (`dsr`/`rch`)** when GH Actions queues > 10 min or has musl issues: `RCH_DISABLED=1 cargo build --release`, build per-target, `sha256sum`, `gh release create`. `dsr fallback exa-agent <version>` runs the full pipeline.

**Install surface — three acquisition paths.** (1) `curl … | sh` (the agent's primary path), (2) `cargo binstall exa-agent-cli` (no-compile, via a `[package.metadata.binstall]` block mapping the release asset names — cheap to add, gives the third path), (3) `cargo install exa-agent-cli` (source). Optional Homebrew tap on top.

**Installer contract (the curl|sh non-negotiables).** Improvising this reproduces the exact anti-patterns `distribution.md` warns about, so it is specified, not left to the script author. Preferred route: generate the matrix + `SHA256SUMS` + a baseline `install.sh` with **cargo-dist**, then layer the agent-config on top. Either way the installer MUST:
- **Verify the checksum *before* installing** — download asset + its `.sha256`, compare, hard-fail on mismatch. If neither `sha256sum` nor `shasum` is present, warn and proceed only with an explicit `--no-verify`/`INSTALL_NO_VERIFY=1`.
- **Be non-TTY-safe** — zero prompts; in a non-TTY it proceeds with defaults. Drive everything by flag/env: `--yes`/`FORCE`, `--dest`/`DEST` (default `~/.local/bin`, no sudo), `--version`, `--no-verify`, `--quiet`.
- **Never silently source-compile** — a target-triple/asset mismatch is a hard error naming the supported triples, **not** a silent fallback (the previous "musl/gnu mismatch falls back to source compile" behavior is removed).
- **Be idempotent** — re-running with the same version is a no-op; `install -m 0755` atomic copy; print the `PATH` export line if `--dest` isn't on `PATH`.
- **End with a grep-able status line** — `INSTALL_OK exa-agent <version> <dest> <target>` (or `INSTALL_FAIL <reason>`), so a driving agent can branch on the result without scraping prose.

**Completions & man pages** are generated build-only via `clap_complete`/`clap_mangen` (`xtask gen-completions`/`gen-man`) and attached as release assets — primarily for human maintainers; an agent self-orients from `capabilities --json`/`robot-docs`, so these are a convenience, not load-bearing.

---

## Crate-dependency bring-up checklist

Pin in `Cargo.toml` as Phase 1 lands; features chosen for the single-static-binary constraint.

**Runtime (ship in the binary):**
- [ ] `clap` v4 (`derive`) — parser + suggestions + `ValueEnum` (D13).
- [ ] `serde` (`derive`) + `serde_json` (`preserve_order`) — envelopes (§4/§5).
- [ ] `ureq` (rustls backend, D14) — blocking, **no async runtime**, short read-timeout on SSE; `Read` body.
- [ ] `ctrlc` — SIGINT flag polled in the SSE read loop (→ exit 12). **No `tokio`** (D14).
- [ ] `thiserror` — error taxonomy → exit-code enum (§6).
- [ ] `ulid` — `requestId` (`SOURCE_DATE_EPOCH`-seedable).
- [ ] `directories` — XDG config + state-dir (config.toml; pending-run JSONL).
- [ ] `tempfile` — atomic temp-then-`persist`(rename) for `--output`/auto-spill payloads (arch §7). Pending-run JSONL uses plain `O_APPEND` (arch §5).
- [ ] `toml` — config parsing (D12).
- [ ] `anstream` + `anstyle` — stderr color with auto-disable.
- [ ] redaction `Scrubber` — targeted key-prefix + secret-header-name scan (no runtime `regex`; arch §10).
- [ ] `keyring` (v3) — **optional, feature-gated** behind a `Keyring` trait (in-memory fake for tests); default on macOS/Windows, off for musl (D15).
- [ ] SSE: hand-rolled framing in transport (small, exact for `--raw`). `eventsource-stream` only if hand-rolling proves fiddly.

**Build / dev only (not in the binary):**
- [ ] `build.rs` registry codegen reads the **vendored JSON** specs via `serde_json` — no YAML parser in the binary; emits `registry.rs` into `OUT_DIR` (not committed).
- [ ] `xtask`: `vendor-spec` (fetch `exa-spec.json` + team-management JSON directly), `generate-registry`, `ci`, `phase-gate`, `smoke`, `ergonomics`, `gen-completions`/`gen-man`, golden helpers. `serde_norway` is pulled in here **only** if a future source is YAML-only.
- [ ] `clap_complete` + `clap_mangen` — generate shell completions + man pages as release assets (build/dev only, not in the binary).
- [ ] `cargo-deny` (committed `deny.toml`) + `cargo-audit` — RUSTSEC/license/ban scanning in `xtask ci` and pre-publish.
- [ ] `cargo-dist` — generates the release matrix, `SHA256SUMS`, and the checksum-verifying `install.sh` baseline.
- [ ] `insta` (`json`, `redactions`); `assert_cmd` + `predicates`; `actionlint`.

### Spec vendoring (D9/D21/D22)
`xtask vendor-spec` fetches `https://exa.ai/docs/exa-spec.json` and the Team-Management spec **directly as JSON** (Exa serves JSON — no YAML conversion needed) into committed `openapi/exa-openapi.json` + `openapi/team-management.json`. `build.rs` parses those + `overlay.toml` → `registry.rs` in `OUT_DIR`. The drift job re-runs `vendor-spec` and `git diff`s the inputs. Full-surface coverage stays honest (D9) and the binary ships no YAML parser and no async runtime.

---

## Resolved seams (historical — retained for traceability)

All seams from the drafting pass are resolved by `decisions.md` D14–D22 and the review pass; none is open. Listed so an implementer doesn't read a resolved tension as a live question.

1. **Paginated/streaming/pending-run goldens lock when their command lands** (Phase 2/3), not Phase 1 — sequencing, not contradiction. The §14 list is frozen; the *lock point* is per-command.
2. **TLS = rustls via `ureq`** (D14) — musl-clean, no OpenSSL. Final.
3. **Keyring off for musl artifacts** (D15) — Linux static binaries ship env-first auth only; macOS/Windows get keyring. Final; tested via the `--no-default-features` CI job.
4. **`openai` not a v1 typed namespace** (D16) — `raw` covers it. Final.
5. **Blocking `ureq`, no `tokio`** (D14); SSE is a blocking line reader with a ≤250 ms read timeout so SIGINT exits 12 promptly (arch §5). Final.
6. **Spec vendored as JSON** (D21/D22) — fetched directly as JSON; no YAML parser ships. Final.
7. **Canonical spec identity** (D22, verified 2026-06-29) — `exa-spec.json`, "Exa Public API" 2.0.0; `/context` overlay-defined; admin from the Team-Management spec; the `work/research/` specs are stale. Phase-0 blocker.
