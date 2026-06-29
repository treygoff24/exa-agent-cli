# Lane E — Agent-first CLI command taxonomy for Exa

Date: 2026-06-29
Target repo: `/Users/treygoff/Code/exa-agent-cli`
Mode: design only; no implementation.

## Source basis and confidence

Primary source priority used here:

1. Live official OpenAPI spec: `https://exa.ai/docs/exa-spec.yaml` — observed as `Exa Public API` version `2.0.0`; this is the highest-authority source for endpoint breadth, auth, request fields, pagination limits, response shapes, and lifecycle/deprecation hints.
2. Official Exa docs index snapshot already in the repo: `work/research/exa-docs-llms.txt`, which links official docs for Search, Contents, Context, Answer, Agent API, Search Monitors, Websets, Websets events/imports/monitors/webhooks/searches/enrichments/items, and coding-agent references.
3. Repo snapshots: `work/research/exa-openapi-spec.yaml` and `work/research/exa-websets-spec.yaml`. These are useful but older/narrower than the live spec in some places. Example drift: the local search spec is `Exa Search API` version `1.2.0` and includes `/research/v0/tasks`; the live spec includes `/agent/runs`, `/research/v1`, `/monitors`, `/v0/teams/me`, and the Websets API in the same spec.

Design implication: embed an official-spec-derived operation registry, but keep `exa raw` and `exa schema refresh` so future Exa endpoints are not blocked by the curated taxonomy.

## Design north star

The CLI should be **agent-first, not simplified-agent-only**:

- The first command an agent guesses should work: `exa search "query"`, `exa contents URL`, `exa answer "question"`, `exa agent run "task"`, `exa websets create --query ...`.
- Every official API capability should have a stable command path, and every power-user request option should be expressible as a named flag, `--body`, `--set`, or raw passthrough.
- Human-friendly aliases may exist, but canonical commands must map one-to-one to official Exa operations.
- Structured output must be deterministic by default for automation: stdout is data; stderr is diagnostics.
- No macro may hide the underlying API shape. Every macro must be inspectable with `--dry-run --print-request` and reversible into canonical commands.

## Proposed top-level command tree

Canonical binary name used below: `exa`. If the package name must be `exa-agent`, keep `exa` as the recommended installed alias.

```text
exa
├── search                         # POST /search
├── contents                       # POST /contents
├── similar                        # POST /findSimilar; alias: find-similar
├── answer                         # POST /answer
├── context                        # context/code-search docs surface if supported by current spec/docs
├── monitor                        # top-level recurring Search Monitors: /monitors...
│   ├── create                     # POST /monitors
│   ├── list                       # GET /monitors
│   ├── get                        # GET /monitors/{id}
│   ├── update                     # PATCH /monitors/{id}
│   ├── delete                     # DELETE /monitors/{id}
│   ├── trigger                    # POST /monitors/{id}/trigger
│   ├── runs list                  # GET /monitors/{id}/runs
│   ├── runs get                   # GET /monitors/{id}/runs/{runId}
│   └── batch                      # POST /monitors/batch
├── agent                          # Agent API: /agent/runs...
│   ├── run                        # POST /agent/runs; alias of runs create
│   └── runs
│       ├── create                 # POST /agent/runs
│       ├── list                   # GET /agent/runs
│       ├── get                    # GET /agent/runs/{id}
│       ├── events                 # GET /agent/runs/{id}/events
│       ├── cancel                 # POST /agent/runs/{id}/cancel
│       └── delete                 # DELETE /agent/runs/{id}
├── research                       # Research API compatibility/current v1 surface
│   ├── create                     # POST /research/v1
│   ├── list                       # GET /research/v1
│   └── get                        # GET /research/v1/{researchId}
├── websets                        # Websets API: /v0/websets...
│   ├── create                     # POST /v0/websets
│   ├── list                       # GET /v0/websets
│   ├── get                        # GET /v0/websets/{id}
│   ├── update                     # POST /v0/websets/{id}
│   ├── delete                     # DELETE /v0/websets/{id}
│   ├── cancel                     # POST /v0/websets/{id}/cancel
│   ├── preview                    # POST /v0/websets/preview
│   ├── items
│   │   ├── list                   # GET /v0/websets/{webset}/items
│   │   ├── get                    # GET /v0/websets/{webset}/items/{id}
│   │   └── delete                 # DELETE /v0/websets/{webset}/items/{id}
│   ├── searches
│   │   ├── create                 # POST /v0/websets/{webset}/searches
│   │   ├── get                    # GET /v0/websets/{webset}/searches/{id}
│   │   └── cancel                 # POST /v0/websets/{webset}/searches/{id}/cancel
│   ├── enrichments
│   │   ├── create                 # POST /v0/websets/{webset}/enrichments
│   │   ├── get                    # GET /v0/websets/{webset}/enrichments/{id}
│   │   ├── update                 # PATCH /v0/websets/{webset}/enrichments/{id}
│   │   ├── delete                 # DELETE /v0/websets/{webset}/enrichments/{id}
│   │   └── cancel                 # POST /v0/websets/{webset}/enrichments/{id}/cancel
│   ├── imports
│   │   ├── create                 # POST /v0/imports
│   │   ├── list                   # GET /v0/imports
│   │   ├── get                    # GET /v0/imports/{id}
│   │   ├── update                 # PATCH /v0/imports/{id}
│   │   └── delete                 # DELETE /v0/imports/{id}
│   ├── monitors                   # Websets monitors, not top-level Search Monitors
│   │   ├── create                 # POST /v0/monitors
│   │   ├── list                   # GET /v0/monitors
│   │   ├── get                    # GET /v0/monitors/{id}
│   │   ├── update                 # PATCH /v0/monitors/{id}
│   │   ├── delete                 # DELETE /v0/monitors/{id}
│   │   └── runs
│   │       ├── list               # GET /v0/monitors/{monitor}/runs
│   │       └── get                # GET /v0/monitors/{monitor}/runs/{id}
│   ├── events
│   │   ├── list                   # GET /v0/events
│   │   └── get                    # GET /v0/events/{id}
│   └── webhooks
│       ├── create                 # POST /v0/webhooks
│       ├── list                   # GET /v0/webhooks
│       ├── get                    # GET /v0/webhooks/{id}
│       ├── update                 # PATCH /v0/webhooks/{id}
│       ├── delete                 # DELETE /v0/webhooks/{id}
│       └── attempts list          # GET /v0/webhooks/{id}/attempts
├── team info                      # GET /v0/teams/me
├── capabilities                   # CLI self-description
├── schema                         # embedded API/CLI schema export and validation
│   ├── list
│   ├── show
│   ├── export
│   ├── validate-input
│   └── refresh                    # optional: fetch official spec and report diff
├── robot-docs
│   ├── guide
│   ├── commands
│   ├── errors
│   ├── examples
│   └── prompts
├── doctor                         # local/auth/connectivity/config/API-contract checks
├── config
│   ├── list|get|set|unset
│   ├── path
│   └── profiles list|show|create|use|delete
├── auth
│   ├── status
│   └── test
└── raw                            # exact API escape hatch: METHOD PATH [--body ...]
```

