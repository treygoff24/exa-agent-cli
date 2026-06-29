# Exa API research synthesis

Date: 2026-06-29

## Research method

I used current primary Exa sources and split research into five lanes: complete API surface, search/retrieval controls, contents/answer/agent/context, auth/limits/errors/security, and agent-first CLI taxonomy. Local source snapshots are in `work/research/`.

Primary sources used:

- Exa docs index: https://exa.ai/docs/llms.txt
- Official public OpenAPI/spec docs: https://exa.ai/docs/exa-spec.yaml and https://exa.ai/docs/api-reference/openapi.json
- Official OpenAPI repo snapshots: https://github.com/exa-labs/openapi-spec
- Search guide: https://exa.ai/docs/reference/search-api-guide-for-coding-agents
- Contents guide: https://exa.ai/docs/reference/contents-api-guide-for-coding-agents
- Answer reference: https://exa.ai/docs/reference/answer
- Agent API: https://exa.ai/docs/reference/agent-api/overview and https://exa.ai/docs/reference/agent-api-guide
- Context / Exa Code: https://exa.ai/docs/reference/context
- Websets coding-agent guide: https://exa.ai/docs/websets/api-guide-for-coding-agents
- Monitors guide: https://exa.ai/docs/reference/monitors-api-guide-for-coding-agents
- Team Management API: https://exa.ai/docs/team-management-spec.yaml
- Error codes: https://exa.ai/docs/reference/error-codes
- Rate limits: https://exa.ai/docs/reference/rate-limits
- Billing/pricing/security: https://exa.ai/docs/reference/billing, https://exa.ai/pricing, https://exa.ai/docs/reference/security
- OpenAI compatibility: https://exa.ai/docs/reference/openai-sdk and https://exa.ai/docs/reference/openai-responses-api-with-exa
- Exa MCP and x402: https://exa.ai/docs/reference/exa-mcp and https://exa.ai/docs/reference/x402-guide

## Full API surface for CLI coverage

### Core synchronous APIs

| Family | Endpoint(s) | CLI implication |
|---|---|---|
| Search | `POST /search` | Top-level `exa search`. Expose all retrieval, filter, content, synthesis, schema, and streaming fields. |
| Contents | `POST /contents` | Top-level `exa contents`. URL/ID-first extraction. Surface per-URL `statuses` because HTTP 200 can contain item failures. |
| Answer | `POST /answer` | Top-level `exa answer`. Preserve `answer`, citations, citation text when requested, cost, and stream events. |
| Similar | `POST /findSimilar` | Keep `exa similar` for full API breadth, but mark deprecated if current spec/docs do. Provide migration hints to search. |
| Context / Exa Code | `POST /context` | Top-level `exa context` for coding agents. Returns a formatted context blob, result count, search time, output tokens, and cost. |

### Async/research APIs

| Family | Endpoint(s) | CLI implication |
|---|---|---|
| Agent API | `POST/GET /agent/runs`, `GET/DELETE/POST cancel /agent/runs/{id}`, `GET /agent/runs/{id}/events` | Top-level `exa agent runs ...`; support create/run, poll, get, list, cancel, delete, events JSON pagination, and SSE replay. |
| Research compatibility | `/research/v1` in current docs, `/research/v0/tasks` in the GitHub Search spec snapshot | Expose under `exa research` as compatibility, but prefer Agent/deep search for new workflows unless runtime validation proves otherwise. |
| OpenAI compatibility | `/chat/completions`, `/responses` | Expose as compatibility/raw surfaces rather than default path. Current docs map chat completions to Answer and Responses to Agent. |

### Monitors

There are two monitor families that should not be collapsed:

- **Standalone Search Monitors** under top-level `/monitors`: create/list/get/update/delete/trigger/runs and batch actions. Batch actions support dry-run by default in docs.
- **Websets Monitors** under Websets `/v0/monitors`: scheduled behavior over Websets, with monitor runs.

CLI should use `exa monitor ...` for top-level Search Monitors and `exa websets monitors ...` for Websets monitors.

### Websets

Websets are a full async product surface, not a minor search option. Coverage should include:

- Websets: create, list, get, update, delete, cancel, preview.
- Items: list, get, delete.
- Searches: create, get, cancel.
- Enrichments: create, get, update, delete, cancel.
- Imports: create, list, get, update, delete.
- Monitors: create, list, get, update, delete, runs list/get.
- Webhooks: create, list, get, update, delete, attempts list.
- Events: list/get.
- Team info: `/v0/teams/me`.
- Exports: docs mention schedule/get export even though the downloaded Websets OpenAPI snapshot did not list the endpoints. Treat exports as docs-confirmed/spec-drift and runtime-validate before implementation.

Key Websets constraints/semantics from docs:

- Websets are asynchronous and items can appear while a Webset is still running.
- Criteria verify whether results match; enrichments extract additional data.
- `externalId` can be used for idempotency and may return 409 on conflicts.
- Webhook secrets are shown once on creation.
- Enrichment delete/cancel can make generated results unavailable and cancellation cannot be resumed.
- Export events include `webset.export.created` and `webset.export.completed`.

### Team/API-key administration

Team Management API is a separate admin surface at `https://admin-api.exa.ai/team-management` and uses a service key. Confirmed endpoints:

