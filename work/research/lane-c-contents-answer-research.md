# Lane C — Exa Contents / Answer / Research-style capabilities

Date: 2026-06-29
Scope: Exa APIs that retrieve page contents, crawl/scrape URLs or subpages, produce answer/research outputs, expose code/document context, and return citations/sources.
Sources: current Exa docs via Context7 and Exa primary docs pages. OpenAPI spec is the schema source of truth per Exa docs: https://exa.ai/docs/reference/openapi-spec

## Executive map

| Capability | Current endpoint / surface | Best CLI shape | Main output | Source |
|---|---|---|---|---|
| URL/page extraction, scrape-style retrieval, subpage crawl | `POST https://api.exa.ai/contents` | `exa contents ...` / `exa crawl ...` aliases over one endpoint | `requestId`, `results[]`, `statuses[]`, `costDollars` | https://exa.ai/docs/reference/get-contents, https://exa.ai/docs/reference/contents-api-guide-for-coding-agents |
| Direct cited answer | `POST https://api.exa.ai/answer` | `exa answer` | `answer`, `requestId`, `citations[]`, `costDollars` | https://exa.ai/docs/reference/answer |
| Long-running research/list-building/enrichment | `POST /agent/runs`, `GET /agent/runs/{id}`, events | `exa research create/get/poll/events` or `exa agent ...` | `agent_run` with `output.text`, `output.structured`, `output.grounding`, `usage`, `costDollars` | https://exa.ai/docs/reference/agent-api/overview, https://exa.ai/docs/reference/agent-api/create-a-run |
| OpenAI Responses-compatible research | `POST /responses`, OpenAI SDK `responses.create` against `https://api.exa.ai` | `exa responses ...` only if supporting compatibility/debug; otherwise hide behind `research` | async Agent API via model `exa-agent` | https://exa.ai/docs/reference/openai-sdk, https://exa.ai/docs/reference/openai-responses-api-with-exa |
| Code/document context for coding agents | `POST https://api.exa.ai/context` | `exa context` | `requestId`, `query`, `response`, `resultsCount`, `costDollars`, `searchTime`, `outputTokens` | https://exa.ai/docs/reference/context |
| Legacy Research API | `/research/v1` | Do not prioritize for new CLI surface; maybe `legacy research` if needed | deprecated/legacy, concurrent-task limited | https://exa.ai/docs/reference/rate-limits |

## Contents / getContents

### What it does

`POST /contents` gets full page contents, summaries, and metadata for URLs or search document IDs. It returns cached results quickly and live-crawls as a fallback for uncached pages. Source: https://exa.ai/docs/reference/get-contents

The coding-agent reference says Contents extracts clean LLM-ready content from URLs, including JavaScript-rendered pages, PDFs, and complex layouts, and can return full text, highlights, summaries, or combinations. Source: https://exa.ai/docs/reference/contents-api-guide-for-coding-agents

### Request shape and limits

- Auth: `x-api-key` header; `Authorization: Bearer` is also documented. Source: https://exa.ai/docs/reference/get-contents
- Body accepts either `ids` or `urls`, not both. `ids` are search document IDs; `urls` is backwards-compatible with `ids`. Source: https://exa.ai/docs/reference/get-contents
- `ids` / `urls` array length: 1-100; each string length 1-2048. Source: https://exa.ai/docs/reference/get-contents
- `subpages`: integer 0-100, default 0; actual crawl count can be system-limited. Source: https://exa.ai/docs/reference/get-contents
- `subpageTarget`: string or string array, each target string length 1-100; prioritizes specific linked pages. Source: https://exa.ai/docs/reference/get-contents
- `/contents` rate limit: 100 QPS by default. Source: https://exa.ai/docs/reference/rate-limits
- `livecrawlTimeout`: default 10000 ms, allowed range `0 < x <= 90000`. Source: https://exa.ai/docs/reference/get-contents
- `maxAgeHours`: allowed range -1 to 720; controls cache freshness. Source: https://exa.ai/docs/reference/get-contents