### Why this shape

- `search`, `contents`, `similar`, and `answer` are top-level because these are Exa’s core synchronous primitives in the official public API.
- `agent` is top-level because `/agent/runs` is an asynchronous workflow API with runs, events, cancel/delete, SSE, input rows, data sources, and structured outputs.
- `research` remains top-level because the live spec exposes `/research/v1` and the repo snapshot exposes `/research/v0/tasks`; treat this as a compatibility/current research surface until product direction is clarified. Do not bury it under `agent` unless Exa officially deprecates it.
- `monitor` vs `websets monitors` intentionally separates two similarly named API families: top-level Search Monitors under `/monitors`, and Websets monitors under `/v0/monitors`.
- `raw` is mandatory to avoid false completeness. If the official spec moves faster than this CLI, agents can still call the API safely with the same auth/output/error contracts.

## Operation-to-command mapping table

| Official operation | Canonical CLI | Notes |
|---|---|---|
| `POST /search` | `exa search QUERY` | Search modes from live spec: `instant`, `fast`, `auto`, `deep-lite`, `deep`, `deep-reasoning`. Include `additionalQueries`, `systemPrompt`, `outputSchema`, `stream`, `contents`, filters, moderation, compliance. |
| `POST /contents` | `exa contents URL...` | Also accept `--ids`; official request accepts either `urls` or `ids`. Prefer `urls` in help because IDs are only useful after search. |
| `POST /findSimilar` | `exa similar URL` | Live spec marks this deprecated and says prefer `/search` with a source-describing query. Keep command for full breadth, but emit a non-fatal deprecation warning on stderr and show `exa search --similar-to URL` as the recommended future-safe pattern. |
| `POST /answer` | `exa answer QUESTION` | Supports `--text`, `--output-schema`, `--stream`. |
| `POST /monitors` | `exa monitor create` | Top-level recurring search monitors. Creation returns `webhookSecret` once; doctor should warn if output is not stored when user asks. |
| `GET /monitors` | `exa monitor list` | Cursor pagination with `--cursor`, `--limit`, `--all`; filters include status/name/metadata. |
| `POST /monitors/batch` | `exa monitor batch` | Default dry-run mirrors official `dry_run: true`; require `--yes` for delete/pause/unpause mutation. |
| `GET/PATCH/DELETE /monitors/{id}` | `exa monitor get/update/delete` | Delete requires `--yes`; update supports `--set`, `--body`, and named flags. |
| `POST /monitors/{id}/trigger` | `exa monitor trigger ID` | Mutating but reversible-ish; no `--yes`, but support `--dry-run --print-request`. |
| `GET /monitors/{id}/runs[/runId]` | `exa monitor runs list|get` | Cursor pagination on list. |
| `POST /agent/runs` | `exa agent run QUERY` or `exa agent runs create QUERY` | Supports JSON response or SSE via `Accept: text/event-stream`; expose `--stream`, `--output-schema`, `--input-json`, `--data-source`, `--effort`, `--beta`, `--body`. |
| `GET /agent/runs` | `exa agent runs list` | Cursor pagination. |
| `GET /agent/runs/{id}` | `exa agent runs get ID` | Stable status read. |
| `GET /agent/runs/{id}/events` | `exa agent runs events ID` | JSON list by default; `--stream` for SSE replay; support `--last-event-id`. |
| `POST /agent/runs/{id}/cancel` | `exa agent runs cancel ID` | Safe cancellation; return existing terminal run if already complete. |
| `DELETE /agent/runs/{id}` | `exa agent runs delete ID --yes` | Destructive; require confirmation. |
| `GET/POST /research/v1` | `exa research list|create` | Current live spec surface. Use `--stream` where official API supports streaming. |
| `GET /research/v1/{researchId}` | `exa research get ID` | Include `--stream` compatibility. |
| `POST/GET /v0/websets` | `exa websets create|list` | Websets create accepts search/import/enrichment configs; support `--body`, named common flags, and presets. |
| `GET/POST/DELETE /v0/websets/{id}` | `exa websets get|update|delete` | Delete requires `--yes`; update is POST per official spec, not PATCH. |
| `POST /v0/websets/{id}/cancel` | `exa websets cancel ID` | Cancel all running operations for a Webset; require `--yes` because it can discard running work. |
| `POST /v0/websets/preview` | `exa websets preview` | Agent-first planning command before create; should be promoted in help. |
| `GET/DELETE /v0/websets/{webset}/items` | `exa websets items list|get|delete` | List supports cursor/limit/sourceId; delete requires `--yes`. |
| `POST/GET/cancel webset searches` | `exa websets searches create|get|cancel` | Create additional searches in an existing webset. |
| `POST/PATCH/GET/DELETE/cancel enrichments` | `exa websets enrichments create|update|get|delete|cancel` | Delete/cancel require `--yes` because enrichment results may become unavailable or cannot resume. |
| `POST/GET/PATCH/DELETE imports` | `exa websets imports create|list|get|update|delete` | Create returns upload URL; CLI should optionally upload local CSV as a follow-up convenience only if explicit. |
| `POST/GET/PATCH/DELETE /v0/monitors` | `exa websets monitors ...` | Websets monitor family; distinct from top-level `exa monitor`. |
| `GET /v0/events[/id]` | `exa websets events list|get` | Event list supports cursor, limit, types, createdBefore, createdAfter. |
| `POST/GET/PATCH/DELETE webhooks` | `exa websets webhooks ...` | Include `attempts list`; doctor should include signature verification help. |
| `GET /v0/teams/me` | `exa team info` | Read-only account/limits surface. |

