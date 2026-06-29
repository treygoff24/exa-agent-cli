# Reviewing model: OpenAI Codex

## Top 3 findings

1. **Must-fix:** `implementation-plan.md` does not schedule the full `commands.md` surface: `team`, most `schema`, most `robot-docs`, `auth`, and `config` have no build phase.
2. **Must-fix:** Phase 1's `raw` spine is useful but not sufficient: it bypasses the registry-driven typed path that later phases depend on.
3. **Must-fix:** The plan has CI ingredients but no single canonical green command or per-phase runnable gate.

## Must-fix

### 1. Full command surface is not scheduled

**Docs/sections:** `implementation-plan.md` §Phase 1, §Phase 2, §Phase 4, §Phase 5, §Phase 6; `commands.md` §1 Command tree, §4 Per-command flag reference, §Self-description surfaces, §6 Macros.

`commands.md` includes `team info`, `schema show/export/validate-input/refresh`, `robot-docs commands/errors/examples/prompts`, `auth status/test/login`, and `config list/get/set/unset/path/profiles ...`. The plan only explicitly lands `capabilities --json`, `schema list --json`, `robot-docs guide`, `doctor`, `raw`, typed API families, macros, and admin. That leaves several v1 leaves with no phase, acceptance criteria, or tests.

**Concrete suggested change:** Add a command-coverage table to `implementation-plan.md` after Phase 6 with one row per top-level namespace from `commands.md` and columns `phase`, `acceptance`, `test/golden`. Then edit phases as follows:

- Phase 1: add `schema show`, `schema export`, `schema validate-input`, `schema refresh --check`, `robot-docs commands/errors/examples/prompts`, `config path/list/get/set/unset/profiles ...`, `auth status`, and `auth login` behind the keyring feature.
- Phase 1 or Phase 2: add `auth test` as an opt-in online probe using the same smoke gating as live tests.
- Phase 2: add `team info` as the read-only account/limits command.
- Phase 2: explicitly pin all deprecation behavior from `commands.md` §5, not only `similar`.

### 2. `raw` in Phase 1 does not de-risk the typed registry path

**Docs/sections:** `implementation-plan.md` §Phase 1; `architecture.md` §3 Build-time registry codegen, §4 Request builder, §6 clap surface.

`raw METHOD PATH --body` exercises auth, HTTP, envelope, redaction, and exit mapping, but it can bypass the load-bearing path for typed commands: `OperationDef.cli_path` -> clap leaf -> named flags -> `FieldDef` request assembly -> registry safety/pagination/idempotency metadata. Phase 2 is currently the first time a real typed command proves that path.

**Concrete suggested change:** In Phase 1, add one registry-driven typed command smoke before any live API dependency, e.g. `exa-agent search "q" --num-results 1 --print-request --dry-run --json`, plus a fixture-backed transport case. Add acceptance: "a typed command resolves through registry metadata, builds the expected request body, redacts the printed request, and emits the normal envelope without network." Add tests named `typed_spine_search_print_request_golden` and `registry_clap_bidirectional_consistency`.

### 3. No canonical local green command / phase gates

**Docs/sections:** `implementation-plan.md` §Testing strategy, §CI, all phase Acceptance blocks.

The plan lists CI steps, but there is no single command a maintainer or agent can run to answer "is the build green?" Acceptance bullets are also uneven: some are behavioral prose, some are live smokes, and most do not name the test command that gates the phase.

**Concrete suggested change:** Add a `## Canonical gates` section:

```bash
cargo xtask ci              # offline, deterministic: fmt, clippy, unit/integration/goldens, actionlint when present
cargo xtask phase-gate 1    # only tests/snapshots required for Phase 1
cargo xtask phase-gate 2
cargo xtask smoke --budget "$EXA_E2E_BUDGET"  # opt-in live Exa
```

Then edit every phase Acceptance block to include `Gate: cargo xtask phase-gate N` and list the exact test names/snapshots that must pass. Keep live smoke out of the default green command unless explicitly requested with `EXA_E2E=1`.

### 4. Invariant-to-test mapping is incomplete and mostly unnamed