- `POST /api-keys` create key with optional `name`, `rateLimit`, `budgetCents`.
- `GET /api-keys` list key metadata.
- `GET /api-keys/{id}` get key metadata.
- `PUT /api-keys/{id}` update key metadata/rate/budget.
- `DELETE /api-keys/{id}` permanently delete key.
- `GET /api-keys/{id}/usage` retrieve authoritative billing usage for a key, default last 30 days, lookback limited to 180 days.

CLI implication: expose under a gated `exa admin keys ...` or `exa keys ... --admin` namespace. Do not mix service keys with normal query keys.

## Search and retrieval details that matter for CLI design

Current search `type` values in the coding-agent docs are `auto`, `fast`, `instant`, `deep-lite`, `deep`, and `deep-reasoning`. Older `neural`/`keyword` language appears in older docs/examples; treat it as compatibility only.

Default agent search should likely be:

```text
exa search "query" --type auto --num-results 10 --highlights --json
```

Recommended retrieval controls:

- Domain filters: `includeDomains`, `excludeDomains`; docs mention domain/path and wildcard support.
- Date filters: published and crawl dates.
- Category filters: `company`, `people`, `research paper`, `news`, `personal site`, `financial report`.
- Text filters: `includeText` and `excludeText`; docs warn only one phrase is currently supported.
- Location and moderation: `userLocation`, `moderation`.
- Compliance: enterprise `compliance: "hipaa"` where supported.
- Query controls: `systemPrompt`, `additionalQueries`; do not expose deprecated `useAutoprompt` as a normal flag.
- Contents under Search must be nested under `contents`; Contents API uses top-level `text`, `highlights`, `summary`.

Freshness/livecrawl planning:

| CLI flag | Exa field | Meaning |
|---|---|---|
| omit | omit `maxAgeHours` | Default: use cached content if available, livecrawl fallback. |
| `--fresh` | `maxAgeHours: 0` | Always livecrawl; slower/costlier. |
| `--cache-only` | `maxAgeHours: -1` | Never livecrawl; fastest but incomplete if uncached. |
| `--max-age-hours N` | positive integer | Use cache if younger than N hours, otherwise livecrawl. |

## Contents and citation handling

Contents supports text, highlights, summary, subpages, and extras. For agents, highlights are the best default for repeated multi-step work; full text should be opt-in and character-capped.

Important: `/contents` can return HTTP 200 while individual URLs fail. CLI envelopes must preserve `statuses[]` and support policy flags such as `--fail-on-url-error` later.

Citation/source handling by API:

- Search: result URLs/titles and optional `output.grounding` for synthesized/schema outputs.
- Contents: URL/title/id/author/publishedDate plus per-URL status; subpages are nested source material.
- Answer: `citations[]`, optional full citation text when requested.
- Agent: `output.grounding[]`, possibly field-level citations for structured output.
- Context: formatted response contains examples/URLs plus metadata; treat as context, not a final factual answer.

## Auth, limits, pricing, errors

Auth:

- Normal APIs use `x-api-key`; docs also mention Bearer in some OpenAPI security sections.
- Standard environment variable is `EXA_API_KEY`.
- Team Management uses a service key on a different host; store separately.

Documented default limits:

- `/search`: 10 QPS.
- `/contents`: 100 QPS.
- `/answer`: 10 QPS.
- Legacy `/research/v1`: 15 concurrent tasks.
- Agent concurrency: one fifth of account QPS; docs say default pay-as-you-go implies two active Agent runs.

Error handling:

- General documented error shape includes `requestId`, `error`, and `tag`, but 429 examples may only include `{ "error": "..." }`.
- `/contents` item failures are in `statuses[]` despite HTTP 200.
- Retry only clear transient classes automatically: 429, 5xx, network timeouts, and selected crawl timeouts. Do not auto-retry ambiguous async create requests without a local pending-run/resume mechanism, because retries may create billable duplicates.

Pricing/cost:

- Exa returns `costDollars` on many responses; the CLI should preserve and surface it.
- Agent cost is effort/usage/component based. Effort values: `minimal`, `low`, `medium`, `high`, `xhigh`, `auto`.
- Team key usage endpoint is the authoritative billing view for a specific key.

## Spec/docs drift and runtime validation list

Known drift or uncertainty:

1. Exa has multiple spec surfaces: docs OpenAPI JSON/YAML, GitHub `exa-openapi-spec.yaml`, GitHub `exa-websets-spec.yaml`, and docs-only pages. They do not perfectly match.
2. Websets exports are in docs/index and coding-agent guide but not in the downloaded Websets spec snapshot.
3. Research appears as `/research/v1` in current docs and `/research/v0/tasks` in the GitHub Search spec snapshot.
4. OpenAI `/responses` model naming should be live-tested; some snippets mention different names.
5. Team Management `rateLimit` wording should be runtime-verified if possible, though the spec says requests per second.
6. It is not documented whether Exa returns `Retry-After` on 429.
7. It is not documented whether create API key returns a raw secret once; docs examples show metadata only.

Implementation should therefore embed a spec snapshot but include `schema refresh --check`, `raw`, `--body`, and `--set` escape hatches.