## Flag taxonomy

### Universal flags

These must work on every command unless explicitly irrelevant. The same spelling everywhere is more valuable than short aliases.

```text
--json                       Emit CLI envelope JSON to stdout on success.
--ndjson                     Emit one envelope per line for streams/batches.
--format human|json|ndjson|raw
--raw-response               Emit exact upstream JSON/SSE bytes; no CLI envelope.
--pretty / --compact         Pretty-print JSON for humans vs compact for machines.
--quiet                      Suppress non-error diagnostics on stderr.
--verbose                    Emit request summary and retry diagnostics on stderr.
--trace FILE                 Write redacted request/response trace to FILE.
--no-color                   Disable ANSI; also honor NO_COLOR/CI/TERM=dumb/non-TTY.

--profile NAME               Select config profile.
--api-key KEY                Explicit key; discouraged in shell history; env/config preferred.
--base-url URL               Default https://api.exa.ai; useful for tests/proxies.
--header 'Name: value'       Extra header; repeatable. Redact known secret headers in traces.
--beta VALUE                 Sets Exa beta header when official API requires it.
--timeout DURATION           Whole request timeout, e.g. 30s.
--connect-timeout DURATION
--retry N                    Retry count for retryable network/429/5xx failures.
--retry-after                Honor Retry-After headers when present; default true.

--input FILE|-               Read request body, query list, URL list, or rows from file/stdin.
--input-format text|json|jsonl|csv|auto
--body JSON|@file|-          Exact request body object; bypasses named flag assembly.
--set path=value             Patch request body field; repeatable. Example: --set contents.text.maxCharacters=1000.
--print-request              Print redacted upstream request envelope to stdout and do not call API unless --execute is also passed.
--dry-run                    Alias for --print-request on read commands; preview on mutating commands.

--limit N                    Page size for list endpoints; endpoint-specific max enforced.
--cursor TOKEN               Cursor for next page.
--all                        Follow cursors until exhausted; forbidden for non-cursor search endpoints.
--max-pages N                Safety cap for --all.
--page-delay DURATION        Delay between paginated calls.

--stream                     Use streaming where official endpoint supports SSE.
--raw-sse                    With --stream, emit upstream SSE exactly.
--last-event-id ID           For Agent event replay.

--yes                        Required for irreversible deletes/cancels/batch actions.
--confirm TOKEN              Stronger confirmation for high-blast-radius batch deletes.
--idempotency-key KEY        Client-generated idempotency key when Exa supports/accepts one; otherwise stored only in CLI trace metadata.
```