### Content extraction options

Top-level options on `/contents`:

- `text`: boolean or object. Object form includes `maxCharacters`, `includeHtmlTags`, and in the coding-agent reference also `verbosity`, `includeSections`, `excludeSections`. Source: https://exa.ai/docs/reference/contents-api-guide-for-coding-agents
- `highlights`: boolean or object. Highlights are key excerpts; object form supports `query` and `maxCharacters`. Source: https://exa.ai/docs/reference/contents-api-guide-for-coding-agents
- `summary`: boolean/object in best-practices docs; object form supports custom `query` and JSON `schema` for structured extraction. Source: https://exa.ai/docs/reference/contents-best-practices, https://exa.ai/docs/reference/contents-api-guide-for-coding-agents
- `extras.links` / `extras.imageLinks`: number of links/image links to extract. Source: https://exa.ai/docs/reference/contents-api-guide-for-coding-agents
- `context`: deprecated; use `highlights` or `text` instead. Source: https://exa.ai/docs/reference/get-contents

Agent consumption recommendation: default to `highlights: true` for repeated agent workflows because Exa says highlights are token-efficient and directly extracted; use `text.maxCharacters` when the agent needs deep review or when the relevant section is unknown. Source: https://exa.ai/docs/reference/contents-best-practices

### Response format

`/contents` returns:

- `requestId`: request identifier. Source: https://exa.ai/docs/reference/get-contents
- `results[]`: page result objects with fields such as `title`, `url`, `id`, `publishedDate`, `author`, `image`, `favicon`, `text`, `highlights`, `highlightScores`, `summary`, `subpages`, `extras`. Source: https://exa.ai/docs/reference/contents-api-guide-for-coding-agents
- `statuses[]`: per-URL/document status; always check it because individual URL failures can be returned inside an HTTP 200 response. Source: https://exa.ai/docs/reference/contents-api-guide-for-coding-agents
- `costDollars`: endpoint-dependent estimated dollar cost breakdown; Exa notes billing is computed from usage counters rather than this response object. Source: https://exa.ai/docs/reference/get-contents
- Deprecated `context`: combined context string; use `highlights` or `text` instead. Source: https://exa.ai/docs/reference/get-contents

Per-URL status errors include `CRAWL_NOT_FOUND`, `CRAWL_TIMEOUT`, `CRAWL_LIVECRAWL_TIMEOUT`, `SOURCE_NOT_AVAILABLE`, `UNSUPPORTED_URL`, and `CRAWL_UNKNOWN_ERROR` in the coding-agent reference/best-practices pages. Source: https://exa.ai/docs/reference/contents-api-guide-for-coding-agents, https://exa.ai/docs/reference/contents-best-practices

### Crawling / livecrawl / scrape-style behavior

Exa models crawling through `/contents` rather than a separate scrape endpoint: pass one or more starting URLs/IDs, choose content modes, and optionally set `subpages` plus `subpageTarget` for within-site crawl. Source: https://exa.ai/docs/reference/contents-api-guide-for-coding-agents

Recommended docs-crawl request pattern:

```json
{
  "urls": ["https://platform.openai.com/docs"],
  "subpages": 15,
  "subpageTarget": ["api", "models", "embeddings"],
  "maxAgeHours": 24,
  "livecrawlTimeout": 15000,
  "text": { "maxCharacters": 5000 }
}
```

Source: https://exa.ai/docs/reference/contents-best-practices

Best-practice crawl guidance:

- Start with `subpages` 5-10 and increase if needed. Source: https://exa.ai/docs/reference/contents-best-practices
- Use specific `subpageTarget` terms such as `api`, `reference`, `guide`, `about`, `pricing`; without it, selection is best-effort. Source: https://exa.ai/docs/reference/contents-api-guide-for-coding-agents
- Combine subpage crawl with `maxAgeHours` when freshness matters. Source: https://exa.ai/docs/reference/contents-best-practices

