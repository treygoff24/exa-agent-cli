# Lane B: Exa search modes and retrieval controls for CLI design

Date: 2026-06-29. Sources are current primary Exa docs/API schema fetched during this pass.

## Executive takeaways for `exa-agent-cli`

1. Default the CLI to `POST /search` with `type=auto`, `numResults=10`, and `contents.highlights=true` for agent workflows. Exa says `/search` is the primary semantic retrieval surface for new integrations, and highlights are the token-efficient default for agents. Sources: https://exa.ai/docs/reference/search, https://exa.ai/docs/reference/search-best-practices, https://exa.ai/docs/reference/search-api-guide-for-coding-agents
2. Expose search type as a latency/quality preset, not as old `neural`/`keyword` language. Current API search types are `auto`, `fast`, `instant`, `deep-lite`, `deep`, and `deep-reasoning`; Exa says older `neural` mentions are legacy for new code. Sources: https://exa.ai/docs/reference/search, https://exa.ai/docs/reference/search-best-practices
3. Do not add `useAutoprompt` to new CLI examples. Exa docs list `useAutoprompt` in new requests as deprecated/removed; the current OpenAPI `SearchRequest` schema has no `useAutoprompt` field. Sources: https://exa.ai/docs/reference/search-api-guide-for-coding-agents, https://exa.ai/docs/exa-spec.yaml
4. Put all Search content extraction under `contents`. Do not allow top-level `text`, `highlights`, or `summary` on `/search`; `/contents` is different and uses top-level `text`, `highlights`, and `summary`. Source: https://exa.ai/docs/reference/search-api-guide-for-coding-agents, https://exa.ai/docs/reference/contents-api-guide-for-coding-agents
5. Make freshness explicit with `maxAgeHours`, not old `livecrawl`. Exa marks `livecrawl` deprecated and recommends `maxAgeHours`: omit = livecrawl fallback, `0` = always livecrawl, `-1` = cache only. Sources: https://exa.ai/docs/reference/get-contents, https://exa.ai/docs/reference/livecrawling-contents, https://exa.ai/docs/exa-spec.yaml

## Endpoint map

### `POST https://api.exa.ai/search`

Use when starting from a query and wanting Exa to find pages. It returns ranked search results and can attach content extraction per result via nested `contents`. It also supports synthesized output via `systemPrompt` and `outputSchema`, and SSE streaming via `stream: true`. Sources: https://exa.ai/docs/reference/search, https://exa.ai/docs/reference/search-api-guide-for-coding-agents

Current OpenAPI request fields include: `query`, `type`, `numResults`, `category`, `includeDomains`, `excludeDomains`, crawl and published date filters, `moderation`, `contents`, `additionalQueries`, `userLocation`, `compliance`, `systemPrompt`, `outputSchema`, and `stream`. Source: https://exa.ai/docs/exa-spec.yaml

### `POST https://api.exa.ai/contents`

Use when URLs or Exa document IDs are already known. It extracts text, highlights, summaries, metadata, extras, and subpages from up to 100 `ids` or `urls`. It returns 200 even when individual URLs fail, so the CLI must surface and optionally fail on `statuses` failures. Sources: https://exa.ai/docs/reference/get-contents, https://exa.ai/docs/reference/contents-api-guide-for-coding-agents, https://exa.ai/docs/exa-spec.yaml

### Not this lane, but relevant

Exa also has Research/Agent/Websets/Monitors surfaces. They are not the core search-retrieval primitive for this CLI lane, except that Research and Agent endpoints may stream/paginate their own run events. Sources: https://exa.ai/docs/reference/agent-api, https://exa.ai/docs/reference/research

## Search type choices

Current Search API `type` values:

| Type | CLI preset name | Use when | Tradeoff |
| --- | --- | --- | --- |
| `auto` | `balanced` / default | General web retrieval | Exa default balance of speed and quality. Source: https://exa.ai/docs/reference/search |
| `fast` | `fast` | Agent loops, command-line grounding, low-latency products | Lower latency with broad parameter compatibility. Sources: https://exa.ai/docs/reference/search, https://exa.ai/docs/reference/search-best-practices |
| `instant` | `instant` | autocomplete, voice/chat live suggestions, first-pass probes | Lowest-latency path; Exa changelog says it is built for real-time and sub-150ms/sub-200ms style use cases in current docs. Sources: https://exa.ai/docs/changelog, https://exa.ai/docs/reference/search-best-practices |
| `deep-lite` | `deep-lite` | Lightweight synthesized output | More reasoning/synthesis than `auto`, lower latency than deeper modes. Source: https://exa.ai/docs/reference/search |
| `deep` | `deep` | Multi-step research with structured output | Higher latency; useful for complex queries and JSON output. Source: https://exa.ai/docs/reference/search |
| `deep-reasoning` | `deep-reasoning` | Hardest research tasks | Highest reasoning depth and latency. Source: https://exa.ai/docs/reference/search |

