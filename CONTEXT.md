# Context

## Domain purpose

Build an agent-first CLI over Exa that exposes the full public and documented Exa API surface without hiding capability behind a simplified wrapper.

## Glossary

- **Agent-first CLI**: A command-line interface where AI agents are the primary user. It must be discoverable, deterministic, parseable, safe under non-interactive use, and helpful when an agent makes a predictable mistake.
- **Canonical command**: A stable command path that maps directly to an official Exa operation, e.g. `exa search` -> `POST /search`.
- **Macro**: A convenience command that expands into one or more canonical commands. Macros must be inspectable with `--dry-run --print-request` and must not hide the underlying API shape.
- **Raw passthrough**: A command such as `exa raw METHOD PATH --body ...` that lets agents call newly added or not-yet-modeled Exa endpoints while retaining auth, retry, tracing, output, and error contracts.
- **Envelope**: The CLI-owned JSON wrapper around upstream responses. It normalizes `ok`, command, operation, request IDs, data, pagination, cost, warnings, diagnostics, and errors while preserving raw upstream output when requested.
- **Operation registry**: Embedded map of official Exa operations, schemas, flags, defaults, pagination, streaming support, deprecations, and safety metadata used by `capabilities`, `schema`, validation, and help.
- **Search API**: Exa query-first retrieval, primarily `POST /search`, with result ranking, filters, contents extraction, synthesis, structured output, and streaming where supported.
- **Contents API**: Exa URL/document-ID-first extraction, `POST /contents`, with text, highlights, summaries, freshness/livecrawl controls, subpages, extras, and per-URL statuses.
- **Answer API**: Exa cited answer generation, `POST /answer`, with citations and optional structured output/streaming.
- **Context API / Exa Code**: Query-first code/docs context endpoint, `POST /context`, returning formatted code examples and metadata for coding agents.
- **Agent API**: Exa asynchronous research/list-building/enrichment workflow API under `/agent/runs`, with run lifecycle, events/SSE, output text, structured output, grounding, usage, and cost.
- **Research API compatibility**: Older/current research endpoints exposed in Exa specs such as `/research/v1` and older `/research/v0/tasks`. Treat as compatibility until product direction is clarified.
- **Standalone Monitor**: Recurring search monitor under top-level `/monitors`, distinct from Websets monitors.
- **Webset**: Exa asynchronous structured collection/list-building system under Websets APIs. Websets contain searches, items, criteria/evaluations, enrichments, imports, monitors, events, webhooks, and exports.
- **Websets Monitor**: Scheduled behavior over a Webset under Websets `/v0/monitors`; distinct from top-level Search Monitors.
- **Admin/service key**: Higher-privilege key for Team Management API at `https://admin-api.exa.ai/team-management`. Store and expose separately from ordinary Exa API keys.
- **Team Management API**: Admin API for creating, listing, getting, updating, deleting, and reading usage for API keys.
- **x402**: Pay-per-request payment flow documented by Exa. It is independent from API-key billing; API-key auth takes precedence when both are present.

## Open decisions

- Whether mutating admin/API-key operations should ship in v1 or behind an explicit experimental/admin gate.
- Whether the implementation should be TypeScript-first, Rust-first, or hybrid generated-from-OpenAPI.
- Whether local response caching should exist at all; default planning assumption is no persistent cache unless explicitly requested.
