# Context

## Domain purpose

Build an agent-first CLI over Exa that exposes the full public and documented Exa API surface without hiding capability behind a simplified wrapper.

## Glossary

Terms whose meaning is not obvious from the command surface. Endpoint-to-command mappings live in `capabilities --json` and `docs/v2/commands.md`; the locked design decisions and their rationale live in `docs/v2/decisions.md`.

- **Agent-first CLI**: A command-line interface where AI agents are the primary user. It must be discoverable, deterministic, parseable, safe under non-interactive use, and helpful when an agent makes a predictable mistake.
- **Canonical command**: A stable command path mapping directly to one official Exa operation, e.g. `exa-agent search` → `POST /search`.
- **Macro**: A thin, transparent expansion of a canonical command (`ask` → `answer`, `fetch` → `contents`). Macros must be inspectable with `--dry-run --print-request` and must never hide the underlying API shape.
- **Raw passthrough**: `exa-agent raw METHOD PATH --body ...` — reaches endpoints the registry does not model (`/chat/completions`, `/responses`) while keeping auth, retry, tracing, output, and error contracts.
- **Envelope**: The CLI-owned JSON wrapper around upstream responses (`exa.cli.response.v1` / `exa.cli.error.v1`). It normalizes ok, command, operation, request ids, data, pagination, cost, warnings, diagnostics, and errors; `--raw` bypasses it and emits exact upstream bytes.
- **Operation registry**: The build-time table merged from the vendored OpenAPI specs plus `openapi/overlay.toml`, carrying each operation's command path, HTTP method, schema, pagination style, safety, and idempotency metadata. It backs `capabilities`, `schema`, validation, and help. The overlay may fully define real-but-unspecced operations, not just annotate spec-derived ones (e.g. `/context`).
- **Webset**: Exa's asynchronous structured collection under `/v0/websets`, containing searches, items, criteria/evaluations, enrichments, imports, monitors, events, webhooks, and exports. Creation is async: it returns an id you poll or stream events from.
- **Standalone Monitor** vs **Websets Monitor**: `exa-agent monitor …` drives top-level `/monitors` (a recurring search); `exa-agent websets monitors …` drives Websets `/v0/monitors` (scheduled behavior over an existing Webset). Different resources — the CLI gives the confusable pair reciprocal did-you-mean rather than aliasing them.
- **Admin/service key**: `EXA_SERVICE_KEY`, used only for the Team Management API at `https://admin-api.exa.ai/team-management`. Never interchangeable with `EXA_API_KEY`; stored and resolved separately.

**Not implemented:** `x402`, Exa's pay-per-request payment flow, is documented upstream but has no CLI surface. Original notes are in `docs/research/exa-api-research.md`.