**Docs/sections:** `implementation-plan.md` §Testing strategy; `contracts.md` §1, §6, §7, §12, §14; `architecture.md` §3.

The test layers mention the right themes, but the plan does not provide a named invariant matrix. Several load-bearing invariants are only described generically, which makes them easy to miss during implementation.

**Concrete suggested change:** Add a table under `implementation-plan.md` §Testing strategy named `Invariant regression matrix` with these exact test names:

- `golden_capabilities_json`, `golden_schema_list_json`, `golden_robot_docs_guide`, `golden_success_envelope`, `golden_error_envelope`, `golden_raw_passthrough`, `golden_exit_code_table`, `golden_streaming_ndjson`, `golden_paginated_all_ndjson` for `contracts.md` §14.
- `retry_create_post_matrix_no_unkeyed_retry` for D7/§7.
- `stdout_data_stderr_diagnostics_split` for §1.
- `exit_code_dictionary_all_categories` for §6.
- `redaction_all_sinks_trace_suggested_command` for §12.
- `deterministic_envelope_field_order_twice_serialize` for §12.
- `non_tty_no_color_no_ansi` for §1/§12.
- `registry_clap_bidirectional_consistency`, `registry_idempotency_matches_contract_create_list`, and `registry_dangerous_requires_confirmation` for `architecture.md` §3.

### 5. Create-POST retry coverage omits or conflicts on some create operations

**Docs/sections:** `contracts.md` §7 Retry & idempotency; `commands.md` §1 Command tree, §2 Operation-to-command mapping; `implementation-plan.md` §Testing strategy item 4.

`implementation-plan.md`'s no-retry test list omits `admin keys create`, even though `contracts.md` §7 includes it. `commands.md` also marks `websets monitors create` as `[create-POST]`, but `contracts.md` §7 does not list it. `websets webhooks create` creates a resource/secret but is not marked `[create-POST]`. This can produce an overlay/contract mismatch or, worse, a create operation that auto-retries without an idempotency key.

**Concrete suggested change:** In `contracts.md` §7, replace the hard-coded sentence with: "Affected operations are exactly the registry entries with `idempotency_sensitive = true`; the current list is ..." and enumerate every create resource that must not auto-retry. Then update `implementation-plan.md` §Testing strategy item 4 to say the test iterates the registry's `idempotency_sensitive` set and asserts it equals the contract list, including `admin keys create` and the resolved websets monitor/webhook decisions.

### 6. Registry codegen/package story conflicts with D9/D17

**Docs/sections:** `decisions.md` D9 and D17; `implementation-plan.md` §Phase 1 and §cargo publish checklist; `architecture.md` §3 Build-time registry codegen.

D9 says both the OpenAPI snapshot and generated registry are committed. The architecture generates `registry.rs` into `$OUT_DIR`, and the publish checklist only requires packaging the vendored spec snapshot. D17 also makes `openapi/overlay.toml` a build input, but the publish checklist does not mention packaging or diffing it.

**Concrete suggested change:** Amend the plan to require `openapi/exa-openapi.json`, `openapi/overlay.toml`, and the generated registry artifact to be committed and included in `cargo package --list`. Add a gate named `registry_codegen_reproducible` that runs `cargo xtask vendor-spec && cargo xtask generate-registry && git diff --exit-code openapi src/registry` before publish. If the design intentionally prefers `$OUT_DIR` only, update D9 instead; otherwise the plan should follow D9.

## Should-fix

### 7. Blocking SSE + SIGINT needs a timely-interrupt design, not just a flag

**Docs/sections:** `implementation-plan.md` §Phase 3 and §Crate-dependency bring-up checklist; `architecture.md` §5 Transport layer; `contracts.md` §8 Streaming contract.

The plan says `ctrlc` sets an interrupt flag checked in the blocking SSE read loop. A blocking `Read` may not wake promptly on Ctrl-C if the server stalls, so exit 12 can hang until more bytes arrive or a long timeout fires.

**Concrete suggested change:** Add a Phase 3 implementation requirement: SSE reads must use a short read timeout/poll interval (for example <=250ms) or another wakeable mechanism so SIGINT exits 12 within 1s while preserving the last `eventId`. Add an integration test named `sse_sigint_stalled_stream_exits_12_with_last_event_id` using a local fixture server that sends one event, stalls, sends SIGINT, and asserts the timeout.

