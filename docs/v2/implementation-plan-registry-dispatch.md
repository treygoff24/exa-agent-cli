# Implementation plan: registry-driven dispatch convergence

Date: 2026-06-30
Status: **ready for autonomous execution** (patched after native plan-reviewer + Codex plan review, 2026-06-30). Companion to [`refactor-registry-driven-dispatch.md`](refactor-registry-driven-dispatch.md) (design + fusion verdict). This is the execution playbook: phases, lane assignments, per-phase review-fix loops, mechanical exit gates, and the autonomy contract. Where the design doc's §0.5 revisions (R1–R8) amend the original proposal, this plan follows the revision.

> **Why this is structured for autonomy:** every decision an orchestrator must make is pre-resolved here (no runtime forks), every phase exit is a command + assertion (no prose judgment), and every lane failure has a recovery branch. A fully autonomous run executes top to bottom without needing a human until the Definition of Done — or an explicit halt.

---

## 1. Operating model

Claude (the session) is **orchestrator + reviewer**, not the implementer. Every code change is produced by a `delegate` work-lane and verified by Claude before it is kept. Phases are **strictly serial** — no phase advances until its gate is green and its review-fix loop has converged. Within a phase, only one lane touches shared files (`lib.rs`, `request.rs`, `overlay.toml`, `build.rs`, `registry/mod.rs`) at a time.

**Implementation lanes** (`delegate <lane> work`):
- **`codex`** (OpenAI) — strongest typing/design discipline + best failure reporting. Owns the architectural pieces: `exec::run`, the capability stages, `request.rs` enrichments, the `BuilderId` registry, the `IntoFlagValues` proc-macro, the validator wiring.
- **`cursor`** (Cursor Composer, fast) — bulk/mechanical: per-group overlay population, handler deletion, repetitive call-site updates.
- **`grok`** (Grok Composer) — alternate mechanical lane; alternates with `cursor` across the per-group waves to spread Composer-subscription load and add model diversity.

**Review lanes:**
- **Native sub-agent** (Opus or Sonnet via `Agent`) — adversarial fresh-context code review at each phase end. Primary reviewer.
- **`delegate codex safe`** — decorrelated second opinion (read-only isolated copy). Run in parallel with the native reviewer on **critical phases** (anything touching confirm-gate, secret-capture, validation, transport, or the request builder): **Phases 1, 2, and 11**.