### Search flags: `exa search`

Expose all live `SearchRequest` fields as flags. Do not hide synthesized-output fields behind a separate command.

```text
exa search QUERY
  --type instant|fast|auto|deep-lite|deep|deep-reasoning
  --num-results N
  --category 'company|research paper|news|personal site|financial report|people|CUSTOM'
  --include-domain DOMAIN     # repeatable; maps to includeDomains[]
  --exclude-domain DOMAIN     # repeatable; maps to excludeDomains[]
  --start-crawl-date ISO8601
  --end-crawl-date ISO8601
  --start-published-date ISO8601
  --end-published-date ISO8601
  --user-location CC
  --moderation / --no-moderation
  --compliance hipaa
  --additional-query QUERY     # repeatable; deep-search variants only
  --system-prompt TEXT|@file
  --output-schema JSON|@file
  --stream
  --contents JSON|@file
  --text[=true|false]
  --text-max-characters N
  --text-verbosity compact|standard|full
  --include-section header|navigation|banner|body|sidebar|footer|metadata
  --exclude-section header|navigation|banner|body|sidebar|footer|metadata
  --include-html-tags
  --highlights[=true|false]
  --highlight-query TEXT
  --highlight-max-characters N
  --summary-query TEXT
  --summary-schema JSON|@file
  --extras-links N
  --extras-image-links N
  --extras-rich-links N
  --extras-rich-image-links N
  --extras-code-blocks N
  --subpages N
  --subpage-target TEXT        # repeatable or comma-separated
  --max-age-hours N            # preferred freshness control
  --livecrawl never|always|fallback|preferred       # deprecated; accepted for breadth
  --livecrawl-timeout MS
  --context[=true|false]       # deprecated; accepted but warned
  --context-max-characters N   # deprecated
```

Guardrails:

- If `--category company` or `--category people` is combined with unsupported date/domain filters, fail locally with exit 1 before the API call and cite the exact offending flags. The live spec documents limited filter support for those categories.
- If `--livecrawl` and `--max-age-hours` are both set, fail locally; live spec says not to send both.
- `--all` is invalid on `search`; search uses `--num-results` with public max 100. Error should say: `Use --num-results N (1..100); --all is only for cursor-paginated list commands.`

### Contents flags: `exa contents`

```text
exa contents URL...
exa contents --input urls.txt
exa contents --ids ID...       # compatibility; urls preferred

# all ContentsOptions flags from search apply here:
--text, --text-max-characters, --include-html-tags, --text-verbosity,
--include-section, --exclude-section,
--highlights, --highlight-query, --highlight-max-characters,
--summary-query, --summary-schema,
--extras-links, --extras-image-links, --extras-rich-links, --extras-rich-image-links, --extras-code-blocks,
--subpages, --subpage-target,
--max-age-hours, --livecrawl, --livecrawl-timeout, --context
```

Local validation:

- Official spec allows up to 100 `urls` or `ids`; reject larger inputs unless `--chunk-size` is provided.
- `--chunk-size N` can split a large stdin URL list into multiple calls, emitting NDJSON envelopes per chunk.

### Similar flags: `exa similar`

```text
exa similar URL
  --exclude-source-domain
  --category ...
  --num-results N
  --include-domain DOMAIN
  --exclude-domain DOMAIN
  --start/end crawl/published date filters
  --contents/text/highlights/summary/extras/subpages/max-age-hours flags
```

Because the live spec marks `/findSimilar` deprecated, help should say:

```text
Deprecated upstream: prefer `exa search --similar-to URL "..."` once your query can describe the source. This command remains for full API coverage.
```

Do not remove it; deprecation is not deletion.

### Answer flags: `exa answer`

```text
exa answer QUESTION
  --text / --no-text
  --output-schema JSON|@file
  --stream
```