Legacy note: older docs and SDK examples mention `neural` and `keyword`. Exa's current coding-agent docs say older `neural` references should be treated as legacy for new integrations, and current OpenAPI `SearchRequest.type` enum does not include `neural` or `keyword`. If preserving compatibility, make them hidden/legacy aliases that warn and normalize only if the API still accepts them at runtime. Sources: https://exa.ai/docs/reference/search-api-guide-for-coding-agents, https://exa.ai/docs/exa-spec.yaml

Recommended CLI behavior:

```text
exa search "query"                         # type=auto, highlights=true
exa search "query" --fast                  # type=fast
exa search "query" --instant               # type=instant
exa search "query" --deep                  # type=deep
exa search "query" --deep-reasoning        # type=deep-reasoning
exa search "query" --type auto|fast|instant|deep-lite|deep|deep-reasoning
```

Avoid separate `--neural` / `--keyword` in the default help. If implemented, mark as `deprecated` in `capabilities --json`.

## Query rewriting / autoprompt

Current guidance is: do not expose `useAutoprompt` as a new first-class flag. Exa docs explicitly list `useAutoprompt` in new requests as deprecated, and the OpenAPI schema omits it. Sources: https://exa.ai/docs/reference/search-api-guide-for-coding-agents, https://exa.ai/docs/exa-spec.yaml

There are still query-planning controls:

- `systemPrompt`: instructions for synthesized output and, for deep-search variants, search planning. Source: https://exa.ai/docs/reference/search
- `additionalQueries`: extra query variations for deep-search variants, used alongside the main query. Sources: https://exa.ai/docs/reference/search, https://exa.ai/docs/exa-spec.yaml
- Prompt engineering and long semantic queries remain valid: Exa supports semantically rich natural-language queries and long queries. Sources: https://exa.ai/docs/reference/exas-capabilities-explained, https://exa.ai/docs/reference/search-best-practices

CLI design: use explicit names, not autoprompt language.

```text
--system-prompt TEXT          # maps to systemPrompt
--additional-query TEXT       # repeatable; maps to additionalQueries[]
--rewrite                     # if added later, local CLI-side rewrite only; show rewritten query in JSON metadata
```

## Filters and restrictions

### Domain include/exclude

`includeDomains` and `excludeDomains` restrict or exclude domains. Current docs also say domain filters support path-specific filters such as `exa.ai/blog` and wildcard patterns such as `*.substack.com`; Search API reference says max 1200 domains. Sources: https://exa.ai/docs/changelog, https://exa.ai/docs/reference/search

CLI mapping:

```text
--include-domain openai.com --include-domain exa.ai/blog
--exclude-domain wikipedia.org --exclude-domain "*.substack.com"
```

### Date filters

`startPublishedDate` / `endPublishedDate` filter by estimated publication date. OpenAPI also exposes `startCrawlDate` / `endCrawlDate`; the main Search reference emphasizes published dates, while category caveats mention crawl dates for restricted categories. Sources: https://exa.ai/docs/reference/search, https://exa.ai/docs/exa-spec.yaml, https://exa.ai/docs/reference/search-api-guide-for-coding-agents

CLI mapping:

```text
--published-after 2026-01-01
--published-before 2026-06-29
--crawled-after 2026-06-01
--crawled-before 2026-06-29
```

Use ISO 8601 strings. The CLI should accept date-only input and serialize it consistently.

### Category filters

Current documented Search categories: `company`, `people`, `research paper`, `news`, `personal site`, `financial report`. Sources: https://exa.ai/docs/reference/search, https://exa.ai/docs/exa-spec.yaml

Important caveats:

- Do not invent categories such as `github`, `documentation`, `qa`, or `pdf` in new core Search examples; Exa's coding-agent docs explicitly warn against these. Source: https://exa.ai/docs/reference/search-api-guide-for-coding-agents
- `company` does not support at least `startPublishedDate`, `endPublishedDate`, and `excludeDomains`; one Exa coding-agent caveat also warns domain/crawl-date filters may 400 with company. Sources: https://exa.ai/docs/reference/search, https://exa.ai/docs/reference/search-api-guide-for-coding-agents
- `people` does not support `startPublishedDate`, `endPublishedDate`, or `excludeDomains`; `includeDomains` only accepts LinkedIn domains. Source: https://exa.ai/docs/reference/search
- `financial report` has strong support for financial filings, but one Exa financial-report skill says `excludeText` is not supported for that category. Source: https://exa.ai/docs/reference/financial-report-search-claude-skill

CLI should validate obvious invalid category/filter combinations before making the API call and emit copy-pasteable corrections.

### Text presence filters

`includeText` and `excludeText` filter on phrases. Exa's coding-agent docs warn that both only support single-item arrays; multi-item arrays cause 400 errors. Sources: https://exa.ai/docs/reference/search-api-guide-for-coding-agents, https://exa.ai/docs/reference/exas-capabilities-explained

CLI mapping:

```text
--include-text "Form 10-K"      # maps to includeText: ["Form 10-K"]
--exclude-text "press release"  # maps to excludeText: ["press release"]
```

Agent-friendly constraint: do not allow multiple `--include-text` values unless runtime validation confirms Exa now supports it. For now, error with: `Exa currently supports only one includeText phrase; rerun with one --include-text or move extra phrases into the query.`

### Moderation, location, compliance

- `moderation` filters unsafe content and defaults false. Source: https://exa.ai/docs/reference/search
- `userLocation` biases results toward a two-letter ISO country code. Sources: https://exa.ai/docs/reference/search, https://exa.ai/docs/changelog
- `compliance: "hipaa"` is enterprise-only and fails closed or restricts features when a requested path requires non-HIPAA-safe processors. Sources: https://exa.ai/docs/reference/search, https://exa.ai/docs/exa-spec.yaml

## Pagination and result counts

Core `/search` does not expose `offset`, cursor, or page parameters in the current schema. Docs mapping from Bing explicitly says `offset` is not supported. Sources: https://exa.ai/docs/reference/migrating-from-bing, https://exa.ai/docs/exa-spec.yaml

`numResults` defaults to 10. Current Search docs say 1-100 in the public reference, while an Exa capabilities page mentions large-scale searches with 1000 results restricted to Enterprise/Custom plans. Sources: https://exa.ai/docs/reference/search, https://exa.ai/docs/reference/exas-capabilities-explained

CLI design:

- `--limit N` maps to `numResults`.
- Default `--limit 10`.
- Validate `1 <= N <= 100` by default.
- Allow `--enterprise-limit N` only behind an explicit flag/config because higher caps are plan-dependent.
- No `--page` / `--offset` flags for `/search`; if users need more breadth, use query fan-out (`--additional-query` for deep modes or repeated search calls) and dedupe URLs client-side.

`/contents` accepts up to 100 `ids` or `urls` per request per OpenAPI. Source: https://exa.ai/docs/exa-spec.yaml

## Result fields and response shape

Search response fields include:

- top level: `requestId`, `results`, deprecated `resolvedSearchType`, deprecated `context`, optional `costDollars`, and optional `output` when synthesis/schema is requested. Source: https://exa.ai/docs/exa-spec.yaml
- result fields: `title`, `url`, `publishedDate`, `author`, `id`, `image`, `favicon`, `text`, `highlights`, `highlightScores`, `summary`, `subpages`, `entities`, and `extras`. Sources: https://exa.ai/docs/reference/search, https://exa.ai/docs/exa-spec.yaml
- synthesized output: `output.content` and `output.grounding` with field-level citations and confidence when `outputSchema` is used. Source: https://exa.ai/docs/reference/search
- cost: `costDollars.total` and sometimes endpoint-dependent `costDollars.search`; Exa says these are estimated response values, not invoice records. Source: https://exa.ai/docs/exa-spec.yaml

CLI JSON should preserve the raw fields under `raw` or expose a stable normalized envelope:

```json
{
  "schema": "exa-agent-cli.search.v1",
  "request": {"query": "...", "type": "auto", "limit": 10},
  "requestId": "...",
  "costDollars": {"total": 0.007},
  "results": [
    {"rank": 1, "title": "...", "url": "...", "id": "...", "publishedDate": "...", "highlights": ["..."]}
  ],
  "output": null,
  "warnings": []
}
```

