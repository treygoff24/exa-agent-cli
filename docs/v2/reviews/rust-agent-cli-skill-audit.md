# Reviewing source: `rust-agent-cli` skill audit

Date: 2026-06-29

**Status: actioned.** All must-fixes, should-fixes, nice-to-haves, and both doc-to-doc contradictions below were applied to the v2 set and ratified as decisions **D23–D39** in [`../decisions.md`](../decisions.md). This document is retained as the review record; where a finding cites a doc section, that section now reflects the fix.

Method: the v2 design set (`decisions.md`, `contracts.md`, `commands.md`, `architecture.md`, `implementation-plan.md`) was audited against the `rust-agent-cli` skill — its `SKILL.md`, `references/design-principles.md`, `agent-cli-contract.md`, `clap-patterns.md`, `implementation-patterns.md`, `state-and-persistence.md`, `headless-auth.md`, `doctor-mode.md`, `testing-release.md`, and `distribution.md`. Five parallel lanes (contracts, command surface, architecture, auth/doctor/distribution, testing/CI/release), each reconciled against an independent full read of the docs.

Bottom line: the design set is already well above baseline — it internalizes most agent-CLI wisdom (one transport chokepoint, structural redaction, registry-driven safety metadata, no-auto-retry-on-create, golden-pinned contract surfaces, an explicit invariant→test matrix). The findings below are gaps the spec simply hasn't written down yet, plus two doc-to-doc contradictions. The Codex plan review's must-fixes are already folded in, so testing/CI/release surfaced few *new* issues.

---

## Must-fix

These are contract-coherence or correctness issues — fixing the spec now prevents baking a contract bug into the build. M1–M3 are Phase-1/contract-core; M4 is a release-phase blocker.

### M1 — The `error.code` vocabulary is never published as a dictionary

**Where:** `contracts.md` §5, §13 (`"exitCodes": {}` is an empty placeholder; there is no `errorCodes` key), §14.

`contracts.md` §6 instructs agents to "branch on `error.code` + `error.retryable` first," and §5 says every error carries a "stable machine string" `code` — but the set of possible `code` values appears nowhere. Only one example (`invalid_flag_combination`) exists. The contract elevates `error.code` to the primary branch signal and then leaves it undiscoverable.

**Change:** Add an enumerated, golden-pinned error-code dictionary (each `code` → `category`, default `retryable`, one-line meaning) to `contracts.md` §5; populate `errorCodes` and `exitCodes` in `capabilities --json` (§13); add both to the §14 golden set.

### M2 — clap's default exit code (2) collides with `auth` (2); parse errors aren't specified to be remapped/wrapped

**Where:** `contracts.md` §6 (exit `1 = usage`, `2 = auth`); `architecture.md` §6/§10; `commands.md` §4. *(Confirmed independently by the contracts and command-surface lanes.)*

clap's native parse errors (unknown flag, bad `ValueEnum`, missing positional, `conflicts_with`/`ArgGroup`/`range` violations) exit **2** by default and print plain text — not `exa.cli.error.v1`. The contract reserves exit **2 for auth**. Unless every clap error is intercepted, remapped 2→1, and re-rendered through the envelope, a malformed invocation will (a) exit with the code reserved for auth and (b) print non-JSON to stderr — so an agent reads "auth failure" and tries to re-authenticate instead of fixing the flag. The docs never state this interception. (The small-integer dictionary itself is a permitted "stronger convention" per `SKILL.md` — the bug is the un-remapped collision, not the choice of integers.)

**Change:** In `contracts.md` §6 and `architecture.md` §6/§10, state the rule: clap parse errors are caught via `try_parse`, remapped to exit **1 (usage)**, and rendered as `exa.cli.error.v1` on stderr (never raw clap text); `--help`/`--version` still pass through to stdout at exit 0. Mirror clap's nearest-candidate suggestion into `error.details.didYouMean`. Add a parse-error-envelope golden.

### M3 — `--idempotency-key` is wired as a local retry gate only; upstream forwarding is never specified

**Where:** `architecture.md` §5 (`RetryPolicy::classify(..., keyed)`), §4 (`RequestSpec`); `commands.md`:228; `contracts.md` §7; `decisions.md` carry-over validations.

