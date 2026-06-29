# Agent-first CLI architecture

Date: 2026-06-29

## Design stance

Build `exa` as an agent-first full-surface CLI, not as a small wrapper around Search. The correct shape is:

1. **Canonical wrappers** for every official/documented operation.
2. **Raw passthrough** for anything newly added or not yet modeled.
3. **Transparent macros/presets** for common agent workflows.
4. **Offline self-description** through `capabilities --json`, `schema`, and `robot-docs`.
5. **Safe mutation defaults** for deletes, cancels, batch actions, key admin, and webhooks.

## Proposed command tree

```text
exa
├── search                         # POST /search
├── contents                       # POST /contents
├── similar                        # POST /findSimilar; deprecated upstream compatibility
├── answer                         # POST /answer
├── context                        # POST /context, Exa Code
├── monitor                        # top-level Search Monitors, /monitors
│   ├── create|list|get|update|delete|trigger|batch
│   └── runs list|get
├── agent                          # Agent API, /agent/runs
│   ├── run                        # macro alias for runs create, optionally wait
│   └── runs create|list|get|events|cancel|delete
├── research                       # compatibility research endpoints
│   └── create|list|get
├── websets
│   ├── create|list|get|update|delete|cancel|preview
│   ├── items list|get|delete
│   ├── searches create|get|cancel
│   ├── enrichments create|get|update|delete|cancel
│   ├── imports create|list|get|update|delete
│   ├── monitors create|list|get|update|delete|runs list|get
│   ├── webhooks create|list|get|update|delete|attempts list
│   ├── events list|get
│   └── exports schedule|get       # docs-confirmed; runtime/spec validation needed
├── team info                      # /v0/teams/me
├── admin keys                     # Team Management API, separate service-key auth
│   └── create|list|get|update|delete|usage
├── openai                         # compatibility/raw OpenAI-compatible surfaces
│   └── chat-completions|responses
├── capabilities --json
├── schema list|show|export|validate-input|refresh
├── robot-docs guide|commands|examples|errors|prompts
├── doctor
├── auth status|test|store|logout
├── config list|get|set|unset|path|profiles ...
└── raw METHOD PATH --body ...
```

Binary name: use `exa` if package conflicts allow; otherwise package as `exa-agent-cli` and install `exa` as an alias/shim.

## Universal flags

Every command should share these where applicable:

```text
--json | --ndjson | --format human|json|ndjson|raw
--raw-response
--pretty | --compact
--quiet | --verbose | --trace FILE
--no-color
--profile NAME
--api-key KEY
--base-url URL
--header 'Name: value'
--timeout DURATION
--connect-timeout DURATION
--retry N
--input FILE|-
--input-format text|json|jsonl|csv|auto
--body JSON|@file|-
--set path=value
--print-request
--dry-run
--limit N
--cursor TOKEN
--all
--max-pages N
--page-delay DURATION
--stream
--raw-sse
--last-event-id ID
--yes
--confirm TOKEN
```

## JSON envelope

Never make agents parse mixed prose. `--json` success stdout should use a CLI-owned envelope:

```json
{
  "schema": "exa.cli.response.v1",
  "ok": true,
  "command": "search",
  "operation": {
    "method": "POST",
    "path": "/search",
    "operationId": "search",
    "source": "https://exa.ai/docs/exa-spec.yaml",
    "sourceVersion": "2.0.0"
  },
  "request": {
    "requestId": "req_local_...",
    "upstreamRequestId": "...",
    "profile": "default",
    "redacted": true
  },
  "data": {},
  "pagination": {
    "cursor": null,
    "nextCursor": null,
    "hasMore": false,
    "autoPaginated": false,
    "page": 1,
    "pageCount": 1
  },
  "costDollars": {"total": 0.0},
  "warnings": [],
  "diagnostics": {"durationMs": 0, "retries": 0}
}
```

Failures under `--json` should write an error envelope to stderr and leave stdout empty:

```json
{
  "schema": "exa.cli.error.v1",
  "ok": false,
  "error": {
    "code": "invalid_flag_combination",
    "category": "usage",
    "message": "--all is not valid for exa search because /search is not cursor-paginated.",
    "retryable": false,
    "suggestedCommand": "exa search \"latest AI chips\" --num-results 100 --json"
  },
  "request": {"requestId": "req_local_...", "upstreamRequestId": null, "redacted": true}
}
```

## Exit codes

| Exit | Category | Meaning |
|---:|---|---|
| 0 | success | Completed; empty result sets are success. |
| 1 | usage | Invalid command, flags, JSON body, schema, or local validation. |
| 2 | auth | Missing/invalid API key or team context. |
| 3 | config | Config/profile/env problem. |
| 4 | network | DNS/connect/TLS/timeout before upstream response. |
| 5 | upstream | Exa 5xx or malformed upstream response. |
| 6 | rate_limit | HTTP 429 or concurrency limit. |
| 7 | not_found | Resource not found. |
| 8 | conflict | Resource/idempotency conflict. |
| 9 | safety | Dangerous operation refused. |
| 10 | partial | Mixed success/failure in batch. |
| 11 | no_input | Required stdin/input empty. |
| 12 | interrupted | SIGINT or broken stream after partial output. |

## Auth and config

Credential precedence:

1. `--api-key` for one-shot use; never persist unless explicit.
2. `EXA_API_KEY` for normal API access.
3. OS keyring profile.
4. Config metadata only.

Store normal API keys and service/admin keys separately. Proposed credential scopes:

- `exa:api:<profile>` for normal API.
- `exa:service:<profile>` for Team Management.

Recommended env vars:

```text
EXA_API_KEY
EXA_SERVICE_KEY
EXA_PROFILE
EXA_BASE_URL
EXA_ADMIN_BASE_URL
EXA_TIMEOUT
EXA_RETRY
EXA_OUTPUT
EXA_NO_COLOR
NO_COLOR / CI / TERM=dumb
```

Config should live at project `.exa-agent-cli.toml` and user `$XDG_CONFIG_HOME/exa-agent-cli/config.toml`; config stores profile metadata, env var names, defaults, and presets, not plaintext keys by default.

## Capabilities/schema/robot docs

Required self-description surfaces:

- `exa capabilities --json`: offline command/operation registry, flags, env, exit codes, schemas, safety, streaming, pagination, deprecations.
- `exa schema list/show/export/validate-input/refresh`: offline embedded schemas plus live drift check.
- `exa robot-docs guide`: paste-ready playbook for agents with first-try commands, errors, examples, pagination, and raw fallback.
- `exa doctor`: offline checks by default; `--network` opt-in for auth/connectivity/quota tests.

## Safety rules

- Deletes require `--yes`.
- High-blast-radius batch deletes require `--confirm <token>`.
- Top-level monitor batch must default to dry-run, matching docs.
- Webset cancel/enrichment cancel require confirmation because work may not resume.
- Admin key delete requires confirmation by key ID.
- Webhook secrets and API keys must be redacted in all logs/envelopes/traces.

## Presets and macros

Built-in presets are allowed only if transparent:

```text
exa preset show news-fresh --json
exa search --preset news-fresh "topic" --dry-run --print-request
```

Suggested macros:

- `exa ask QUESTION` -> `exa answer QUESTION --text --json`.
- `exa fetch URL...` -> `exa contents URL... --text --summary-query ... --json`.
- `exa research-run QUERY` -> `exa agent runs create` + poll + output.
- `exa websets quick-create` -> preview + create + wait + list items, but only with explicit `--execute` after preview.

Macros must return `expands_to` in JSON so agents can learn canonical commands.

## Implementation architecture recommendation

Favor a generated-operation-registry architecture:

- Embed official OpenAPI/docs-derived operation metadata.
- Hand-write ergonomic command aliases and macros over that registry.
- Build a single request builder that supports named flags, `--body`, and `--set`.
- Build one transport layer for auth, retry, redaction, trace, rate-limit classification, SSE, and JSON envelopes.
- Build one safety layer for destructive operations.
- Keep raw passthrough in v1 so spec drift never blocks agents.

This points toward TypeScript/Node for fast SDK alignment and packaging, or Rust for single-binary distribution. The first architecture question should settle that trade-off.