Avoid branching on `resolvedSearchType`; OpenAPI marks it deprecated and says current production responses may return an empty string. Source: https://exa.ai/docs/exa-spec.yaml

## Highlights, snippets, text, summaries

On `/search`, request these under `contents`:

- `contents.highlights`: boolean or object. Object supports `query` and `maxCharacters`; legacy sentence/count sizing fields are deprecated. Sources: https://exa.ai/docs/reference/search, https://exa.ai/docs/reference/search-api-guide-for-coding-agents, https://exa.ai/docs/exa-spec.yaml
- `contents.text`: boolean or object. Object supports `maxCharacters`, `includeHtmlTags`, `verbosity` (`compact`, `standard`, `full`), `includeSections`, and `excludeSections`. Sources: https://exa.ai/docs/reference/search, https://exa.ai/docs/exa-spec.yaml
- `contents.summary`: object with `query` and optional JSON `schema`. Source: https://exa.ai/docs/reference/search
- `contents.context`: deprecated; use highlights or text instead. Sources: https://exa.ai/docs/reference/search-api-guide-for-coding-agents, https://exa.ai/docs/exa-spec.yaml

Exa's own best-practice docs recommend `highlights` for agent workflows and multi-step chains, `text` when broad page context is necessary, and `summary` only when Exa-side per-result synthesis is explicitly desired. They warn not to stack text, highlights, and summary by default because summaries add per-result LLM calls and text+highlights can double views/billing. Sources: https://exa.ai/docs/reference/search-best-practices, https://exa.ai/docs/reference/search-api-guide-for-coding-agents

CLI presets:

```text
--snippets                 # default: contents.highlights=true
--highlight-query TEXT     # contents.highlights.query
--text                     # contents.text={maxCharacters: default}
--text-chars N             # contents.text.maxCharacters
--text-verbosity compact|standard|full
--include-section body --exclude-section navigation
--summary [QUERY]          # contents.summary.query
--summary-schema FILE      # contents.summary.schema
```

For CLI naming, prefer `snippets` as the user-facing alias and document that it maps to Exa `highlights`; keep `--highlights` as a power-user synonym.

## Livecrawl, cache freshness, and latency knobs

Current freshness control is `maxAgeHours`:

| CLI flag | Exa value | Behavior |
| --- | --- | --- |
| omit | omit | Default: use cached content if available, livecrawl as fallback. Source: https://exa.ai/docs/reference/get-contents |
| `--fresh` | `maxAgeHours: 0` | Always livecrawl; higher latency. Source: https://exa.ai/docs/reference/get-contents |
| `--cache-only` | `maxAgeHours: -1` | Never livecrawl; maximum speed, may miss uncached content. Source: https://exa.ai/docs/reference/get-contents |
| `--max-age-hours N` | positive integer | Use cache if younger than N hours, otherwise livecrawl. Source: https://exa.ai/docs/reference/livecrawling-contents |
| `--livecrawl-timeout-ms N` | `livecrawlTimeout` | Livecrawl timeout; OpenAPI max is 90000 ms and default is 10000 ms. Source: https://exa.ai/docs/exa-spec.yaml |

The old `livecrawl` enum (`never`, `always`, `fallback`, `preferred`) still appears in OpenAPI but is deprecated; do not include it in normal help. If supported for compatibility, hide it under `--raw-livecrawl` and warn to use `--fresh`/`--cache-only`/`--max-age-hours`. Source: https://exa.ai/docs/exa-spec.yaml

Latency levers, in order:

1. `type`: `instant`/`fast` for low latency, `auto` for default, `deep*` for synthesis/reasoning. Source: https://exa.ai/docs/reference/search
2. content mode: highlights cheaper/token-smaller than full text; summary adds LLM work per result. Source: https://exa.ai/docs/reference/search-best-practices
3. freshness: `maxAgeHours: 0` forces livecrawl and increases latency. Source: https://exa.ai/docs/reference/get-contents
4. `outputSchema`: adds synthesis latency and returns `output`. Source: https://exa.ai/docs/reference/search
5. `subpages`/`extras`: more crawling/extraction per result. Sources: https://exa.ai/docs/reference/search, https://exa.ai/docs/reference/get-contents
6. `numResults`: more results can increase response size/cost. Source: https://exa.ai/docs/reference/search

## Subpages and extras