The key is used solely as a local boolean deciding whether retry is permitted. No doc specifies that transport injects it as an upstream `Idempotency-Key:` header at the chokepoint, nor that Exa honors it for dedup. If the key isn't forwarded/honored, a keyed auto-retry of a create double-bills *exactly* like an un-keyed one — voiding D7's central money-safety distinction. Worse, the carry-over validations list "whether key-create returns a one-time secret" but **not** "does Exa support idempotency keys" — the capability the whole safety model rests on is unverified.

**Change:** In `architecture.md` §5 add: for `idempotency_sensitive` ops with `--idempotency-key`, transport injects `Idempotency-Key: <key>` upstream at the chokepoint (alongside `Authorization`) — this is what makes keyed auto-retry non-double-billing. Add "Verify Exa honors a client idempotency-key header for create-POSTs; if not, keyed auto-retry must be disabled" to `decisions.md` carry-over validations.

### M4 — The `curl | sh` installer contract is essentially unspecified (release-phase blocker)

**Where:** `implementation-plan.md` → Release / distribution → "Install surface" (one sentence + a silent-fallback note).

The `release.yml` side is detailed, but the install side commits to none of `distribution.md`'s non-negotiables: no checksum-verify-*before*-install, no non-TTY-safe defaults, no flag/env drive (`--yes`/`--dest`/`--version`/`--no-verify`), no idempotent re-run, no no-sudo/PATH story, no grep-able `INSTALL_OK …` final line. The one behavior described — "musl/gnu mismatch silently falls back to source compile" — is itself the silent-fallback footgun the rubric warns against. This is the agent's primary acquisition path.

**Change:** Add an "Installer contract" subsection enumerating the non-negotiables, or adopt **cargo-dist** to generate the matrix + `SHA256SUMS` + a baseline `install.sh` with platform-detect + verify-before-install already wired. Hard-fail on checksum mismatch (warn+skip only if no `sha256sum`/`shasum`); never silently source-compile.

---

## Should-fix (close concrete rubric gaps; mostly cheap)

**Contract / envelope**

