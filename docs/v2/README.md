# exa-agent — v2 design set

Date: 2026-06-29
Status: design-complete. This is the canonical design for the Rust rebuild of the Exa agent CLI. It supersedes the v1 docs in `docs/` (kept for traceability). Ready to start [Phase 1](implementation-plan.md#phase-1--skeleton-contracts-raw-offline-self-description).

## What changed from v1

v1 was a language-agnostic research/design pass. v2 commits to **Rust, a single static binary**, and bakes in the review feedback: consolidated output flags, auto-JSON-when-piped default, a no-auto-retry-on-create rule, a context-window-aware `--output`, a deliberately *lean* (read-only) doctor, a gated admin namespace, and a trimmed v1 surface (the preset/profile system is deferred).

## Read in this order

1. **[decisions.md](decisions.md)** — the 39 locked calls + rationale (ADR-style): D1–D22 (original + reconciliation) and D23–D39 (the skill-audit pass). Start here; everything else implements these. **Canonical on any conflict.**
2. **[contracts.md](contracts.md)** — the agent-facing wire/output spec: envelopes, exit codes, retry/idempotency, streaming, pagination, batch, redaction, determinism. **Canonical for schema ids, field names, exit codes.** This is what `robot-docs` is generated from.
3. **[commands.md](commands.md)** — the full command tree (~20 namespaces, ~100 leaf commands), consolidated flag taxonomy, per-command reference, deprecations, macros.
4. **[architecture.md](architecture.md)** — the Rust build-shape: crate layout, dependencies, build-time registry codegen, request builder, transport chokepoint, clap surface, output module, auth/config, doctor, error model.
5. **[implementation-plan.md](implementation-plan.md)** — phased build (raw-first), testing strategy (insta goldens + live smoke + spec-drift conformance), CI, release/distribution, crate bring-up checklist.
6. **[autonomous-implementation-plan.md](autonomous-implementation-plan.md)** — execution overlay for a long `/goal` run using native implementation subagents, Delegate Cursor work lanes, and required native + GLM review gates.

## The locked decisions at a glance

| # | Decision |
|---|---|
| D1 / D14 | **Rust**, single static binary; HTTP via **`ureq` + rustls**, **no `tokio`** |
| D2 | Binary `exa-agent`, crate `exa-agent-cli` (the `exa` name is taken/contested) |
| D3 | Default output **auto**: JSON when piped, human in a TTY |
| D4 | **Admin** (`exa-agent admin keys …`) ships in v1, gated: separate `EXA_SERVICE_KEY`, admin host, confirm-by-key-id delete |
| D5 / D19 | Stateless client, **no cache** (writes config/keyring/pending-run/trace/spill, none a cache) |
| D6 | Output flags consolidated: `--format` + `--json`/`--ndjson` aliases + single `--raw` + `--pretty`/`--compact` |
| D7 | **Never auto-retry a create-POST** without `--idempotency-key`; ambiguous create → pending-run record + recovery command |
| D8 | **Lean doctor**: read-only diagnostics, no `--fix`/undo/backup machinery in v1 |
| D9 / D17 / D21 / D22 | Registry generated at build time from the committed **normalized-JSON** specs (`exa-spec.json` Public API 2.0.0 + Team-Management) + `overlay.toml`, which may also fully define docs-only ops (`/context`); generated into `OUT_DIR`, not committed; no YAML parser in the binary |
| D10 | `-o/--output FILE` + threshold-gated auto-spill to protect the agent's context window |
| D11 / D15 | Env-first auth, optional keyring (feature-gated off for musl artifacts), scopes `exa-agent:api:<profile>` / `exa-agent:service:<profile>` |
| D12 | v1 trims the preset/profile system; keeps `ask`/`fetch` macros + `--profile` + minimal config |
| D13 | `clap` v4 derive; its suggestion engine satisfies the intent-inference axiom |
| D16 | OpenAI-compat deferred post-v1; `raw` covers it in v1 |
| D18 | `--header` cannot override managed auth headers (refused) |
| D20 | Search count is `--num-results`/`-n`; `--limit` on search → did-you-mean, never aliased |

Full text and rationale in [decisions.md](decisions.md).

## How this was built (and reconciled)

`decisions.md` and `contracts.md` were authored as the source of truth, then three subagents drafted `architecture.md`, `commands.md`, and `implementation-plan.md` in parallel against that foundation. The coordinator then reconciled every flagged seam. Resolutions:

- **HTTP/async stack** — the architecture and plan drafts disagreed (`ureq`/blocking vs `reqwest`/`tokio`). Resolved to **`ureq`, no `tokio`** (D14): `reqwest::blocking` embeds a tokio runtime, which a per-call CLI shouldn't carry. SSE is a blocking line reader; SIGINT uses an interrupt flag (→ exit 12).
- **Spec embedding** — YAML-embedded vs JSON-vendored. Resolved to **normalized JSON** (D21): `xtask` converts the upstream YAML once; the binary embeds JSON and ships no YAML parser.
- **`admin keys create` retry** — added to the create-POST no-retry list and the rule generalized to "non-idempotent create-POST" (contracts §7).
- **`diagnostics.cache`** — removed from the envelope (permanently-null under D5).
- **Keyring scopes** — locked to `exa-agent:*` (D15).
- **Search count flag** — `--num-results`/`-n` canonical; `--limit` gives a did-you-mean (D20).
- **`--header` auth override** — refused (D18). **OpenAI namespace** — deferred (D16).

Each subagent doc keeps its original `## Open seams` section under a "Resolved by the coordinator" banner for traceability.

## Review pass (2026-06-29)

After the set was assembled, two independent reviewers checked the plan: the `plan-reviewer` subagent (opus) and Codex (work-mode). Their reviews are in [`reviews/`](reviews/). The patch that followed:

- **Spec sourcing is now a Phase-0 blocker (D22, verified live).** The canonical `exa-spec.json` is "Exa Public API" 2.0.0; the partial specs sitting in `work/research/` are stale and must not be vendored.
- **`/context` is overlay-defined** — it's docs-only (absent from the OpenAPI), so the registry defines it via `overlay.toml`; admin keys come from the separate Team-Management spec.
- **create-POST no-retry list reconciled** — added `websets webhooks create`, `websets monitors create`, `admin keys create`; the list is now registry-driven with a test asserting it equals the registry's `idempotency_sensitive` set.
- **Registry build story fixed (D9)** — inputs committed, generated registry lives in `OUT_DIR`; the publish checklist now packages `overlay.toml` (its omission would break a published build).
- **Executable gates added** — a canonical `cargo xtask ci` + per-phase `phase-gate N`, a named invariant-test matrix, plus the previously-missing `--header`-refusal, keyring-seam, musl-feature-set, and stalled-SSE-SIGINT tests.
- **Phase 6 ergonomics is now a committed harness**, not a local skill path; `--print-request`'s phantom `--execute` flag removed.

## Skill-audit pass (2026-06-29)

A second review audited the whole set against the `rust-agent-cli` skill (five parallel lanes: contracts, command surface, architecture, auth/doctor/distribution, testing/CI/release). The findings and the changes they drove are in [`reviews/rust-agent-cli-skill-audit.md`](reviews/rust-agent-cli-skill-audit.md); the resulting decisions are **D23–D39** in [decisions.md](decisions.md). The load-bearing fixes:

- **clap's default exit 2 collided with `auth`** — parse errors are now caught and remapped to exit 1 + the JSON error envelope (D23).
- **the `error.code` vocabulary is now published** (`capabilities.errorCodes`/`exitCodes`, golden-pinned) instead of an empty placeholder (D24).
- **`--idempotency-key` is now forwarded upstream as a header** (not just a local retry gate) — the thing that actually makes a keyed retry non-double-billing; Exa's support for it is a tracked validation (D25).
- **the `curl|sh` installer is now a specified, checksum-verifying contract** (D26), and **`cargo deny`/`cargo audit` are in the gate** (D36).
- plus per-command blast-radius in `capabilities` (D27), success-path `nextActions`/`count`/`dataHash` (D28), a default output ceiling (D29), input forgiveness (D31), a linter-style doctor exit dictionary (D32), distinct auth-error codes + non-blocking keyring (D33), a crash-safe pending-run write (D35), and the `--body` deep-merge / doctor `--check` doc reconciliations (D38–D39).

## Source material

The v1 research corpus — primary-source Exa snapshots and the five subagent lane reports — stays in [`work/research/`](../../work/research/). `commands.md` carries [`lane-e-cli-taxonomy.md`](../../work/research/lane-e-cli-taxonomy.md) forward.
