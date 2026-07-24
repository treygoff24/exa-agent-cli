# Implementation plan

Date: 2026-06-29

## Phase 0 — Decisions before coding

Ask/answer these before implementation:

1. Runtime/language: TypeScript, Rust, or hybrid generated CLI.
2. Scope gate: include admin key management in v1 or behind experimental/admin namespace.
3. Auth storage: keyring-only vs env-first + optional keyring + plaintext opt-in.
4. Output default: human by default with universal `--json`, or JSON by default in non-TTY.
5. Spec source: which Exa spec URL is canonical for generated registry, and how often to refresh.

## Phase 1 — Skeleton and contracts

Deliverables:

- CLI project scaffolding.
- Command parser with top-level tree stubs.
- `capabilities --json` with static command registry.
- JSON success/error envelopes.
- Exit code dictionary.
- `robot-docs guide` initial version.
- `doctor --json` offline checks.
- Unit tests for stdout/stderr separation, no-color, exit codes, and envelope schemas.

Acceptance:

- `exa capabilities --json | jq` works offline.
- Unknown flags produce exact suggested commands.
- `exa search --help` and `exa robot-docs guide` are agent-usable without web docs.

## Phase 2 — Core synchronous APIs

Implement:

- `search` with full filters, contents nesting, output schema, stream modes, request printing.
- `contents` with URL/ID input, chunking, statuses handling.
- `answer` with citations/text/schema/stream.
- `context`.
- `similar` deprecated compatibility.
- `raw` for arbitrary API calls.

Acceptance:

- Contract tests compare request bodies for every flag.
- Live smoke tests optional behind `EXA_API_KEY`.
- `/contents` partial statuses produce clear partial/success behavior.

## Phase 3 — Async APIs

Implement:

- `agent runs create/list/get/events/cancel/delete` plus `agent run` macro.
- `research` compatibility endpoints.
- Streaming/SSE normalization and raw SSE.
- Local pending-run records for ambiguous create failures.

Acceptance:

- Agents can create, poll, stream/replay events, and retrieve structured output/grounding/cost.
- Interrupting streams preserves last event metadata when available.

## Phase 4 — Monitors and Websets

Implement:

- Top-level `monitor` family.
- Websets core, items, searches, enrichments, imports, monitors, events, webhooks.
- Exports if runtime validation confirms exact endpoints.
- Webhook signature guidance in robot-docs/doctor.

Acceptance:

- All destructive operations are confirmation-gated.
- Cursor pagination works uniformly.
- Websets `externalId` conflict maps to exit 8 with helpful recovery commands.

## Phase 5 — Admin/team management

Implement behind explicit admin namespace/config:

- `admin keys create/list/get/update/delete/usage`.
- Separate service key credential storage.
- Usage date validation and 180-day lookback guard.

Acceptance:

- Normal `EXA_API_KEY` is never used accidentally as admin key unless explicitly configured.
- Deletes require confirmation by key ID.
- Created secrets, if ever returned by API, are shown/stored once and redacted everywhere else.

## Phase 6 — Polish and agent ergonomics pass

Run an agent-ergonomics audit against the built CLI:

- First-try command simulations.
- Intent-mistake tests (`--all` on search, top-level `text` on search, nested contents on `/contents`, bad category/filter combinations).
- Deterministic JSON snapshots.
- Robot-docs completeness tests.
- Live API smoke suite with budget/rate controls.

## Open runtime validations

- Exact canonical spec drift behavior.
- Retry-After headers.
- Team Management key-create secret behavior.
- Admin `rateLimit` semantics.
- OpenAI compatibility model names.

Resolved 2026-07-23: the canonical public spec is
`https://exa.ai/docs/exa-spec.json`; undocumented Websets export endpoints are
not implemented; retired `/research/v1` remains a deprecated compatibility
overlay with migration guidance to `search --type deep-reasoning`.