- **S1 — Surface per-command blast radius in `capabilities`.** The registry already carries `dangerous` and `idempotency_sensitive`, but `capabilities.commands[]` (contracts §13) exposes only `readOnly`/`dangerous`/method — not `idempotencySensitive`. An agent can't tell which creates need `--idempotency-key` before calling. Add `idempotencySensitive` (and an explicit `destructive`/`requiresConfirm`) per command. *(design-principles "Annotate blast radius.")*
- **S2 — Add success-path `nextActions`.** The error envelope's `suggestedCommand` is excellent, but the *success* envelope has no follow-up hint. After `agent run`/`websets create`, the obvious next step (`… runs events <id> --stream`, `… get <id>`) lives only in `robot-docs`. Add optional `nextActions[]` to the success envelope for async-create + paginated commands. *(design-principles "Return next-step hints"; anti-pattern "output without the next move.")*
- **S4 — Add a default output ceiling.** §9 names the hazard ("`contents --text` … 100k+ tokens … blows the agent's context window") but the only mitigations are manual `--output` and deferred auto-spill. Add a universal `--max-output-bytes N` with a conservative default (complements, doesn't relitigate, D10). *(principle 9.)*
- **S5 — Add `--correlation-id`.** The envelope carries a locally-generated `requestId` but no way for an orchestrating agent to stamp its own id and have it echoed across stdout/stderr/`--trace`. Add a global `--correlation-id` (+ `EXA_CORRELATION_ID`) echoed into `request.correlationId`. *(agent-cli-contract global-flags.)*
- **S8 — Complete the streaming event shape.** `exa.cli.event.v1` is `{schema, command, seq, eventId, event}` — no top-level `type` kind discriminator, no `timestamp`, no correlation. Add them so an agent can route events without unwrapping the opaque upstream blob. *(SKILL "Emit structured results.")*

**Architecture**

- **S9 — Spec the pending-run JSONL write contract.** The record *schema* is frozen, but the *write* is only "append-only JSONL": no `O_APPEND` single-line atomicity, no flush-before-exit, no concurrent-writer story, no bound/rotation — and this breadcrumb is written at the worst moment (an ambiguous *billable* create). A torn line loses the only recovery handle. Specify the atomic-append + flush contract; add `tempfile` if temp+rename is adopted. *(state-and-persistence "Atomic JSONL write" / "Crash-safe idempotency keys.")*
- **S10 — Give the command-handler layer a home.** §5 says "a handler returns a `Plan`," but the §1 module tree has no `commands/` dir and `cli/<group>.rs` is "the only place clap types live." Either add `src/commands/` or state explicitly that dispatch is fully generic and registry-driven (no per-command handlers) — and make §5 consistent. *(SKILL step 3.)*
- **S11 — Guard `--body -` / `@file` stdin reading.** §4 names `-` as a body source but never specifies the stdin-TTY guard. Reading `--body -` when stdin is a TTY hangs the agent. Add a shared `read_input` that rejects `-` when `stdin().is_terminal()` with `CliError::NoInput` (exit 11). *(implementation-patterns "Reading stdin safely.")*
- **S3 — Input forgiveness.** clap `ValueEnum` is case-*sensitive* by default; the design leans on it (D13) but never mandates `ignore_case = true`, so `--type Fast`/`--format JSON`/`--effort Medium` hard-fail. Add: `ignore_case = true` on every `ValueEnum`; `BoolishValueParser` for the optional bools (`--text[=1|yes|on]`); make `--category` a `ValueEnum` (multi-word variants) so typos get suggestions; add a placeholder-literal guard (`<id>`, `$VAR`, `YOUR_*`, `…`) on ID/URL/query positionals. *(design-principles "Input forgiveness.")*
- **S14 — Bake binary provenance.** `capabilities` carries the *spec* SHA and tool `version` but not the binary's commit SHA / build date / target triple, so neither an agent nor `doctor` can detect a stale build. Bake `GIT_SHA`/`BUILD_DATE`/`target` (build.rs or vergen) into `capabilities` + `--version`. *(distribution "`--version` carries provenance.")*

**Auth / doctor**

- **S6 — Reconcile the doctor contract.** `architecture.md` §9 maps doctor's exit onto the *general* category dictionary (auth=2/config=3/network=4) instead of the linter dictionary `doctor-mode.md` prescribes — so warns-only → exit 0 (healthy and degraded are indistinguishable by exit code), there's no exit-1=findings, and doctor exit 2/3 collides with the general meanings the agent already learned. Doctor output also has no schema id (`exa.cli.doctor.v1` absent) and isn't golden-pinned. Adopt **0 = healthy / 1 = findings present** (keep per-finding `category` in JSON), add `exa.cli.doctor.v1`, golden-pin one report, publish the detector ids + exit mapping in `capabilities`. Also reconcile the `--check` vocabulary (commands lists `webhooks,limits` — no detectors in §9; §9 has `tty.discipline`/`binary.version`/`service-key.scope` not in `--check`) and give `service-key.scope` a fix command.
- **S7 — Strengthen the auth-error contract.** There's no dedicated `not_authenticated` code, no `details.checked` ladder (which rungs were tried: `--api-key`/env/keyring/config), and local-missing vs upstream-401 are conflated under one exit-2 example — so an agent can't branch "set a key" vs "rotate a revoked key." Add distinct `not_authenticated`/`reauth_required` codes with `details.checked` and remediation naming `EXA_API_KEY` + `auth login`; golden-pin it. Separately, state that keyring reads are **best-effort / non-blocking** and fail fast to the next rung (a macOS Keychain GUI prompt otherwise hangs a per-call agent). *(headless-auth.)*

**Testing / CI**

- **S12 — Add supply-chain scanning.** The only dep-graph checks are the custom `cargo tree -i` ban-list guards (no tokio/openssl/serde_norway) — they enforce architecture, not RUSTSEC/license policy. For a binary shipped to many agents over TLS, add `cargo deny check` + `cargo audit` to `xtask ci` (committed `deny.toml`) and make `cargo audit` a hard gate before `publish-crates`. *(testing-release "Recommended additional checks.")*
- **S13 — Pin help text, error hints, and the parser/deserialize tiers.** §14 has no `--help` snapshot, yet the agent reads `--help` top-down; only one representative of each self-description family is golden-pinned. Add per-command help-content assertions (key-line, not full-layout), a golden per §6 error category's `suggestedCommand`, a named parser tier (`Cli::try_parse_from`: `--num-results` 0/101 range reject, `ValueEnum` rejection, defaults, help/version semantics), and an `envelope_roundtrip_deserialize` tier (every envelope fixture parses back into its struct). *(checklist 11; SKILL §8.)*

**Command surface**

- **S15 — Reciprocal did-you-mean for confusable pairs.** `monitor` (`/monitors`) vs `websets monitors` (`/v0/monitors`) differ only by singular/plural+nesting — a near-guaranteed mistype with no cross-redirect. Same for `--num-results` (search) vs `--count` (websets), where D20 only handles `--limit`→`--num-results`. Add custom did-you-mean both directions (don't alias — that reintroduces the ambiguity D20 avoided).
- **S16 — Disambiguate `--dry-run`.** Three notions are collapsed under one flag: local `--print-request` (no network), `--dry-run` "preview" on mutations (network cost unstated), and genuine server-side `dry_run` that spends a request (`monitor batch` defaults `dry_run: true`). An agent can't tell whether `--dry-run` is free/offline or costs a call. Pin per-command semantics and mark which previews touch the network.

---

## Nice-to-have

- **N1 — `dataHash` on the success envelope** for cheap drift/dedup detection (deterministic for the offline surfaces; a change-fingerprint, not a guarantee, for live results). *(checklist 8; anti-pattern "output without a data_hash.")*
- **N2 — `describe` alias of `capabilities`; list `ask`/`fetch` in the §1 command tree** (they're real top-level commands but appear only in trailing prose). *(principle 1, 7.)*
- **N3 — Enumerate the volatile fields in `contracts.md` §12** (`requestId`, `upstreamRequestId`, `durationMs`, `retries`, `attemptedAt`) so the determinism claim and the §14 goldens are mutually coherent.
- **N4 — Empty result populates a `warnings[]` broaden hint; terminal rate-limit error carries `details.retryAfterMs`** so the agent's own backoff can use the server-advised wait.
- **N5 — `--api-key-stdin`/`@file`** (argv leaks to `ps`/history/transcript); **`auth logout`** (clears the keyring entry); **`[package.metadata.binstall]`** (the no-compile acquisition path); generated completions/man pages — or record them as intentionally out-of-scope for an agent-only CLI.
- **N6 — Agent auto-configuration on install** (idempotent skill-drop + AGENTS.md marker-block merge pointing at `capabilities --json`/`robot-docs guide`). `distribution.md` calls this "the single most important deliverable of the whole install," but auto-editing AGENTS.md is intrusive — flagging as a **product-scope decision**, not a clear must.
- **N7 — Precision items:** spec the registry merge precedence (overlay-vs-spec conflict rule) + deterministic codegen ordering (so `registry_codegen_reproducible` is meaningful); name where `--timeout` is enforced at the chokepoint; tighten the gate to `clippy --all-targets --all-features` + `cargo doc`; add a true two-invocation byte-identical determinism test (current one is in-process serialize-twice).

---

## Doc-to-doc contradictions to reconcile

1. **`--body` semantics.** `commands.md` §3 says `--body` "bypasses named-flag assembly"; `architecture.md` §4 says `--body` is "deep-merged over the flag-built object" (last-writer-wins: `flags < --body < --set`). These can't both hold. Decide whether `--body` + named flags is a **conflict error** (cleaner, deterministic) or a **merge**, and state it once.
2. **Doctor `--check` vocabulary** (`commands.md`: `auth,config,spec,connectivity,webhooks,limits`) vs the detector ids in `architecture.md` §9. Reconcile to one list (see S6).

---

## Sound parts (do not redo)

- The transport chokepoint (one swappable `Transport`, `RetryPolicy::classify` as the single retry authority below every command so D7 can't be bypassed) is exemplary.
- Redaction is structural, not disciplinary — `Secret` newtype + scrub at the one serialization boundary, so a new error variant can't reintroduce a leak.
- The registry build boundary (data-table-not-client; `const` hot path + `include_bytes!` cold path; three consistency tests) is the right way to keep full-surface coverage honest.
- Output-flag consolidation (D6), the auto-JSON-when-piped default (D3), and the stdout-data/stderr-diagnostics `Sink` are all on-rubric.
- The no-auto-retry-on-create rule + frozen `exa.cli.pending_run.v1` recovery record is best-in-class money-safety design (the gap is only forwarding/verifying the key — M3).
- The invariant→test matrix, the offline-golden vs gated-live-smoke split, the spec-drift conformance job, and the admin/service-key separation are all spec-grade.