### 8. `--print-request` contradicts the command reference

**Docs/sections:** `commands.md` §3 Request assembly & preview; `implementation-plan.md` §Phase 1; `architecture.md` §4 Request builder.

`commands.md` says `--print-request` does not call the API "unless `--execute` is also passed", but `--execute` is not in the global flags, contracts, architecture, or plan. The plan and architecture both make `--print-request`/`--dry-run` short-circuit before transport.

**Concrete suggested change:** Delete "unless `--execute` is also passed" from `commands.md` §3, or add `--execute` everywhere and test it. Prefer deletion: it keeps preview mode simple and matches the current plan.

### 9. Feature-matrix CI should enforce the static-binary dependency decisions

**Docs/sections:** `implementation-plan.md` §CI, §Release / distribution, §Crate-dependency bring-up checklist; `decisions.md` D14, D15, D21.

The plan cross-builds musl artifacts, but it does not add an explicit feature/dependency regression gate for "no tokio, no OpenSSL, no YAML parser in the shipping binary" or for the no-keyring musl feature set.

**Concrete suggested change:** Add CI steps:

```bash
cargo check --locked --no-default-features --features musl-static
cargo tree -e features -i tokio --target x86_64-unknown-linux-musl && exit 1 || true
cargo tree -e features -i openssl --target x86_64-unknown-linux-musl && exit 1 || true
cargo tree -e features -i serde_norway -p exa-agent-cli && exit 1 || true
```

Use the real feature name chosen in `Cargo.toml`; the point is to fail if D14/D21 drift into the runtime dependency graph.

### 10. Live smoke flake/cost policy is under-specified

**Docs/sections:** `implementation-plan.md` §Testing strategy item 5, §CI `smoke.yml`, §Phase 6.

The plan has budget caps, but "treat 429 as a controlled skip" can hide rate-limit/retry regressions, and Phase 6 makes live smoke the final gate without a clear failure policy for paid or flaky surfaces.

**Concrete suggested change:** Add `tests/smoke/manifest.toml` with per-test `max_cost_dollars`, `paid = true|false`, `retry_budget`, and `flake_policy`. Make 429 a skip only for explicitly marked quota-sensitive tests and require the test to log `httpStatus`, `Retry-After`, and the redacted command. Keep release blocking to the non-paid smoke subset unless `EXA_E2E_PAID=1` is set.

### 11. Phase 6 depends on a local absolute skill path

**Docs/sections:** `implementation-plan.md` §Phase 6.

Phase 6 references `/Users/treygoff/.claude/skills/.../SKILL.md` and says fixes land on `main`. That is not portable, not CI-runnable, and not a clean release gate for a greenfield CLI plan.

**Concrete suggested change:** Replace the absolute skill dependency with a committed `tests/ergonomics/` harness and a checklist in `docs/v2/ergonomics-checklist.md`. Keep the skill as an optional audit aid, but make the release gate `cargo xtask ergonomics` plus the named intent-mistake tests.

## Optional

### 12. Stale open-seam text should be removed after D14-D21

**Docs/sections:** `implementation-plan.md` §Open seams; `architecture.md` §Open seams; `commands.md` §Open seams.

Several retained open-seam bullets still say "needs sign-off" or "confirm" even though the headers say the coordinator resolved them via D14-D21. This is traceable but easy for an implementer to read as still open.

**Concrete suggested change:** Move the old unresolved text into a `Historical seams` subsection and add a one-line final state to each bullet, e.g. "Final: D15 accepted; no further sign-off required." Do not leave "needs sign-off" in the active plan.

## Sound parts

- Moving `raw` into Phase 1 is directionally right for transport/auth/redaction/envelope/exit-code smoke coverage.
- The offline contract/golden vs opt-in live-smoke split is sound.
- The `ureq` + rustls + no-async-runtime decision is coherent for a per-call CLI, provided the SIGINT/SSE and feature-matrix gates above are added.
- The registry + overlay design is the right way to keep full-surface coverage honest; it just needs the packaging/reproducibility gate tightened.