Agent contract: default `--json` returns `data.answer`, `data.citations`, `costDollars`; `--stream --ndjson` emits event envelopes.

### Agent run flags

```text
exa agent run QUERY
exa agent runs create QUERY
  --output-schema JSON|@file
  --input JSON|@file          # maps to request.input when using rows/exclusions
  --input-row JSON            # repeatable convenience
  --exclusion JSON|@file
  --data-source PROVIDER      # repeatable, e.g. similarweb
  --effort auto|...           # only enumerate if current official schema does
  --stream
  --beta VALUE
```

Read/event flags:

```text
exa agent runs list --limit N --cursor TOKEN --all
exa agent runs get ID
exa agent runs events ID --limit N --cursor TOKEN
exa agent runs events ID --stream --last-event-id ID
exa agent runs cancel ID
exa agent runs delete ID --yes
```

### Websets flags

For Websets, prefer body-first completeness plus named flags for the common path:

```text
exa websets create --query TEXT --count N
exa websets create --body @webset.json
exa websets preview --query TEXT --criteria TEXT --count N
exa websets searches create WEBSET --query TEXT --count N --criteria TEXT --scope JSON|@file
exa websets enrichments create WEBSET --description TEXT --format text|number|date|boolean|options --body @json
exa websets imports create --source csv --url URL
exa websets imports create --csv FILE       # convenience: create import then upload to returned uploadUrl only when explicit
exa websets monitors create --body @json
exa websets webhooks create --url URL --event EVENT --secret-output FILE
```

Power-user rule: every Websets create/update command must accept `--body @file` and `--set path=value`, because Websets schemas are rich and will evolve.

## JSON output envelope

Default human output can be terse, but `--json` must always use a stable envelope. Do not print upstream JSON bare unless `--raw-response` is requested.

Success envelope:

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
  "costDollars": {
    "total": 0.007
  },
  "warnings": [],
  "diagnostics": {
    "durationMs": 1234,
    "retries": 0,
    "cache": null
  }
}
```

Error envelope, written to stderr for `--json`/`--ndjson` failures, with empty stdout:

```json
{
  "schema": "exa.cli.error.v1",
  "ok": false,
  "error": {
    "code": "invalid_flag_combination",
    "category": "usage",
    "message": "--all is not valid for exa search because /search is not cursor-paginated.",
    "details": {
      "flag": "--all",
      "command": "exa search"
    },
    "retryable": false,
    "suggestedCommand": "exa search \"latest AI chips\" --num-results 100 --json"
  },
  "operation": {
    "method": "POST",
    "path": "/search"
  },
  "request": {
    "requestId": "req_local_...",
    "upstreamRequestId": null,
    "redacted": true
  }
}
```

Streaming contract:

- `--stream --raw-sse`: exact upstream SSE bytes to stdout; diagnostics to stderr.
- `--stream --json`: final accumulated JSON envelope if upstream stream reaches terminal status.
- `--stream --ndjson`: one envelope per event. Example schemas: `exa.cli.event.v1`, `exa.cli.response.v1`, `exa.cli.error.v1`.
- On interrupted streams, exit 12 and write an error envelope that includes the last observed event id when available.

Batch contract:

- For `contents --input urls.txt --chunk-size 100 --ndjson`, emit one success/error envelope per chunk.
- A mixed batch exits 10 after emitting per-item/per-chunk NDJSON envelopes. Agents can still parse partial success.

## Capabilities, schema, robot-docs, and doctor surfaces

### `exa capabilities --json`

Must be callable without network. It describes the CLI contract, not account state.

Minimum fields:

```json
{
  "schema": "exa.cli.capabilities.v1",
  "tool": "exa",
  "version": "0.0.0",
  "api": {
    "specUrl": "https://exa.ai/docs/exa-spec.yaml",
    "specTitle": "Exa Public API",
    "specVersion": "2.0.0",
    "embeddedSpecSha256": "..."
  },
  "commands": [
    {
      "path": ["search"],
      "operationId": "search",
      "method": "POST",
      "apiPath": "/search",
      "readOnly": true,
      "streaming": true,
      "pagination": "none",
      "supportsRawBody": true,
      "supportsPrintRequest": true,
      "dangerous": false
    }
  ],
  "universalFlags": [],
  "env": [],
  "configPrecedence": [],
  "exitCodes": {},
  "schemas": {
    "responseEnvelope": "exa.cli.response.v1",
    "errorEnvelope": "exa.cli.error.v1"
  }
}
```

### `exa schema`

Surfaces:

```text
exa schema list --json
exa schema show SearchRequest --json
exa schema show exa.cli.response.v1 --json
exa schema export --api openapi --output exa-spec.yaml
exa schema export --cli jsonschema --output cli-schemas.json
exa schema validate-input search --body @request.json
exa schema refresh --check               # report live-spec drift; no file writes unless --output is provided
```

Agent-first behavior:

- `schema show` works offline from the embedded spec.
- `schema refresh --check` compares embedded spec metadata/hash to live `https://exa.ai/docs/exa-spec.yaml`; it reports added/removed/changed operations but does not auto-update code.
- If a command lacks first-class flags for a new field, `schema show` plus `raw` and `--body` still unblock the agent.

