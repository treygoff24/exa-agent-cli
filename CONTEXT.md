# Context

## Domain purpose

Build an agent-first CLI over Exa that exposes the full public and documented Exa API surface without hiding capability behind a simplified wrapper.

## Glossary

Installed `capabilities --json` gives endpoint-to-command mappings. The
[command reference](docs/v2/commands.md) includes design targets; the
[decision record](docs/v2/decisions.md) preserves the rationale and later changes.

| Term | Meaning |
| --- | --- |
| Agent-first CLI | Commands designed for non-interactive callers, with offline discovery, parseable output, stable exit codes, and explicit confirmation for destructive actions |
| Canonical command | A stable command path for one Exa operation, such as `search` for `POST /search` |
| Macro | A transparent command expansion: `ask` calls `answer`; `fetch` calls `contents`. Inspect the request with `--dry-run --print-request` |
| Raw request | `raw METHOD PATH --body ...` calls an endpoint outside the typed surface with shared auth, retries, tracing, and output handling |
| Envelope | The CLI's JSON wrapper: `exa.cli.response.v1` for success and `exa.cli.error.v1` for errors. `--raw` emits HTTP content-decoded body bytes instead; signed-payment credential echoes are redacted |
| Operation registry | A build-time table generated from committed OpenAPI specs and `openapi/overlay.toml`, containing command paths, methods, fields, pagination, streaming, and safety metadata |
| Webset | An asynchronous structured collection at `/websets/v0/websets`, containing searches, items, enrichments, and related resources. Creation returns an ID for later status and result retrieval |
| Search monitor | `monitor` manages recurring searches at `/monitors` |
| Websets monitor | `websets monitors` manages scheduled Websets work at `/websets/v0/monitors`; it is a separate resource family from Search monitors |
| Admin/service key | `EXA_SERVICE_KEY`, resolved separately for the Team Management API at `https://admin-api.exa.ai/team-management`. It is not interchangeable with `EXA_API_KEY` |

The overlay also defines the retained `/context` compatibility command, which
is undocumented upstream. SDK-only beta Agent Monitors are not typed commands.
Raw payment discovery and signed-header pass-through support exact nonstreaming
`POST /search` and `POST /contents` on the default host. Wallet custody and signing
are outside the CLI. Historical research remains in
[Exa API research](docs/research/exa-api-research.md).