### Cache / freshness options

`maxAgeHours` is the current freshness control:

| `maxAgeHours` | Behavior | Source |
|---|---|---|
| omitted | default: livecrawl only if no cached content exists; recommended/balanced | https://exa.ai/docs/reference/livecrawling-contents |
| positive N | use cache if content is less than N hours old; otherwise livecrawl | https://exa.ai/docs/reference/livecrawling-contents |
| `0` | always livecrawl / ignore cache | https://exa.ai/docs/reference/livecrawling-contents |
| `-1` | never livecrawl / cache only | https://exa.ai/docs/reference/livecrawling-contents |

If you set `maxAgeHours`, Exa recommends pairing it with explicit `livecrawlTimeout` for reliability; best-practice range is 10000-15000 ms, although examples may use lower values for specific production cases. Source: https://exa.ai/docs/reference/contents-api-guide-for-coding-agents, https://exa.ai/docs/reference/livecrawling-contents

The old `livecrawl` string parameter is deprecated. Equivalents: `always` -> `maxAgeHours: 0`; `never` -> `maxAgeHours: -1`; `fallback` -> omit `maxAgeHours`; `preferred` has no direct equivalent, but Exa suggests a low positive value such as `1` for similar behavior. Source: https://exa.ai/docs/reference/livecrawling-contents

Important caveat: Exa says `livecrawl` does not guarantee freshly fetched parser output and should not be sent with `maxAgeHours`. Source: https://exa.ai/docs/reference/get-contents

### Agent CLI design implications

- Provide aliases, but one implementation: `contents get`, `contents`, `crawl`, and `scrape` should all map to `/contents`; `crawl` only means `subpages > 0` or site-level extraction. Source: inference from Exa's single `/contents` endpoint: https://exa.ai/docs/reference/get-contents
- Output should preserve Exa's `requestId`, `statuses`, and `costDollars`; do not return only `results`, because agents need failure and cost metadata. Source: https://exa.ai/docs/reference/contents-api-guide-for-coding-agents
- Treat HTTP 200 + status errors as partial failure in the CLI: exit 0 only if all requested URLs succeeded, or provide a `--allow-partial` flag that exits 0 with a machine-readable `partial: true`. Source basis: per-URL failures in `statuses`: https://exa.ai/docs/reference/contents-api-guide-for-coding-agents
- Default agent mode should prefer `highlights: true`, with `--full-text` / `--text-max-chars` for deep reading. Source: https://exa.ai/docs/reference/contents-best-practices
- Expose freshness explicitly: `--fresh` maps to `maxAgeHours=0`; `--cache-only` maps to `-1`; `--max-age-hours N`; `--livecrawl-timeout-ms N`. Source: https://exa.ai/docs/reference/livecrawling-contents
- For docs crawling, require or strongly suggest `--subpage-target` when `--subpages` is set. Source: https://exa.ai/docs/reference/contents-api-guide-for-coding-agents

## Answer API

### What it does

`POST /answer` performs Exa search, then uses an LLM to generate either a direct answer for specific questions or a cited summary for open-ended questions. Source: https://exa.ai/docs/reference/answer

The response includes the generated answer and the search results/citations used. Source: https://exa.ai/docs/reference/answer

### Request shape and limits

- Endpoint: `POST https://api.exa.ai/answer`. Source: https://exa.ai/docs/reference/answer
- Auth: `x-api-key` header; `Authorization: Bearer` is also documented. Source: https://exa.ai/docs/reference/answer
- Body: `query` string is required, minimum length 1. Source: https://exa.ai/docs/reference/answer
- `stream`: boolean, default false; true returns server-sent events. Source: https://exa.ai/docs/reference/answer
- `text`: boolean, default false; true returns full page text with default settings in citations. Source: https://exa.ai/docs/reference/answer
- `outputSchema`: JSON Schema Draft 7; when provided, `answer` is a structured object instead of a string. Source: https://exa.ai/docs/reference/answer
- `/answer` default rate limit: 10 QPS. Source: https://exa.ai/docs/reference/rate-limits