### `exa robot-docs`

`robot-docs guide` should be a paste-ready, compact playbook. It should include:

- first-try commands;
- command tree;
- stdout/stderr contract;
- JSON envelope schemas;
- pagination rules;
- error/exit code table;
- dangerous operation confirmation rules;
- exact examples for common tasks;
- how to inspect raw requests with `--dry-run --print-request`;
- how to fall back to `exa raw`.

Suggested subcommands:

```text
exa robot-docs guide --format markdown|json
exa robot-docs commands --json
exa robot-docs examples --task search|answer|agent-run|websets|monitor
exa robot-docs errors --json
exa robot-docs prompts --json             # copy-paste prompts for coding agents using this CLI
```

### `exa doctor`

Doctor should be safe by default. It can run offline checks without API calls, and network checks only when asked.

```text
exa doctor
exa doctor --json
exa doctor --network
exa doctor --check auth,config,spec,connectivity,webhooks,permissions,limits
```

Checks:

- config parse and precedence;
- profile selection;
- API key presence without printing secret value;
- base URL validity;
- embedded spec hash/version;
- installed binary version;
- `NO_COLOR`/non-TTY discipline self-check;
- optional network auth test;
- optional `team info` to report concurrency/limits if official endpoint returns them;
- webhook signature guidance for Websets webhooks and Search Monitor webhooks;
- warnings for deprecated flags/surfaces (`similar`, `livecrawl`, `context`, deprecated highlight knobs).

## Error and exit-code contract

Exit codes are CLI categories, not raw HTTP codes. Upstream HTTP details belong in the JSON error envelope.

| Exit | Category | Meaning | Examples |
|---:|---|---|---|
| 0 | success | Command completed; empty result sets are success. | Search returns `results: []`; list returns `data: []`. |
| 1 | usage | Invalid command, flag, JSON body, schema, or local validation. | `--all` on `search`; both `--urls` and `--ids`; unsupported filter for `category=people`. |
| 2 | auth | Missing/invalid API key or team context. | Upstream 401. |
| 3 | config | Config file/profile/env problem. | Malformed TOML; unknown profile. |
| 4 | network | DNS/connect/TLS/timeout before upstream response. | Offline, timeout. |
| 5 | upstream | Exa 5xx or malformed upstream response. | Server error. |
| 6 | rate_limit | Rate/concurrency limit. | HTTP 429; Agent run concurrency limit reached. |
| 7 | not_found | Requested resource does not exist. | Run/webset/item/monitor not found. |
| 8 | conflict | Resource conflict or idempotency conflict. | Webset externalId already exists. |
| 9 | safety | Dangerous operation refused because confirmation missing. | Delete without `--yes`; batch delete without confirm. |
| 10 | partial | Batch had mixed success/failure. | Some content chunks failed. |
| 11 | no_input | Required stdin/input was empty. | `exa contents --input -` with empty stdin. |
| 12 | interrupted | User/system interruption or stream broken after partial events. | SIGINT during `--stream`. |

Error message requirements:

- one-line summary first;
- exact failing command/flag/path;
- upstream HTTP status and request id when present;
- retryability boolean;
- exact suggested command;
- JSON error envelope on stderr when `--json` or `--ndjson` is active;
- no secrets in any error, trace, or suggested command.

## Config and environment conventions

Precedence, highest to lowest:

1. explicit CLI flags;
2. environment variables;
3. selected profile in project config;
4. selected profile in user config;
5. global defaults;
6. built-in defaults.

Config files:

```text
./.exa-agent-cli.toml                 # project-local, optional
$XDG_CONFIG_HOME/exa-agent-cli/config.toml
~/.config/exa-agent-cli/config.toml   # fallback when XDG_CONFIG_HOME unset
```

Do not read arbitrary `.env` files by default. If supported, require `--env-file FILE` and document precedence.

Recommended env vars:

```text
EXA_API_KEY                 API key; never printed except fingerprint last4/hash.
EXA_PROFILE                 Select profile.
EXA_BASE_URL                Override API base URL; default https://api.exa.ai.
EXA_TIMEOUT                 Request timeout, e.g. 30s.
EXA_CONNECT_TIMEOUT         Connect timeout.
EXA_RETRY                   Retry count.
EXA_OUTPUT                  human|json|ndjson|raw default.
EXA_NO_COLOR                CLI-specific color disable; also honor NO_COLOR.
EXA_TRACE                   Trace file path.
EXA_BETA                    Default Exa beta header value, when needed.
NO_COLOR, CI, TERM=dumb     Standard no-color/non-interactive conventions.
SOURCE_DATE_EPOCH           If timestamps are generated by CLI, make them deterministic.
```