`subpages` crawls additional subpages per result or URL; OpenAPI bounds it 0-100, but docs recommend starting small such as 5-10. `subpageTarget` is a string or string array to prioritize which subpages matter. Sources: https://exa.ai/docs/reference/get-contents, https://exa.ai/docs/exa-spec.yaml

`contents.extras` currently supports at least `links`, `imageLinks`, `richLinks`, `richImageLinks`, and `codeBlocks` in OpenAPI; public tables emphasize `links` and `imageLinks`. Source: https://exa.ai/docs/exa-spec.yaml, https://exa.ai/docs/reference/get-contents

CLI mapping:

```text
--subpages N
--subpage-target docs --subpage-target api
--links N
--image-links N
--rich-links N          # advanced
--rich-image-links N    # advanced
--code-blocks N         # advanced/code workflows
```

## Streaming and async

`/search` supports `stream: true` and returns `text/event-stream`. OpenAPI stream chunks include typed frames for `text-delta`, `grounding`, `results`, `stream-reset`, `done`, and `error`; docs also describe OpenAI-compatible chat completion chunks for synthesized output. Sources: https://exa.ai/docs/reference/search, https://exa.ai/docs/exa-spec.yaml

Important limitation: Exa coding-agent docs say streaming is currently used only when `outputSchema` is provided; otherwise the endpoint returns the regular JSON response. Source: https://exa.ai/docs/exa-spec.yaml

`/contents` does not support streaming. Source: https://exa.ai/docs/reference/contents-api-guide-for-coding-agents

CLI design:

```text
--stream                 # valid only with --output-schema / --synthesize
--jsonl                  # emit SSE frames as newline-delimited normalized events
--no-stream              # force single JSON body
```

For agent ergonomics, default to non-streaming deterministic JSON. Streaming is useful for interactive human display or long deep searches, but agents usually prefer one parseable final envelope unless they explicitly request `--jsonl`.

## Cost controls

Current docs expose estimated `costDollars` in responses; OpenAPI warns these are estimates, not invoice records. Source: https://exa.ai/docs/exa-spec.yaml

Known cost/size controls:

- `numResults` / `--limit`: fewer results = less content/synthesis work. Source: https://exa.ai/docs/reference/search
- choose one content mode by default; do not stack text/highlights/summary. Source: https://exa.ai/docs/reference/search-api-guide-for-coding-agents
- `contents.text.maxCharacters` caps returned text; OpenAPI max for Search content text is 10000 characters. Source: https://exa.ai/docs/exa-spec.yaml
- `contents.highlights.maxCharacters` caps highlights; OpenAPI max is 10000 characters. Source: https://exa.ai/docs/exa-spec.yaml
- avoid `summary` unless explicitly desired because it adds a per-result LLM call. Source: https://exa.ai/docs/reference/search-api-guide-for-coding-agents
- avoid `outputSchema` unless synthesized/structured output is needed. Source: https://exa.ai/docs/reference/search
- avoid `maxAgeHours: 0` unless live freshness is required. Source: https://exa.ai/docs/reference/get-contents
- avoid large `subpages`/`extras` by default. Source: https://exa.ai/docs/reference/get-contents

CLI controls:

```text
--budget cheap|normal|deep
--max-chars N
--no-content              # search metadata only, where API/SDK supports it
--show-cost               # include costDollars in human output
--fail-over-cost FLOAT    # client-side guard if estimate is returned post-call; not a preflight guarantee
```

## Agent-friendly presets

These presets are intentionally boring and map directly to documented Exa controls.

### 1. `search` default: agent snippets

```json
{
  "type": "auto",
  "numResults": 10,
  "contents": {"highlights": true}
}
```

Use for most CLI invocations. Sources: https://exa.ai/docs/reference/search-best-practices, https://exa.ai/docs/reference/search

### 2. `search --fast`: low-latency grounding

```json
{
  "type": "fast",
  "numResults": 5,
  "contents": {"highlights": true, "maxAgeHours": -1}
}
```

Use for agent loops where stale-ish cached content is acceptable. Sources: https://exa.ai/docs/reference/search-best-practices, https://exa.ai/docs/reference/get-contents

### 3. `search --fresh`: current facts

```json
{
  "type": "auto",
  "numResults": 10,
  "contents": {"highlights": true, "maxAgeHours": 0, "livecrawlTimeout": 10000}
}
```

Use for news, prices, recent releases, and anything date-sensitive. Sources: https://exa.ai/docs/reference/get-contents, https://exa.ai/docs/reference/livecrawling-contents

### 4. `search --deep --output-schema schema.json`: structured research