### Response format

- `answer`: string by default, or structured object matching `outputSchema`. Source: https://exa.ai/docs/reference/answer
- `requestId`: unique request identifier. Source: https://exa.ai/docs/reference/answer
- `citations[]`: search results used to generate the answer; example fields include `title`, `url`, `publishedDate`, `author`, `id`, `image`, `favicon`, and optionally `text` when requested. Source: https://exa.ai/docs/reference/answer
- `costDollars`: endpoint-dependent estimated dollar cost breakdown; billing is based on usage counters rather than this response object. Source: https://exa.ai/docs/reference/answer

### Agent CLI design implications

- `answer` is the low-latency, single-question surface; do not use Agent runs for simple factual Q&A unless the user asks for multi-hop/deep research. Source basis: `/answer` description vs Agent "when to use": https://exa.ai/docs/reference/answer, https://exa.ai/docs/reference/agent-api/overview
- Preserve `citations` by default and optionally add a `--sources markdown|json|none` formatter; never print an answer without source URLs unless user explicitly asks. Source: https://exa.ai/docs/reference/answer
- Add `--with-text` mapping to `text: true`, but keep default false to control payload/context size. Source: https://exa.ai/docs/reference/answer
- Add `--schema FILE|JSON` for `outputSchema`, and validate that the resulting `answer` may be object-valued. Source: https://exa.ai/docs/reference/answer
- For streaming, expose `--stream` as SSE-to-stdout text plus optional event JSON mode; document that streaming is supported by `/answer`, unlike `/contents`. Source: https://exa.ai/docs/reference/answer, https://exa.ai/docs/reference/contents-api-guide-for-coding-agents

## Research-style capabilities: Agent API and Responses API

### Current primary surface: Agent API

Exa's current research-style product is the Agent API. It runs asynchronous, multi-step web research, list-building, and enrichment workflows with natural-language answers, structured outputs, and citations. Source: https://exa.ai/docs/reference/agent-api/overview

Use Agent when a workflow needs more than a single search or extraction call: open-ended research, list building, structured extraction, entity enrichment, follow-up over previous results, or multi-hop tasks. Source: https://exa.ai/docs/reference/agent-api/overview, https://exa.ai/docs/reference/agent-api-guide

Core endpoints:

| Method | Path | Purpose | Source |
|---|---|---|---|
| POST | `/agent/runs` | create async run; JSON or SSE | https://exa.ai/docs/reference/agent-api/create-a-run |
| GET | `/agent/runs/{id}` | retrieve/poll a run | https://exa.ai/docs/reference/agent-api/get-a-run |
| GET | `/agent/runs` | list runs | https://exa.ai/docs/reference/agent-api/overview |
| POST | `/agent/runs/{id}/cancel` | cancel queued/running run | https://exa.ai/docs/reference/agent-api/overview |
| DELETE | `/agent/runs/{id}` | delete stored run | https://exa.ai/docs/reference/agent-api/overview |
| GET | `/agent/runs/{id}/events` | list or replay run events | https://exa.ai/docs/reference/agent-api/list-run-events |

Run lifecycle: `queued -> running -> completed | failed | cancelled`; terminal stop reasons include `schema_satisfied`, `budget_reached`, `error`, and `cancelled`. Source: https://exa.ai/docs/reference/agent-api/overview

Runs time out after one hour. Source: https://exa.ai/docs/reference/agent-api/overview

### Agent create-run request shape

`POST /agent/runs` accepts:

- `query` required natural-language instructions. Source: https://exa.ai/docs/reference/agent-api/create-a-run
- `systemPrompt` for source preferences, novelty constraints, duplicate avoidance, or behavior guidance. Source: https://exa.ai/docs/reference/agent-api/create-a-run
- `input.data` for rows/entities to enrich and `input.exclusion` for entities to avoid. Source: https://exa.ai/docs/reference/agent-api/create-a-run, https://exa.ai/docs/reference/agent-api-guide
- `outputSchema` for validated structured output; supports JSON Schema draft-07, 2019-09, and 2020-12 via `$schema`. Source: https://exa.ai/docs/reference/agent-api/create-a-run, https://exa.ai/docs/reference/agent-api/overview
- `effort`: `minimal`, `low`, `medium`, `high`, `xhigh`, or `auto`; default `auto`. Source: https://exa.ai/docs/reference/agent-api/create-a-run
- `previousRunId` to continue from a completed run; must belong to same team. Source: https://exa.ai/docs/reference/agent-api/create-a-run
- `metadata` stored with run. Source: https://exa.ai/docs/reference/agent-api/create-a-run
- `dataSources`, max length 5, for Exa Connect providers. Source: https://exa.ai/docs/reference/agent-api/create-a-run
- `Accept: text/event-stream` streams run events. Source: https://exa.ai/docs/reference/agent-api/create-a-run

### Agent response format

Agent run objects include:

- `id`, `object: "agent_run"`, `status`, `stopReason`, `createdAt`, `completedAt`. Source: https://exa.ai/docs/reference/agent-api/create-a-run
- `request`: canonicalized request fields. Source: https://exa.ai/docs/reference/agent-api/create-a-run
- `output.text`: natural-language answer or summary. Source: https://exa.ai/docs/reference/agent-api/overview
- `output.structured`: validated JSON when `outputSchema` was provided; otherwise null. Source: https://exa.ai/docs/reference/agent-api/overview
- `output.grounding`: citations for the text answer or structured fields when emitted. Source: https://exa.ai/docs/reference/agent-api/overview
- `usage`: agent compute units, searches, emails, phone numbers, dataSources. Source: https://exa.ai/docs/reference/agent-api/get-a-run
- `costDollars`: total and component cost breakdown. Source: https://exa.ai/docs/reference/agent-api/get-a-run

Grounding can be field-level; examples show `output.grounding[].field` pointing into structured output, with `citations[]` containing URLs/titles. Source: https://exa.ai/docs/reference/agent-api/get-a-run

### Agent limits, pricing, and effort

- Agent concurrency limit is one fifth of account QPS; with default pay-as-you-go QPS, docs say this means two active Agent runs. Source: https://exa.ai/docs/reference/agent-api/overview
- Legacy `/research/v1` default limit is 15 concurrent tasks; docs label it legacy/deprecated. Source: https://exa.ai/docs/reference/rate-limits
- Agent pricing components documented: 1 ACU = $0.10; search tool calls are $0.005/search; email contact enrichment is $0.02/email and phone enrichment is $0.07/phone number. Source: https://exa.ai/docs/reference/agent-api/overview
- Fixed effort request prices: `minimal` $0.012, `low` $0.025, `medium` $0.10, `high` $0.50, `xhigh` $1.00. Source: https://exa.ai/docs/reference/agent-api/overview
- Exa recommends `medium` as a default starting point for standard single-entity research; move down for cost/latency, up for difficult verification or deeper reasoning; use `auto` for variable-scope list-building or unknown entity counts. Source: https://exa.ai/docs/reference/agent-api/overview
- Runtime varies by query difficulty, schema complexity, and source availability; effort modes are tradeoffs, not strict latency guarantees. Source: https://exa.ai/docs/reference/agent-api/overview
- Exa Agent is not ZDR. Source: https://exa.ai/docs/reference/agent-api/overview

### Streaming and events