**Gate** (Claude runs it, never a sub-agent): `cargo xtask ci` (fmt --check, clippy --all-targets --all-features -D warnings, full `cargo test --workspace --locked`) **and** `cargo xtask ergonomics` (the latter adds `cargo run -- capabilities/robot-docs` schema checks against the built binary; it overlaps `ci`'s test run but the binary checks are additive). Both must pass.

---

## 2. The migration contract (the backbone — built first, in Phase 0)

The single artifact that makes the whole run mechanical: a checked-in, machine-readable table (`tests/migration_contract.toml` + a test that validates it against `REGISTRY`) with **one row per operationId**, columns:

- `corpus_argv` — one or more canonical argv vectors for the golden corpus (valid invocation that produces a request preview, not an error), plus any stdin/file fixture names, required env, and confirmation flags for destructive dry-runs.
- `current_handler` / `current_builder` / `current_validator` — the existing `dispatch_*` / `build_*` / `validate_*` fn names (so deletions are verifiable).
- `route_state` — `Legacy` | `Pipelined` (target: all `Pipelined` by Phase 11).
- `capability` — `SecretCapture{field,flag,required}` / `Confirm{protocol}` / `Chunked` / `Macro` / `QueryFromBody` / none.
- `validator_class` — exactly one of `Declarative` (required/enum/range only) | `NamedValidator{id}` (semantic) | `LegacyUntilWave` (not yet migrated).
- `body_builder` — `None` | `BuilderId` (the conditional-body tail).
- `owned_phase` — which phase/wave migrates this op.

A test asserts: every `REGISTRY` op has exactly one contract row; every row's `current_*` names resolve to real functions; `route_state`/`validator_class` are consistent with the code. This table is what Phase 1 reads to know which ops are safe to touch, what the goldens must cover, and what each lane owns. **It removes every hidden cross-phase dependency** the reviews flagged (Codex F1/F2/F7, native #1).

---

## 3. Invariants (true at every commit)

1. **Tree stays green.** `cargo xtask ci` + `cargo xtask ergonomics` pass at every phase commit. Never commit red.
2. **No *unintended* request-shape change.** The Phase-0 golden corpus is diffed after every phase touching request construction. A diff is a **failure unless it is predeclared** in `tests/allowed_golden_diffs.toml` (operationId + JSON-Pointer + before/after matcher + owning phase + reason). The test auto-accepts only predeclared diffs and halts on any other — this is the autonomous accept-vs-regression oracle (Codex F5). Default: zero accepted diffs.
3. **One pipeline, scoped per wave.** "Routes through `exec::run`" is a **per-op** property (`route_state = Pipelined`), enforced by a route-coverage test, not a single Phase-1 flag. Phase 1 pipelines only the already-registry-driven ops + the seeded bug-fix paths; each wave pipelines its group; Phase 11 asserts all 68 are `Pipelined` (Codex F7, native — rescopes the original "all through exec::run after Phase 1").
4. **Canaries enforced** (R5): a CI test counts distinct `BuilderId`/`ValidatorId` in `REGISTRY`; hard-fails if `builders > 20` or `validators > 10`; warns above target (`12`/`8`); a grandfather allowlist pins the set discovered in Phase 0 (re-derived from the current tree, **not** the design doc's cached "16" — native #4 found only 7 discrete `_named_body` fns today). Removing a builder requires decommissioning its ID.
5. **No secret to stdout.** A property test over the **explicitly enumerated** `SecretCapture` op IDs (not a possibly-empty quantification) asserts the captured secret never appears in the response envelope (Codex F1 — non-vacuous).
6. **Stage by name, never `git add -A`.** Branch `refactor/registry-driven-dispatch` (created from `main` at start). Commit per phase; never push.

---

## 4. Lane-orchestration contract (autonomy: how lanes run and fail)

Every implementation-lane invocation follows this contract (Codex F8, native #5):

- **Isolation & review-before-keep:** run `delegate <lane> work --isolation worktree --forbid-commit` so the lane leaves edits in a managed worktree without committing; Claude inspects the diff (`delegate worktree show`), then applies/keeps it on the real tree. Fallback if worktree is unavailable: record `git stash create` of the clean pre-lane state so any lane edits can be reverted wholesale.
- **Ownership:** every lane prompt lists **owned file globs** and **forbidden files**. After the lane returns, `git diff --name-only` must be a subset of the owned globs; any unowned edit is reverted before the gate runs.
- **Failure recovery (deterministic branches):**
  - *Timeout / no usable diff:* inspect the partial diff once; retry the **same** lane once with a narrower prompt. Still bad → switch to the alternate mechanical lane (`cursor`↔`grok`) or, for architectural work, halt-and-summarize.
  - *400 / quota / auth error:* switch lane (e.g. `delegate codex-auth swap`, or `codex`→`cursor`); if all lanes fail, halt-and-summarize with the artifact.
  - *Repeated gate failure (3×):* revert the lane's work, split the phase task into smaller pieces, retry once; still failing → halt-and-summarize.
- **Never** `rm`/`git worktree remove` a Delegate-managed worktree (orphans the registry); use `delegate worktree remove`.

---

## 5. Per-phase review-fix loop

At each phase end, after the lane reports done and Claude has applied the diff:

1. **Verify on disk** (don't trust the lane summary): confirm claimed files/functions changed; finish any trailing call-site/test updates the lane cut off (the known cutoff failure mode).
2. **Run the gate.** Red → fix or send back before review.
3. **Dispatch review:** native sub-agent + (critical phases 1/2/11) `delegate codex safe` in parallel, each briefed with the phase's review focus + the relevant invariants.
4. **Triage** (Claude): real defects vs misreads. **Convergence rule:** every reviewer-flagged **Blocker/critical** finding must be fixed or explicitly waived-with-reason in the phase summary before commit; non-blocking findings may defer to a tracked list. Reviewer disagreement on a critical finding → Claude decides and records the call.
5. **Fix iteration:** real-defect list → cheapest adequate lane (`codex` logic, `cursor`/`grok` mechanical). Re-gate, re-review (focused on fixes only).
6. **Up to 3 rounds.** >5 real defects first pass → expect 2–3 rounds. Still unconverged after 3 → **halt and summarize** (no thrashing).
7. **Commit** once gate-green + converged. The commit message lists what changed and any waived/deferred findings.

---

## 6. Phases

### Phase 0 — Migration contract, oracle, additive scaffolding + seed Phase-1 metadata
**Goal:** build everything Phase 1+ reads, with zero behavior change. **Lane:** `codex`.
**Work:**
- **Migration contract** (§2): generate `tests/migration_contract.toml` by re-running the handler classification against the *current* tree; the validating test.
- **Golden corpus + manifest:** `tests/request_corpus/` capturing `--print-request --dry-run` for every op using the contract's `corpus_argv`, under pinned `SOURCE_DATE_EPOCH`; a regen+diff test. **Caveat row:** `context` is `defined_by_overlay` with no OpenAPI schema (native #8) — its later OpenAPI cross-check is single-witness; mark it.
- **`allowed_golden_diffs.toml`** mechanism (invariant 2), initially empty.
- **Additive registry fields** (default-empty, no reads yet): `OperationDef.{capabilities, body_builder, validators, mixed_status_exit}`; `FieldDef.{co_fields, item_template, enum_values, range}`. **Do NOT add clap-presentation fields** (`help`/`value_name`/`conflicts_with`/`positional`) — those stay in derive attributes (R6). Define `Capability`, `ConfirmProtocol`, `BuilderId`, `ValidatorId` enums. Update `build.rs` + `overlay.toml` schema; reproducible-codegen test stays green.
- **Canary** (invariant 4) with the grandfather list from the re-derived builder set.

> **Refinement during execution (2026-06-30):** the bug-fix metadata seeding (`SecretCapture`/`Confirm`/`mixed_status_exit` rows + the `dangerous=true` flip for `websets-searches-cancel`) is **folded into Phase 1**, atomic with the stages that consume it, gated by non-vacuous tests *before* any deletion. Rationale: the `dangerous` flip is not a no-op Phase-0 seed — `output/envelope.rs:310` feeds `op.dangerous` into the `requiresConfirm` capabilities output, so flipping it in Phase 0 both changes output and creates a "dangerous-but-ungated" intermediate. Codex's F1 fix explicitly allows "seed within Phase 1 before deleting." Phase 0 stays pure infra (no registry-data population beyond the additive defaults from 0a).

**Sub-steps:** 0a additive types (done); 0b golden corpus + argv manifest + `allowed_golden_diffs` mechanism + canary-count infra; 0c migration-contract classification table + validating test.
**Exit gates (each a command+assertion):** `cargo xtask ci` + `cargo xtask ergonomics` green; contract test passes (every op has one consistent row); corpus regen diff empty; canary test green; capabilities-codegen reproducible.
**Review focus:** is the corpus derived from handlers (independent of the new fields)? Are additive fields genuinely unread? Does every op have a valid corpus argv that produces a request preview (not an error)?

### Phase 1 — Bug-class kill (high value, low risk)
**Goal:** every documented bug fixed by routing the *seeded* paths through one pipeline. **Lane:** `codex`. **Critical — dual review.**
**Work:**
- `exec::run(Plan)` wrapping the existing `execute_typed_live`/paginated/streaming/chunk engine.
- **Seed-then-consume, atomically, in this order per capability** (Codex F1): (1) add the overlay/registry data + a **non-vacuous test naming the exact op IDs**; (2) build the stage that reads it; (3) only then delete the old path. Never delete before the data + test exist.
- **Confirm-gate stage from `op.destructive()`** (3 protocols via `Confirm` rows seeded here). **Set `dangerous = true` for `websets-searches-cancel` in this phase** (it changes `requiresConfirm` output, so it lands with the gate, not earlier). Non-vacuous regression test **naming `websets searches cancel`** refuses without confirmation (native #3/Codex F3).
- **`SecretCapture` stage** (seed the 3 rows here — `admin keys create`→`apiKey` required; `monitor create`→`webhookSecret` optional; `websets webhooks create`→`secret` optional; capture before redaction; honor `required`): non-vacuous "no secret to stdout" test over the 3 named ops, **then** delete `dispatch_monitor_create_live`, `dispatch_admin_keys_create_live`, `dispatch_websets_webhooks_create_live`.
- **Wire `validate_registry_input` into the live path, row-based** (Codex F2): only ops whose `validator_class` is `Declarative` or `NamedValidator` route through the shared validator; `LegacyUntilWave` ops keep their existing validator until their wave. Phase 1 may claim "one validator path" only for the non-legacy set.
- **The live-validation oracle** (native #3 — the corpus can't see this): a new fixture of **invalid** `--body`/`--set` invocations per already-registry-driven op, exercised on the **live (non-dry-run)** path, with frozen expected error envelopes. Document the intentional exit-code shift (malformed `--set numResults=five` now → local `usage` exit 1 instead of upstream exit 5) in the phase summary as a contract change.
- Fold in: placeholder-guard (one chokepoint; delete sprinkled calls), pending-run-on-ambiguous-create (keep it — don't drop when folding `execute_typed_live`), `mixed_status_exit` (delete the `command=="contents"` compare). Carve out `raw` (no `OperationDef`/confirm/guard — by design) and give the ~8 bespoke residue a typed dispatch enum.
**Exit gates:** gate green; the 3 duplicate fns gone (grep asserts); confirm + secret tests non-vacuous and passing; live-invalid-input fixture passing; corpus diff empty **or** every diff predeclared in `allowed_golden_diffs.toml`; route-coverage test shows the seeded ops `Pipelined`, the rest still `Legacy` (not a regression).
**Review focus:** any seeded path still bypassing? Secret capture ordering/reservation timing? Confirm invariant covers every destructive op including the fixed one? Did live validation change any error envelope not predeclared?

### Phase 2 — `request.rs` enrichments + `BuilderId` registry (must precede waves, R3)
**Goal:** make `co_fields`/`item_template`/`enum`/`range` and the builder table real before any overlay depends on them. **Lane:** `codex`. **Critical — dual review** (native #7: every wave depends on this).
**Work:**
- Extend `build_flag_body`/`build_request` to apply `co_fields` (G1), `item_template` (G2), with property tests (totality, determinism, edge cases: empty arrays, missing co-field parent, range bounds).
- **R4 resolved deterministically — Option (a):** `enum_values`/`range` live in the overlay/`FieldDef`, consumed by `validate_registry_input`. A **member-level** consistency test asserts that for every field also exposed as a clap `ValueEnum`, the overlay enum set **equals** the clap variant set (closes the judge's drift concern — the test checks members, not just flag names). The proc-macro side-table (Option b) is **explicitly deferred/out of scope** — no runtime fork (resolves Codex F6 / native #1).
- **`BuilderId → fn` registry** with the R7 uniform signature (`fn(&[(&str,Option<String>)]) -> Value`, or `Value`; **not** `fn(&SomeArgs)` — there is no single `Args` type). Wire it into `build_request` with correct precedence (registry fields + builder < `--body` < `--set`). A **sentinel builder** + test proving fields + builder + `--body` + `--set` merge deterministically — **before** any wave uses `body_builder` (Codex F10).
**Exit gates:** gate green; co_fields/item_template property tests passing; enum/range member-level consistency test passing; sentinel builder merge test passing; the 7 already-registry-driven ops re-verified through the enriched builder; corpus diff empty-or-predeclared.
**Review focus:** transforms total/deterministic? Member-level enum consistency real (not name-only)? Builder precedence correct vs `--body`/`--set`?

### Phase 3 — `IntoFlagValues` proc-macro (Option B; unpack-boilerplate only)
**Goal:** delete the `build_*_spec` flag-unpack boilerplate so the waves can *delete* code, not move it. **Lane:** `codex`.
**Work:** `#[derive(IntoFlagValues)]` emitting `fn into_flag_values(&self) -> Vec<(&'static str, Option<String>)>` (kebab-case field→flag; exclude flattened `GlobalArgs`; define how `--body`/`--set`/`--dry-run`/`--yes`/`--confirm` never enter `build_flag_body`). **No validation-metadata responsibility** (that's Phase 2 option-a). Extend the consistency test: every `FieldDef.flag` ↔ a struct field.
**Exit gates:** gate green; macro round-trips every flag for a pilot group; flag↔field consistency test passing; corpus diff empty.
**Review focus:** `GlobalArgs` flatten handling; every flag round-trips; no flag silently dropped.

### Phases 4–10 — Per-group overlay migration (strangler waves, 7 waves)
**Goal:** populate overlay `fields` for the 61 hand-coded ops, route each group through `exec::run`, delete its hand-written builder/validator/handler. **One wave per phase number** (4=search-family verify (5 ops), 5=agent+research (9), 6=monitor (9), 7=websets core (7), 8=websets sub-resources: items/searches/enrichments/imports (16), 9=websets monitors+webhooks+events (15), 10=admin+team (7) — sums to 68; numbering fixed per native #2).
**Lane:** alternate `cursor`/`grok` (mechanical bulk); **escalate to `codex`** for any op with a `body_builder` or `NamedValidator`. **Serial on shared files — one lane at a time.**
**Per wave:** (1) lane populates the group's overlay `fields` + `co_fields`/`item_template`/`enum`/`range`/capabilities + `body_builder` for conditional-body members; flips `validator_class` from `LegacyUntilWave` to `Declarative`/`NamedValidator`. (2) route the group through `exec::run`; delete its builder/validator/handler. (3) diff against the golden corpus **and** the embedded OpenAPI `requestBody` schema (two witnesses; single-witness for `context`).
**Exit gates (per wave):** gate green; group handlers deleted (grep asserts); corpus + OpenAPI diffs empty-or-predeclared; group's `route_state` now `Pipelined`; group's existing tests pass; suggestion-quality golden fixtures (per-field `suggestedCommand`/`didYouMean`) unchanged for the group.
**Review focus:** body-shape parity (the silent-400 risk); did a deleted validator drop a semantic check not moved to a `NamedValidator`; suggestion quality preserved.

### Phase 11 — Property-test suite over the registry + collapse
**Goal:** correctness becomes a property of the table; dead code deleted. **Lane:** `codex` (tests) + `cursor` (deletion). **Critical — dual review.**
**Work:** property tests quantified over `REGISTRY`: every op `--dry-run` does zero network I/O; every `idempotency_sensitive` op never auto-retries unkeyed; every `SecretCapture` op's secret never reaches stdout; every destructive op refuses without confirm; every `cli_path` round-trips (the derived surface parses the op's `corpus_argv`); every op with `enum`/`range` returns identical verdicts from `validate-input` and live dispatch. Route-coverage test asserts **all 68 ops `Pipelined`**. Delete residual dead handlers; confirm `lib.rs` collapse. Itemize any `capabilities --json` output-shape change from the new `OperationDef` fields (native #9).
**Exit gates:** gate green; property suite passing with **no op silently excluded** (count asserts 68); route-coverage = all Pipelined; `lib.rs` line count well below 7,435; final corpus + OpenAPI parity clean.
**Review focus:** do the properties hold for all ops (no exclusions)? Anything in the bespoke residue mis-deleted? `capabilities --json` shape change documented?

---

## 7. Risk register (from fusion + plan review)

| Risk | Mitigation |
|---|---|
| Wrong-but-consistent registry passes self-referential tests | Phase 0 handler-derived corpus + Phase 4+ OpenAPI cross-check (two witnesses; `context` single) |
| Phase 1 consumes empty metadata; deletes live fns unsafely | Phase 0 **seeds** SecretCapture/Confirm/mixed_status + the `dangerous` bit; non-vacuous named tests |
| `websets_searches_cancel` ships still-ungated | Phase 0 sets `dangerous=true`; Phase 1 test names it explicitly |
| Live-validation exit-code shift invisible to dry-run corpus | Phase 1 live invalid-input fixture with frozen envelopes |
| `co_fields`/`item_template`/`body_builder` are "spec fiction" until `request.rs` grows them | Phase 2 implements + sentinel-tests them **before** any wave |
| R4 fork unevaluable / defaults silently | resolved now: Option (a) + member-level consistency test; side-table deferred |
| Canary starts at ceiling | budget(12)/max(20) split + grandfather list re-derived in Phase 0 |
| Autonomous loop can't tell intended diff from regression | `allowed_golden_diffs.toml` predeclared-or-halt |
| Lane timeout/400/unowned-edit strands the run | §4 recovery branches + worktree/forbid-commit + ownership allowlist |
| Subjective exits stall an autonomous orchestrator | every exit is a command+assertion; convergence = no open Blocker |
| Suggestion quality degrades on validator collapse | per-field error golden fixtures per wave |

---

## 8. Definition of done

All phases (0–11) committed on `refactor/registry-driven-dispatch`; gate green; property suite over `REGISTRY` passing (all 68 ops); the 3 duplicate create-live fns, the `command=="contents"` leak, the sprinkled placeholder guard, and the two-validator drift all gone; `websets_searches_cancel` gated; `lib.rs` collapsed; golden corpus + OpenAPI parity clean (only predeclared diffs). Branch left for Trey's review — **not pushed**. Final summary: what each phase changed, any `capabilities --json` shape change, and deferred items (the Option-A clap spike; the Option-b validator side-table).