```json
{
  "type": "deep",
  "numResults": 10,
  "systemPrompt": "Prefer primary sources and deduplicate domains.",
  "outputSchema": {"type": "object", "properties": {}, "required": []},
  "contents": {"highlights": true}
}
```

Use when the caller wants synthesized structured output with grounding. Sources: https://exa.ai/docs/reference/search, https://exa.ai/docs/exa-spec.yaml

### 5. `contents`: known URL fetch

```json
{
  "urls": ["https://example.com"],
  "highlights": true,
  "maxAgeHours": 24
}
```

Use when the URL is known. Remember `/contents` fields are top-level, not nested under `contents`. Sources: https://exa.ai/docs/reference/get-contents, https://exa.ai/docs/reference/contents-api-guide-for-coding-agents

### 6. `site-search`: constrained domain/path

```json
{
  "type": "auto",
  "query": "pricing terms",
  "includeDomains": ["exa.ai/docs"],
  "contents": {"highlights": true}
}
```

Use path filters and wildcard domains. Source: https://exa.ai/docs/changelog

### 7. `news`: current events

```json
{
  "type": "auto",
  "category": "news",
  "startPublishedDate": "2026-06-01",
  "contents": {"highlights": true, "maxAgeHours": 1}
}
```

Use category `news` with date filters. Sources: https://exa.ai/docs/reference/search, https://exa.ai/docs/reference/search-best-practices

## CLI validation and help rules

- Validate parameter placement: `/search` requires nested `contents.*`; `/contents` uses top-level `text`/`highlights`/`summary`. Sources: https://exa.ai/docs/reference/search-api-guide-for-coding-agents, https://exa.ai/docs/reference/contents-api-guide-for-coding-agents
- Validate documented category names and reject invented ones with exact alternatives. Source: https://exa.ai/docs/reference/search-api-guide-for-coding-agents
- Validate `includeText`/`excludeText` as single-item arrays. Source: https://exa.ai/docs/reference/search-api-guide-for-coding-agents
- Validate no `--offset`/`--page` on Search. Source: https://exa.ai/docs/reference/migrating-from-bing
- Warn on deprecated knobs: `useAutoprompt`, `context`, `livecrawl`, legacy highlight sizing fields, `resolvedSearchType`. Sources: https://exa.ai/docs/reference/search-api-guide-for-coding-agents, https://exa.ai/docs/exa-spec.yaml
- Keep stdout parseable: JSON goes to stdout; warnings and deprecation hints go to stderr.

## Primary sources used

- Exa Search API reference: https://exa.ai/docs/reference/search
- Exa Search coding-agent guide: https://exa.ai/docs/reference/search-api-guide-for-coding-agents
- Exa Search best practices: https://exa.ai/docs/reference/search-best-practices
- Exa Contents API reference: https://exa.ai/docs/reference/get-contents
- Exa Contents coding-agent guide: https://exa.ai/docs/reference/contents-api-guide-for-coding-agents
- Exa OpenAPI spec: https://exa.ai/docs/exa-spec.yaml
- Exa changelog: https://exa.ai/docs/changelog
- Exa livecrawling/content freshness docs: https://exa.ai/docs/reference/livecrawling-contents
- Exa capabilities explained: https://exa.ai/docs/reference/exas-capabilities-explained
- Exa financial-report skill reference for category-specific filter caveat: https://exa.ai/docs/reference/financial-report-search-claude-skill
- Context7 Exa docs index used for discovery: `/llmstxt/exa_ai_llms_txt`; SDK cross-check: `/exa-labs/exa-js`.

## Runtime validation still required

These should be tested against a real Exa API key before freezing the CLI contract:

1. Whether legacy `type=neural` or `type=keyword` still work despite being absent from current OpenAPI; if yes, support only as hidden deprecated aliases.
2. Exact API behavior for `contents.text` defaults in the chosen SDK version versus raw HTTP. Current JS SDK docs say `search()` may return text contents by default; raw HTTP examples still show explicit `contents`. Source: https://github.com/exa-labs/exa-js
3. Category/filter 400 behavior, especially `company`, `people`, and `financial report` with domain/date/text filters.
4. Streaming frames with and without `outputSchema`, because docs and OpenAPI frame schemas differ slightly in presentation.
5. `extras.richLinks`, `extras.richImageLinks`, and `extras.codeBlocks` availability on the public plan, since they appear in OpenAPI but are not highlighted in every public table.