- Create runs can stream lifecycle events using `Accept: text/event-stream`. Source: https://exa.ai/docs/reference/agent-api/create-a-run
- Events use SSE framing and include terminal events such as `agent_run.completed`, `agent_run.failed`, and `agent_run.cancelled`. Source: https://exa.ai/docs/reference/agent-api/overview
- `GET /agent/runs/{id}/events` returns paginated JSON by default; set `Accept: text/event-stream` to replay stored events as SSE. Use `cursor` for JSON pagination and `Last-Event-ID` for SSE replay. Source: https://exa.ai/docs/reference/agent-api/list-run-events

### OpenAI Responses-compatible route

Exa documents OpenAI-compatible endpoints:

- `/chat/completions` routes to `/answer` with model `exa`. Source: https://exa.ai/docs/reference/openai-sdk
- `/responses` routes to Agent API with model `exa-agent`; runs are asynchronous, so set `background: true` and poll `GET /responses/{id}`. Source: https://exa.ai/docs/reference/openai-sdk

The OpenAI Responses API integration page also shows Exa as a tool inside OpenAI Responses, where a model calls an `exa_websearch` function and the app sends Exa search results back to OpenAI, with sources retained separately. Source: https://exa.ai/docs/reference/openai-responses-api-with-exa

Important version note: Context7 returned a snippet mentioning models `exa-research` and `exa-research-pro` for `/responses`, but the current Exa docs pages opened during this pass document `exa-agent`. Treat `exa-agent` as the current target unless the raw OpenAPI spec or account-specific beta docs say otherwise. Sources: https://exa.ai/docs/reference/openai-sdk, https://exa.ai/docs/reference/openapi-spec

### Agent CLI design implications

- Model as async by default: `research create`, `research get`, `research poll`, `research events`, `research cancel`, `research list`, `research delete`. Source: https://exa.ai/docs/reference/agent-api/overview
- `research run` can be a convenience macro: create + poll until terminal + print output; use polling interval defaults around the SDK example's 4000 ms unless user overrides. Source: https://exa.ai/docs/reference/agent-api-guide
- Preserve all output planes: `output.text`, `output.structured`, `output.grounding`, `usage`, `costDollars`, and run status. Source: https://exa.ai/docs/reference/agent-api/overview
- For structured output, require `--schema` and encourage `maxItems` on arrays to bound cost; Exa specifically recommends bounding arrays where possible. Source: https://exa.ai/docs/reference/agent-api/overview
- Add `--effort auto|minimal|low|medium|high|xhigh`, with `medium` as a good user-facing default for standard single-entity research and `auto` for variable-scope list building. Source: https://exa.ai/docs/reference/agent-api/overview
- Add `--system-prompt`, `--input-data`, `--exclude`, `--previous-run-id`, `--metadata`, and `--data-source` flags matching request fields. Source: https://exa.ai/docs/reference/agent-api/create-a-run
- Surface `stopReason` in both human and JSON output; `budget_reached` should not be silently treated as full success. Source: https://exa.ai/docs/reference/agent-api/overview
- For field-level citations, print grounding next to fields in human output and preserve full JSON in `--json`. Source: https://exa.ai/docs/reference/agent-api/get-a-run

## Context API / Exa Code

### What it does

`POST /context` (Exa Code) finds relevant code snippets and examples from open-source repositories, docs pages, Stack Overflow, and similar sources for coding agents. Source: https://exa.ai/docs/reference/context

The docs position it as a token-efficient way for coding agents to get real working examples for framework usage, API syntax, development setup, library implementation, and best practices. Source: https://exa.ai/docs/reference/context

### Request shape

- Endpoint: `POST https://api.exa.ai/context`. Source: https://exa.ai/docs/reference/context
- `query` required; min length 1, max 2000 characters. Source: https://exa.ai/docs/reference/context
- `tokensNum` optional, string or integer; default `dynamic`; supported exact range 50-100000. Source: https://exa.ai/docs/reference/context
- Exa says 5000 tokens is a good default for most queries and 10000 when 5k is insufficient. Source: https://exa.ai/docs/reference/context

### Response format

`/context` returns JSON with:

- `requestId`
- `query`
- `response`: formatted code snippets and contextual examples
- `resultsCount`
- `costDollars`
- `searchTime`
- `outputTokens`

Source: https://exa.ai/docs/reference/context

### Agent CLI design implications

- Provide `context` as a distinct command from `contents`: it is query-first code/docs context, not URL-first scraping. Source: https://exa.ai/docs/reference/context
- Default `tokensNum` to `dynamic`; add `--tokens` for deterministic budgets. Source: https://exa.ai/docs/reference/context
- Preserve `resultsCount`, `searchTime`, `outputTokens`, and `costDollars` in JSON output for budget-aware agents. Source: https://exa.ai/docs/reference/context

## Source/citation handling across APIs

| API | Citation/source fields | Agent consumption rule | Source |
|---|---|---|---|
| Contents | `results[].url`, `title`, `id`, `publishedDate`, `author`; `statuses[]`; `subpages[]`; `extras.links`; excerpts in `highlights` | Treat each result/subpage as source material; check `statuses` before trusting completeness | https://exa.ai/docs/reference/contents-api-guide-for-coding-agents |
| Answer | `citations[]` are search results used to generate answer; optional citation `text` when `text: true` | Always retain citations with answer; let users ask for full citation text | https://exa.ai/docs/reference/answer |
| Agent | `output.grounding[]` can cite text or specific structured fields | Keep grounding adjacent to structured fields and preserve raw grounding in JSON | https://exa.ai/docs/reference/agent-api/overview, https://exa.ai/docs/reference/agent-api/get-a-run |
| Context | `response` embeds formatted examples and URLs; metadata includes counts/cost/time | Use as context blob; do not treat as a final answer without agent verification | https://exa.ai/docs/reference/context |

## Recommended command taxonomy for exa-agent-cli

Ponytail version: reuse endpoint primitives; avoid separate implementations for scrape/crawl/livecrawl.

```text
exa contents get URL... [--id] [--text|--highlights|--summary] [--text-max-chars N]
                         [--include-html-tags] [--verbosity compact|standard|full]
                         [--include-section S ...] [--exclude-section S ...]
                         [--max-age-hours N|--fresh|--cache-only] [--livecrawl-timeout-ms N]
                         [--links N] [--image-links N] [--json]

exa crawl URL [--subpages N] [--subpage-target TERM ...] [contents flags...] [--json]
# Alias over contents; requires or implies subpages > 0.

exa answer QUERY [--with-text] [--schema FILE] [--stream] [--json]

exa research create QUERY [--schema FILE] [--effort auto|minimal|low|medium|high|xhigh]
                          [--system-prompt TEXT] [--input-data FILE]
                          [--exclude FILE] [--previous-run-id ID]
                          [--data-source PROVIDER] [--metadata JSON]
                          [--stream] [--json]
exa research get RUN_ID [--json]
exa research poll RUN_ID [--interval-ms 4000] [--timeout-ms ...] [--json]
exa research events RUN_ID [--cursor CURSOR|--sse] [--json]
exa research cancel RUN_ID [--json]
exa research list [--limit N] [--json]
exa research delete RUN_ID [--json]

exa context QUERY [--tokens dynamic|N] [--json]
```

## Output contract recommendations for agents

1. Stable JSON envelope for every command:

```json
{
  "ok": true,
  "endpoint": "/contents",
  "requestId": "...",
  "data": {},
  "warnings": [],
  "partial": false,
  "statuses": [],
  "usage": null,
  "costDollars": {},
  "sourceUrls": []
}
```

Source basis: all relevant Exa endpoints return request/run IDs and cost/status/source metadata in different shapes; a CLI should normalize without dropping raw endpoint fields. Sources: https://exa.ai/docs/reference/get-contents, https://exa.ai/docs/reference/answer, https://exa.ai/docs/reference/agent-api/get-a-run, https://exa.ai/docs/reference/context