Config shape:

```toml
default_profile = "default"
output = "human"
base_url = "https://api.exa.ai"
timeout = "30s"
retry = 2

[profiles.default]
api_key_env = "EXA_API_KEY"
base_url = "https://api.exa.ai"

[profiles.prod]
api_key_env = "EXA_PROD_API_KEY"

[presets.official_latest]
command = "search"
type = "fast"
include_domains = ["*.gov", "*.edu"]
contents = { text = { maxCharacters = 2000 }, summary = { query = "What changed?" } }
```

Config commands:

```text
exa config path --json
exa config list --json --effective
exa config get profiles.default.base_url
exa config set profiles.prod.api_key_env EXA_PROD_API_KEY
exa config unset presets.old_news
exa config profiles list --json
exa config profiles use prod
```

## Stdin/stdout behavior

Rules:

- stdout is only command data: human table/markdown, JSON envelope, NDJSON event stream, raw upstream bytes.
- stderr is diagnostics: progress, warnings, deprecations, retry notices, suggested commands, doctor explanations.
- Non-TTY defaults should be parseable. If stdout is not a TTY and no format is specified, prefer JSON for read commands or at minimum suppress decoration. Better: keep human default stable but strongly document `--json`; no ANSI ever in non-TTY.
- Never prompt on stdin unless the command is explicitly interactive. For confirmations in non-TTY, require `--yes`/`--confirm`.

Input conventions:

```text
exa search "query"
echo "query" | exa search --input -
exa contents https://a.com https://b.com --json
cat urls.txt | exa contents --input - --input-format text --ndjson --chunk-size 100
cat requests.ndjson | exa search --input - --input-format jsonl --ndjson
exa agent run --input @rows.json "For each row, find..."
exa websets create --body @webset.json --json
```

For commands that accept both positional args and stdin, positional args win unless `--input -` is explicit. If both are provided accidentally, fail with an actionable error rather than guessing.

## Pagination strategy

Use one uniform CLI pagination model over endpoint-specific cursor fields.

- Cursor-list commands expose `--limit`, `--cursor`, `--all`, `--max-pages`, `--page-delay`.
- JSON envelope always includes `pagination.nextCursor`, `hasMore`, `autoPaginated`, `page`, `pageCount` when applicable.
- `--all --json` returns one envelope with accumulated `data` unless `--ndjson` is set.
- `--all --ndjson` emits one envelope per page; lower memory, better for agents.
- Search is not cursor-paginated; expose only `--num-results` and reject `--all`.
- Contents is not cursor-paginated; batch large URL lists client-side with `--chunk-size`.
- Agent events can paginate JSON with cursor or stream/replay SSE with `--last-event-id`; keep these separate.

Cursor-capable families from official spec:

```text
agent runs list, agent runs events
monitor list, monitor runs list
research list
websets list, websets items list
websets events list
websets webhooks list, websets webhook attempts list
websets imports list
websets monitors list, websets monitor runs list
```

## Presets and macros

Presets are saved request defaults; macros are named command expansions. Both must be transparent.

Mandatory transparency:

```text
exa preset show official-latest --json
exa search --preset official-latest "query" --dry-run --print-request
exa macro show cite-answer --json
```

Suggested built-in presets:

| Preset | Expands to | Use |
|---|---|---|
| `fast-json` | `search --type fast --json --text-max-characters 2000` | User-facing or agent-loop search. |
| `instant` | `search --type instant --num-results 10 --highlights` | Lowest latency lookup. |
| `deep-official` | `search --type deep --system-prompt "Prefer official sources..." --text --output-schema ...` | Higher confidence research. |
| `papers` | `search --category "research paper" --text --summary-query ...` | Academic paper discovery. |
| `news-fresh` | `search --category news --type fast --max-age-hours 0` | Fresh news retrieval. |
| `company` | `search --category company` | Company profile discovery; local validator blocks unsupported filters. |
| `people` | `search --category people` | People/profile discovery; local validator blocks unsupported filters. |
| `contents-clean` | `contents --text --text-verbosity compact --exclude-section navigation --exclude-section footer` | Clean page extraction. |

Suggested macros:

```text
exa ask QUESTION
  expands_to: exa answer QUESTION --text --json

exa fetch URL...
  expands_to: exa contents URL... --text --summary-query "Summarize the page" --json

exa cite QUERY
  expands_to: exa search QUERY --type fast --contents '{"text":{"maxCharacters":2000},"summary":{"query":"Why is this source relevant?"}}' --json

exa investigate QUERY
  expands_to: exa search QUERY --type deep --additional-query ... --output-schema @research.schema.json --json

exa watch create NAME --query QUERY --webhook URL
  expands_to: exa monitor create --name NAME --query QUERY --webhook-url URL ...
```

Do not add high-level product verbs that cannot be explained as a short expansion over canonical commands.

## How to avoid over-abstraction

Ponytail rule for this CLI: keep the command taxonomy boring and spec-shaped; spend cleverness on output contracts and error recovery, not domain-specific verbs.

Practical constraints:

1. Canonical commands mirror official operations. Aliases/macros may exist, but docs always show the canonical path and `expands_to`.
2. Every endpoint has an escape hatch: `--body`, `--set`, and `exa raw`.
3. Every deprecated upstream surface remains accessible until Exa removes it, but help marks it deprecated and suggests the official replacement.
4. Do not create separate commands for every content option. Use flags under `search`/`contents` because `ContentsOptions` is shared.
5. Do not invent pagination for non-paginated endpoints. Reject `--all` where Exa has no cursor.
6. Do not hide Websets complexity behind a single `exa leads` or `exa dataset` verb. Provide small macros for common examples, but leave `websets` as the canonical family.
7. Do not collapse top-level Search Monitors and Websets monitors. Same noun, different API families.
8. Do not auto-write files on behalf of the user except when `--output FILE`, `--trace FILE`, or explicit upload/download commands are used.
9. Do not silently make network calls in `capabilities`, `schema show`, or default `doctor`; require `--network` for live checks.
10. Do not turn `--json` into upstream JSON passthrough. The envelope is the CLI contract; `--raw-response` is the passthrough.

## First-try examples

```bash
# Search, with content extraction
exa search "latest AI chip launches" --type fast --text --summary-query "What changed?" --json

# Deep synthesized search with schema
exa search "Who is the current CEO of OpenAI?" \
  --type deep \
  --additional-query "OpenAI leadership official source" \
  --system-prompt "Prefer official sources and avoid duplicate results" \
  --output-schema @leader.schema.json \
  --json

# Contents for known URLs
exa contents https://exa.ai/docs/reference/search --text --summary-query "CLI-relevant API fields" --json

# Answer with citations
exa answer "What is Exa's maximum public numResults limit?" --text --json

# Agent run, then events
exa agent run "Find five recently launched developer tools for evaluating AI agents" --json
exa agent runs events agent_run_... --stream --ndjson

# Websets preview before create
exa websets preview --query "Tech companies in San Francisco" --count 10 --json
exa websets create --query "Tech companies in San Francisco" --count 10 --json

# Safe destructive flow
exa websets delete webset_123
# stderr: Refusing to delete without --yes. Preview/read first: exa websets get webset_123 --json
exa websets delete webset_123 --yes --json

# Raw escape hatch for newly added Exa endpoint
exa raw POST /new/endpoint --body @request.json --json
```

## Implementation notes for future lanes

- Generate the operation registry from the official OpenAPI spec and commit the generated registry plus the spec hash. Do not hand-maintain endpoint breadth.
- Hand-write aliases/macros and ergonomic help; generate only the low-level operation map and schema references.
- Build the universal envelope first, then bind commands. Otherwise every command will drift.
- Golden-test: `capabilities --json`, `schema list --json`, `robot-docs guide`, one success envelope, one error envelope, one paginated list, one streaming NDJSON path, and one raw passthrough.
- Local validation should catch known Exa constraints before API calls: `numResults` bounds, category filter incompatibilities, `livecrawl` plus `maxAgeHours`, contents `urls` vs `ids`, destructive commands without confirmation, and `--all` on non-cursor endpoints.

## Primary sources

- Exa live OpenAPI spec: `https://exa.ai/docs/exa-spec.yaml`.
- Exa Search reference: `https://exa.ai/docs/reference/search`.
- Exa Answer reference: `https://exa.ai/docs/reference/answer`.
- Exa Contents API guide/reference: `https://exa.ai/docs/reference/contents-api-guide` and `https://exa.ai/docs/reference/contents-api-guide-for-coding-agents`.
- Exa Agent API overview/reference: `https://exa.ai/docs/reference/agent-api/overview` and `https://exa.ai/docs/reference/agent-api/connect/overview`.
- Exa Websets API guide/reference: `https://exa.ai/docs/websets/api-guide` and `https://exa.ai/docs/websets/api-guide-for-coding-agents`.
- Repo source snapshots used for drift comparison: `work/research/exa-docs-llms.txt`, `work/research/exa-openapi-spec.yaml`, `work/research/exa-websets-spec.yaml`.