2. Keep raw Exa response available under `raw` or via `--raw`. Agents often need newly added provider fields before the CLI knows them. Source basis: Exa OpenAPI page states raw specs are source of truth and generated pages can omit details. Source: https://exa.ai/docs/reference/openapi-spec

3. For `/contents`, never rely only on HTTP status. Check `statuses[]`; report partial failures explicitly. Source: https://exa.ai/docs/reference/contents-api-guide-for-coding-agents

4. Prefer `highlights` first for multi-step agents; escalate to `text` with `maxCharacters` for deep analysis. Source: https://exa.ai/docs/reference/contents-best-practices

5. For freshness, default to omitted `maxAgeHours` unless the user asks for fresh/cache-only/current data; expose `--fresh` and `--cache-only` shorthands. Source: https://exa.ai/docs/reference/livecrawling-contents

6. For Answer/Agent outputs, source URLs are not optional in human output. `answer` has `citations[]`; Agent has `output.grounding[]`; consumers should treat missing/empty citations as a warning when the task requires evidence. Source: https://exa.ai/docs/reference/answer, https://exa.ai/docs/reference/agent-api/overview

7. For Agent runs, terminal `budget_reached` should be marked partial/incomplete unless the user requested budget-bounded best effort. Source: https://exa.ai/docs/reference/agent-api/overview

## Known deprecated or mismatch-prone items

- `/contents` `context` is deprecated; use `highlights` or `text`. Source: https://exa.ai/docs/reference/get-contents
- `/contents` `livecrawl` is deprecated; use `maxAgeHours`. Source: https://exa.ai/docs/reference/livecrawling-contents
- Do not send `livecrawl` and `maxAgeHours` together. Source: https://exa.ai/docs/reference/get-contents
- On `/contents`, `text`, `highlights`, and `summary` are top-level; do not wrap in `contents`. That nesting belongs to `/search`. Source: https://exa.ai/docs/reference/contents-api-guide-for-coding-agents
- `/contents` does not support streaming. Source: https://exa.ai/docs/reference/contents-api-guide-for-coding-agents
- Deprecated highlights parameters called out by docs: `numSentences` and `highlightsPerUrl`; use `highlights: true` or current object form. Source: https://exa.ai/docs/reference/contents-api-guide-for-coding-agents
- `tokensNum` is a Context API parameter, not a Contents text limiter; use `text.maxCharacters` for `/contents`. Source: https://exa.ai/docs/reference/context, https://exa.ai/docs/reference/contents-api-guide-for-coding-agents
- Legacy `/research/v1` is deprecated per rate-limit page; new research/list-building work should target Agent API or `/responses` compatibility. Source: https://exa.ai/docs/reference/rate-limits, https://exa.ai/docs/reference/agent-api/overview

## Open questions / runtime validation needed

- Verify raw OpenAPI JSON/YAML before implementation for exact nested schemas, especially `text.verbosity`, section filters, `summary.schema`, and Agent `output.grounding` object shape. Exa says OpenAPI specs are source of truth. Source: https://exa.ai/docs/reference/openapi-spec
- Confirm account-specific Agent concurrency and QPS on Trey's Exa account; docs give defaults and say higher limits require Enterprise. Source: https://exa.ai/docs/reference/rate-limits, https://exa.ai/docs/reference/agent-api/overview
- Confirm current `/responses` model naming against raw spec/account: docs say `exa-agent`, while one Context7 snippet mentioned `exa-research`/`exa-research-pro`. Current web docs should win until contradicted by OpenAPI/account tests. Source: https://exa.ai/docs/reference/openai-sdk, https://exa.ai/docs/reference/openapi-spec
- Validate actual streaming event payloads for `/answer`, `/agent/runs`, and Agent event replay with live API calls before pinning CLI schemas. Sources: https://exa.ai/docs/reference/answer, https://exa.ai/docs/reference/agent-api/create-a-run, https://exa.ai/docs/reference/agent-api/list-run-events
