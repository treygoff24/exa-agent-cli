# Lane A — Complete official Exa API surface for an agent-first CLI

_Generated from current primary sources on 2026-06-29. Main machine-readable source: `https://exa.ai/docs/exa-spec.json`._

## Source set
- Docs OpenAPI JSON used for path/schema extraction: https://exa.ai/docs/exa-spec.json
- Official OpenAPI spec repo: https://github.com/exa-labs/openapi-spec
- Official Search API YAML in repo: https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-openapi-spec.yaml
- Official Websets API YAML in repo: https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-websets-spec.yaml
- Official docs llms-full export used for docs-only endpoints and guidance: https://exa.ai/docs/llms-full.txt
- Official docs index: https://exa.ai/docs/llms.txt
- Official Python SDK repo/README: https://github.com/exa-labs/exa-py / https://raw.githubusercontent.com/exa-labs/exa-py/master/README.md
- Official JavaScript SDK repo/README: https://github.com/exa-labs/exa-js / https://raw.githubusercontent.com/exa-labs/exa-js/master/README.md
- Context7 docs fetches used only as a primary-doc mirror per AGENTS.md: https://context7.com/exa-labs/openapi-spec/llms.txt / https://context7.com/llmstxt/exa_ai_llms_txt

## Executive API map for CLI design

- Base server for the consolidated docs OpenAPI spec is `https://api.exa.ai`; auth is API-key based (`x-api-key` in native examples/spec, Bearer for OpenAI-compatible endpoints). Source: https://exa.ai/docs/exa-spec.json; https://exa.ai/docs/llms-full.txt.
- Current docs OpenAPI JSON reports OpenAPI `3.1.0`, API title `Exa Public API`, version `2.0.0`, and 40 path entries. Source: https://exa.ai/docs/exa-spec.json.
- Official GitHub OpenAPI repo splits the Search API (`exa-openapi-spec.yaml`) and Websets API (`exa-websets-spec.yaml`), while the docs JSON also includes Agent, Research, standalone Monitors, and Websets families. Source: https://github.com/exa-labs/openapi-spec; https://exa.ai/docs/exa-spec.json.
- The docs-only surfaces not present as ordinary paths in the docs OpenAPI JSON are `/context`, `/chat/completions`, and `/responses`; they are documented in the official docs llms export and should be CLI first-class because they are agent/coding-agent oriented or OpenAI-compatible. Source: https://exa.ai/docs/llms-full.txt; https://context7.com/llmstxt/exa_ai_llms_txt.

### Endpoint family table
| Family | Methods/paths | CLI command implication | Source |
|---|---|---|---|
| Search / content / answer | `POST /search`<br>`POST /contents`<br>`POST /answer`<br>`POST /findSimilar` | Expose `search`, `contents`, `answer`; mark `find-similar` deprecated/compat; make `--contents.*` nesting unambiguous. | https://exa.ai/docs/exa-spec.json |
| Standalone monitors | `POST,GET /monitors`<br>`POST /monitors/batch`<br>`GET,PATCH,DELETE /monitors/{id}`<br>`POST /monitors/{id}/trigger`<br>`GET /monitors/{id}/runs`<br>`GET /monitors/{id}/runs/{runId}` | Expose recurring search commands with safe `batch --dry-run` default and webhook-secret handling. | https://exa.ai/docs/exa-spec.json |
| Agent API | `POST,GET /agent/runs`<br>`GET,DELETE /agent/runs/{id}`<br>`POST /agent/runs/{id}/cancel`<br>`GET /agent/runs/{id}/events` | Expose async `agent run create/get/list/cancel/events`, with poll and SSE modes. | https://exa.ai/docs/exa-spec.json |
| Team | `GET /v0/teams/me` | Expose `team me` / `limits` for quota/concurrency diagnostics. | https://exa.ai/docs/exa-spec.json |
| Research v1 | `GET,POST /research/v1`<br>`GET /research/v1/{researchId}` | Expose as legacy/research-v1 or soft-deprecated; prefer search `deep-reasoning` or Agent for new flows. | https://exa.ai/docs/exa-spec.json |
| Websets core | `POST,GET /v0/websets`<br>`GET,POST,DELETE /v0/websets/{id}`<br>`POST /v0/websets/{id}/cancel`<br>`POST /v0/websets/preview`<br>`GET /v0/websets/{webset}/items`<br>`GET,DELETE /v0/websets/{webset}/items/{id}` | Expose dataset/list-building lifecycle: create/list/get/update/delete/cancel/preview/items. | https://exa.ai/docs/exa-spec.json |
| Websets searches/enrichments | `POST /v0/websets/{webset}/searches`<br>`GET /v0/websets/{webset}/searches/{id}`<br>`POST /v0/websets/{webset}/searches/{id}/cancel`<br>`POST /v0/websets/{webset}/enrichments`<br>`PATCH,GET,DELETE /v0/websets/{webset}/enrichments/{id}`<br>`POST /v0/websets/{webset}/enrichments/{id}/cancel` | Expose nested subcommands under `websets search` and `websets enrichment`; cancellation is first-class. | https://exa.ai/docs/exa-spec.json |
| Websets webhooks/events | `POST,GET /v0/webhooks`<br>`GET,PATCH,DELETE /v0/webhooks/{id}`<br>`GET /v0/webhooks/{id}/attempts`<br>`GET /v0/events`<br>`GET /v0/events/{id}` | Expose webhook CRUD plus event/attempt inspection for reliable automation. | https://exa.ai/docs/exa-spec.json |
| Websets monitors/imports | `POST,GET /v0/monitors`<br>`GET,PATCH,DELETE /v0/monitors/{id}`<br>`GET /v0/monitors/{monitor}/runs`<br>`GET /v0/monitors/{monitor}/runs/{id}`<br>`POST,GET /v0/imports`<br>`GET,PATCH,DELETE /v0/imports/{id}` | Separate from standalone monitors; namespace as `websets monitors` and `websets imports` to avoid collision. | https://exa.ai/docs/exa-spec.json |
| Docs-only compatibility/code | `POST /context`<br>`POST /chat/completions`<br>`POST /responses` | Expose `context` for coding agents; treat OpenAI compat as low-level passthrough or migration commands, not the native default. | https://exa.ai/docs/llms-full.txt |

## Complete endpoint inventory from docs OpenAPI JSON

| Method | Path | Summary | Request body | 2xx responses | Params/defaults | Deprecated | Source |
|---|---|---|---|---|---|---|---|
| `POST` | `/search` | Search | `application/json` `SearchRequest` | `200` `application/json` `SearchResponse`<br>`200` `text/event-stream` `SearchStreamChunk` | none | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-openapi-spec.yaml |
| `POST` | `/contents` | Contents | `application/json` `ContentsRequest` | `200` `application/json` `ContentsResponse` | none | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-openapi-spec.yaml |
| `POST` | `/answer` | Answer | `application/json` `AnswerRequest` | `200` `application/json` `AnswerResponse`<br>`200` `text/event-stream` `AnswerStreamChunk` | none | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-openapi-spec.yaml |
| `POST` | `/findSimilar` | Find similar links | `application/json` `FindSimilarRequest` | `200` `application/json` `FindSimilarResponse` | none | yes | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-openapi-spec.yaml |
| `POST` | `/monitors` | Create a Monitor | `application/json` `CreateSearchMonitorParameters` | `201` `application/json` `CreateSearchMonitorResponse` | none | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-openapi-spec.yaml |
| `GET` | `/monitors` | List Monitors | none | `200` `application/json` `ListSearchMonitorsResponse` | `status` query optional string (enum=["active", "paused", "disabled"])<br>`cursor` query optional string<br>`limit` query optional integer (default=50; min=1; max=100)<br>`name` query optional string (maxLen=250)<br>`metadata` query optional object | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-openapi-spec.yaml |
| `POST` | `/monitors/batch` | Batch Action on Monitors | `application/json` `BatchMonitorsRequest` | `200` `application/json` `BatchMonitorsResponse` | none | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-openapi-spec.yaml |
| `GET` | `/monitors/{id}` | Get a Monitor | none | `200` `application/json` `SearchMonitor` | `id` path required string | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-openapi-spec.yaml |
| `PATCH` | `/monitors/{id}` | Update a Monitor | `application/json` `UpdateSearchMonitorParameters` | `200` `application/json` `SearchMonitor` | `id` path required string | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-openapi-spec.yaml |
| `DELETE` | `/monitors/{id}` | Delete a Monitor | none | `200` `application/json` `SearchMonitor` | `id` path required string | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-openapi-spec.yaml |
| `POST` | `/monitors/{id}/trigger` | Trigger a Monitor | none | `200` `application/json` `TriggerSearchMonitorResponse` | `id` path required string | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-openapi-spec.yaml |
| `GET` | `/monitors/{id}/runs` | List Runs | none | `200` `application/json` `ListSearchMonitorRunsResponse` | `id` path required string<br>`cursor` query optional string<br>`limit` query optional integer (default=50; min=1; max=100) | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-openapi-spec.yaml |
| `GET` | `/monitors/{id}/runs/{runId}` | Get a Run | none | `200` `application/json` `SearchMonitorRun` | `id` path required string<br>`runId` path required string | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-openapi-spec.yaml |
| `POST` | `/agent/runs` | Create a run | `application/json` `CreateAgentRunRequest` | `200` `application/json` `AgentRun`<br>`200` `text/event-stream` `AgentRunEvent` | `(header/undocumented-name)`  optional <br>`(header/undocumented-name)`  optional  | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-openapi-spec.yaml |
| `GET` | `/agent/runs` | List runs | none | `200` `application/json` `AgentRunList` | `limit` query optional integer (default=20; min=1; max=100)<br>`cursor` query optional AgentRunId | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-openapi-spec.yaml |
| `GET` | `/agent/runs/{id}` | Get a run | none | `200` `application/json` `AgentRun` | `id` path required AgentRunId | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-openapi-spec.yaml |
| `DELETE` | `/agent/runs/{id}` | Delete a run | none | `200` `application/json` `DeleteAgentRunResponse` | `id` path required AgentRunId | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-openapi-spec.yaml |
| `POST` | `/agent/runs/{id}/cancel` | Cancel a run | none | `200` `application/json` `AgentRun` | `id` path required AgentRunId | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-openapi-spec.yaml |
| `GET` | `/agent/runs/{id}/events` | List run events | none | `200` `application/json` `AgentRunEventList`<br>`200` `text/event-stream` `AgentRunEvent` | `id` path required AgentRunId<br>`limit` query optional integer (default=20; min=1; max=100)<br>`cursor` query optional string<br>`(header/undocumented-name)`  optional <br>`(header/undocumented-name)`  optional  | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-openapi-spec.yaml |
| `GET` | `/v0/teams/me` | Get Team Info | none | `200` `application/json` `WebsetsTeamInfo` | none | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-websets-spec.yaml |
| `GET` | `/research/v1` | List research requests | none | `200` `application/json` `ListResearchResponseDto` | `cursor` query optional string (minLen=1)<br>`limit` query optional number (default=10; min=1; max=50) | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-openapi-spec.yaml |
| `POST` | `/research/v1` | Create a new research request | `application/json` `ResearchCreateRequestDtoClass` | `201` `application/json` `ResearchDtoClass` | none | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-openapi-spec.yaml |
| `GET` | `/research/v1/{researchId}` | Get a research request by id | none | `200` `application/json` `ResearchDtoClass` | `researchId` path required string<br>`stream` query optional string<br>`events` query optional string | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-openapi-spec.yaml |
| `POST` | `/v0/websets` | Create a Webset | `application/json` `CreateWebsetParameters` | `201` `application/json` `Webset` | none | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-websets-spec.yaml |
| `GET` | `/v0/websets` | List all Websets | none | `200` `application/json` `ListWebsetsResponse` | `cursor` query optional string (minLen=1)<br>`limit` query optional number (default=25; min=1; max=100)<br>`search` query optional string (minLen=2; maxLen=50) | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-websets-spec.yaml |
| `GET` | `/v0/websets/{id}` | Get a Webset | none | `200` `application/json` `GetWebsetResponse` | `id` path required string<br>`expand` query optional array<string> | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-websets-spec.yaml |
| `POST` | `/v0/websets/{id}` | Update a Webset | `application/json` `UpdateWebsetRequest` | `200` `application/json` `Webset` | `id` path required string | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-websets-spec.yaml |
| `DELETE` | `/v0/websets/{id}` | Delete a Webset | none | `200` `application/json` `Webset` | `id` path required string | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-websets-spec.yaml |
| `POST` | `/v0/websets/{id}/cancel` | Cancel a running Webset | none | `200` `application/json` `Webset` | `id` path required string | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-websets-spec.yaml |
| `POST` | `/v0/websets/preview` | Preview a webset | `application/json` `PreviewWebsetParameters` | `200` `application/json` `PreviewWebsetResponse` | `search` path optional boolean | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-websets-spec.yaml |
| `GET` | `/v0/websets/{webset}/items/{id}` | Get an Item | none | `200` `application/json` `WebsetItem` | `webset` path required string<br>`id` path required string | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-websets-spec.yaml |
| `DELETE` | `/v0/websets/{webset}/items/{id}` | Delete an Item | none | `200` `application/json` `WebsetItem` | `webset` path required string<br>`id` path required string | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-websets-spec.yaml |
| `GET` | `/v0/websets/{webset}/items` | List all Items for a Webset | none | `200` `application/json` `ListWebsetItemResponse` | `webset` path required string<br>`cursor` query optional string (minLen=1)<br>`limit` query optional number (default=20; min=1; max=100)<br>`sourceId` query optional string | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-websets-spec.yaml |
| `POST` | `/v0/websets/{webset}/enrichments` | Create an Enrichment | `application/json` `CreateEnrichmentParameters` | `200` `application/json` `WebsetEnrichment` | `webset` path required string | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-websets-spec.yaml |
| `PATCH` | `/v0/websets/{webset}/enrichments/{id}` | Update an Enrichment | `application/json` `UpdateEnrichmentParameters` | see spec | `webset` path required string<br>`id` path required string | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-websets-spec.yaml |
| `GET` | `/v0/websets/{webset}/enrichments/{id}` | Get an Enrichment | none | `200` `application/json` `WebsetEnrichment` | `webset` path required string<br>`id` path required string | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-websets-spec.yaml |
| `DELETE` | `/v0/websets/{webset}/enrichments/{id}` | Delete an Enrichment | none | `200` `application/json` `WebsetEnrichment` | `webset` path required string<br>`id` path required string | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-websets-spec.yaml |
| `POST` | `/v0/websets/{webset}/enrichments/{id}/cancel` | Cancel a running Enrichment | none | `200` `application/json` `WebsetEnrichment` | `webset` path required string<br>`id` path required string | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-websets-spec.yaml |
| `POST` | `/v0/webhooks` | Create a Webhook | `application/json` `CreateWebhookParameters` | `200` `application/json` `Webhook` | none | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-websets-spec.yaml |
| `GET` | `/v0/webhooks` | List webhooks | none | `200` `application/json` `ListWebhooksResponse` | `cursor` query optional string (minLen=1)<br>`limit` query optional number (default=25; min=1; max=200) | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-websets-spec.yaml |
| `GET` | `/v0/webhooks/{id}` | Get a Webhook | none | `200` `application/json` `Webhook` | `id` path required string | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-websets-spec.yaml |
| `PATCH` | `/v0/webhooks/{id}` | Update a Webhook | `application/json` `UpdateWebhookParameters` | `200` `application/json` `Webhook` | `id` path required string | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-websets-spec.yaml |
| `DELETE` | `/v0/webhooks/{id}` | Delete a Webhook | none | `200` `application/json` `Webhook` | `id` path required string | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-websets-spec.yaml |
| `GET` | `/v0/webhooks/{id}/attempts` | List webhook attempts | none | `200` `application/json` `ListWebhookAttemptsResponse` | `id` path required string<br>`cursor` query optional string (minLen=1)<br>`limit` query optional number (default=25; min=1; max=200)<br>`eventType` query optional string (enum=["webset.created", "webset.deleted", "webset.paused", "webset.idle", "webset.search.created", "webset.search.canceled", "webset.search.completed", "webset.search.updated", "import.created", "import.completed", "webset.item.created", "webset.item.enriched", "monitor.created", "monitor.updated", "monitor.deleted", "monitor.run.created", "monitor.run.completed", "webset.export.created", "webset.export.completed"])<br>`successful` query optional boolean | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-websets-spec.yaml |
| `GET` | `/v0/events` | List all Events | none | `200` `application/json` `ListEventsResponse` | `cursor` query optional string (minLen=1)<br>`limit` query optional number (default=25; min=1; max=200)<br>`types` query optional array<string><br>`createdBefore` query optional string (format="date-time")<br>`createdAfter` query optional string (format="date-time") | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-websets-spec.yaml |
| `GET` | `/v0/events/{id}` | Get an Event | none | `200` `application/json` `Event` | `id` path required string | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-websets-spec.yaml |
| `POST` | `/v0/websets/{webset}/searches` | Create a Search | `application/json` `CreateWebsetSearchParameters` | `200` `application/json` `WebsetSearch` | `webset` path required string | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-websets-spec.yaml |
| `GET` | `/v0/websets/{webset}/searches/{id}` | Get a Search | none | `200` `application/json` `WebsetSearch` | `webset` path required string<br>`id` path required string | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-websets-spec.yaml |
| `POST` | `/v0/websets/{webset}/searches/{id}/cancel` | Cancel a running Search | none | `200` `application/json` `WebsetSearch` | `webset` path required string<br>`id` path required string | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-websets-spec.yaml |
| `POST` | `/v0/monitors` | Create a Monitor | `application/json` `CreateMonitorParameters` | `201` `application/json` `Monitor` | none | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-websets-spec.yaml |
| `GET` | `/v0/monitors` | List Monitors | none | `200` `application/json` `ListMonitorsResponse` | `cursor` query optional string (minLen=1)<br>`limit` query optional number (default=25; min=1; max=200)<br>`websetId` query optional string | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-websets-spec.yaml |
| `GET` | `/v0/monitors/{id}` | Get Monitor | none | `200` `application/json` `Monitor` | `id` path required string | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-websets-spec.yaml |
| `PATCH` | `/v0/monitors/{id}` | Update Monitor | `application/json` `UpdateMonitor` | `200` `application/json` `Monitor` | `id` path required string | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-websets-spec.yaml |
| `DELETE` | `/v0/monitors/{id}` | Delete Monitor | none | `200` `application/json` `Monitor` | `id` path required string | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-websets-spec.yaml |
| `GET` | `/v0/monitors/{monitor}/runs` | List Monitor Runs | none | `200` `application/json` `ListMonitorRunsResponse` | `monitor` path required string | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-websets-spec.yaml |
| `GET` | `/v0/monitors/{monitor}/runs/{id}` | Get Monitor Run | none | `200` `application/json` `MonitorRun` | `monitor` path required string<br>`id` path required string | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-websets-spec.yaml |
| `POST` | `/v0/imports` | Create an Import | `application/json` `CreateImportParameters` | `201` `application/json` `CreateImportResponse` | none | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-websets-spec.yaml |
| `GET` | `/v0/imports` | List Imports | none | `200` `application/json` `ListImportsResponse` | `cursor` query optional string (minLen=1)<br>`limit` query optional number (default=25; min=1; max=200) | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-websets-spec.yaml |
| `GET` | `/v0/imports/{id}` | Get Import | none | `200` `application/json` `Import` | `id` path required string | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-websets-spec.yaml |
| `PATCH` | `/v0/imports/{id}` | Update Import | `application/json` `UpdateImport` | `200` `application/json` `Import` | `id` path required string | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-websets-spec.yaml |
| `DELETE` | `/v0/imports/{id}` | Delete Import | none | `200` `application/json` `Import` | `id` path required string | no | https://exa.ai/docs/exa-spec.json ; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-websets-spec.yaml |

## Native Search API surfaces

### `POST /search` — search and optional contents. Source: https://exa.ai/docs/exa-spec.json; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-openapi-spec.yaml; docs guide https://exa.ai/docs/llms-full.txt.
- `SearchRequest` object; required fields: `query`.
  - `includeDomains` (optional): `anyOf(array<string> | null)`
  - `excludeDomains` (optional): `anyOf(array<string> | null)`
  - `startCrawlDate` (optional): `anyOf(string | null)`
  - `endCrawlDate` (optional): `anyOf(string | null)`
  - `startPublishedDate` (optional): `anyOf(string | null)`
  - `endPublishedDate` (optional): `anyOf(string | null)`
  - `numResults` (optional): `anyOf(integer | null)`
  - `context` (optional): `anyOf(oneOf(boolean | object) | null)`
  - `moderation` (optional): `anyOf(boolean | null)`
  - `contents` (optional): `anyOf(ContentsOptions | null)`
  - `query` (required): `string`; minLen=1 — The query string for the search.
  - `additionalQueries` (optional): `anyOf(array<string> | null)`
  - `type` (optional): `anyOf(string | null)`
  - `category` (optional): `anyOf(string | null)`
  - `userLocation` (optional): `anyOf(string | null)`
  - `compliance` (optional): `anyOf(string | null)`
  - `outputSchema` (optional): `anyOf(oneOf(object | object) | null)`
  - `systemPrompt` (optional): `anyOf(string | null)`
  - `stream` (optional): `anyOf(boolean | null)`
- `ContentsOptions` object; required fields: none.
  - `text` (optional): `anyOf(oneOf(boolean | object) | null)`
  - `highlights` (optional): `anyOf(oneOf(boolean | object) | null)`
  - `summary` (optional): `anyOf(object | null)`
  - `extras` (optional): `anyOf(object | null)`
  - `context` (optional): `anyOf(oneOf(boolean | object) | null)`
  - `livecrawl` (optional): `anyOf(string | null)`
  - `livecrawlTimeout` (optional): `anyOf(integer | null)`
  - `maxAgeHours` (optional): `anyOf(integer | null)`
  - `subpages` (optional): `anyOf(integer | null)`
  - `subpageTarget` (optional): `anyOf(oneOf(string | array<string>) | null)`

Important CLI implications for `/search`: `query` is the only required request field; content extraction belongs under `contents`, not top-level `text`/`highlights`/`summary`; search streaming is exposed via `stream: true` and `text/event-stream`; `outputSchema` plus `systemPrompt` make search a synthesized/structured-output surface. Sources: https://exa.ai/docs/exa-spec.json; https://exa.ai/docs/llms-full.txt.
Search `type` values documented for new work are `auto`, `fast`, `instant`, `deep-lite`, `deep`, and `deep-reasoning`; practical defaults are `auto` for general retrieval, `fast` with highlights for coding/agent paths, and `instant` for real-time UX. Source: https://exa.ai/docs/llms-full.txt.
Deep variants that support `additionalQueries` are `deep-lite`, `deep`, and `deep-reasoning`; object output currently enforces max nesting depth 2 and max total properties 10 in official SDK READMEs. Sources: https://raw.githubusercontent.com/exa-labs/exa-py/master/README.md; https://raw.githubusercontent.com/exa-labs/exa-js/master/README.md.

### Shared `contents` options for `/search` and `/findSimilar`. Source: https://exa.ai/docs/exa-spec.json; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-openapi-spec.yaml.
- `ContentsOptions` object; required fields: none.
  - `text` (optional): `anyOf(oneOf(boolean | object) | null)`
  - `highlights` (optional): `anyOf(oneOf(boolean | object) | null)`
  - `summary` (optional): `anyOf(object | null)`
  - `extras` (optional): `anyOf(object | null)`
  - `context` (optional): `anyOf(oneOf(boolean | object) | null)`
  - `livecrawl` (optional): `anyOf(string | null)`
  - `livecrawlTimeout` (optional): `anyOf(integer | null)`
  - `maxAgeHours` (optional): `anyOf(integer | null)`
  - `subpages` (optional): `anyOf(integer | null)`
  - `subpageTarget` (optional): `anyOf(oneOf(string | array<string>) | null)`

CLI implication: prefer flags like `--text`, `--highlights`, `--summary`, `--livecrawl`, `--max-age-hours`, `--subpages`, and `--subpage-target`, but serialize them under `contents` for `/search`; do not send `tokensNum` here. Sources: https://exa.ai/docs/exa-spec.json; common-mistakes docs in https://exa.ai/docs/llms-full.txt.

### `POST /contents` — content extraction by URL/document ID. Source: https://exa.ai/docs/exa-spec.json; https://exa.ai/docs/llms-full.txt.
- `ContentsRequest`: allOf(oneOf(object | object) & ContentsOptions)
- `ContentsResponse` object; required fields: none.
  - `requestId` (optional): `string` — Unique identifier for the request.
  - `results` (optional): `array<SearchResultOutput>`
  - `context` (optional): `string` — Deprecated. Combined context string from search results. Use highlights or text instead.
  - `statuses` (optional): `array<object>` — Status information for each requested URL or document ID.
  - `costDollars` (optional): `CostDollarsOutput`
- `SearchResultOutput` object; required fields: `url`, `title`.
  - `title` (required): `string` — The title of the search result.
  - `url` (required): `string`; format="uri" — The URL of the search result.
  - `publishedDate` (optional): `string`; format="date-time" — An estimate of the creation date, from parsing HTML content. Format is YYYY-MM-DD.
  - `author` (optional): `anyOf(string | null)` — If available, the author of the content.
  - `id` (optional): `string` — The temporary ID for the document. Useful for the /contents endpoint.
  - `image` (optional): `string`; format="uri" — The URL of an image associated with the search result, if available.
  - `favicon` (optional): `string`; format="uri" — The URL of the favicon for the search result's domain.
  - `text` (optional): `string` — The full content text of the search result.
  - `highlights` (optional): `array<string>` — Array of highlights extracted from the search result content.
  - `highlightScores` (optional): `array<number>` — Array of cosine similarity scores for each highlighted snippet.
  - `summary` (optional): `string` — Summary of the webpage.
  - `subpages` (optional): `array<object>` — Array of subpages for the search result.
  - `entities` (optional): `array<oneOf(object | object | object)>` — Structured entity data for company, person, or publication search results. Returned for supported entity-backed categories.
  - `extras` (optional): `object` — Results from extras.
- `CostDollarsOutput` object; required fields: none.
  - `total` (optional): `number`; format="float" — Estimated total dollar cost for the completed request. This response value is not an invoice record.
  - `search` (optional): `object` — Endpoint-dependent estimated search cost breakdown by retrieval mode. Instant, fast, and auto search responses may include neural search cost. Deep search modes may be reflected only in total.
CLI implication: unlike `/search`, `/contents` uses extraction options such as `text`, `highlights`, `summary`, `maxAgeHours` at the top level; `/contents` does not support streaming. Sources: https://exa.ai/docs/exa-spec.json; https://exa.ai/docs/llms-full.txt.

### `POST /answer` — grounded answer generation. Source: https://exa.ai/docs/exa-spec.json; https://exa.ai/docs/llms-full.txt.
- `AnswerRequest` object; required fields: `query`.
  - `query` (required): `string`; minLen=1 — Natural-language question or instructions for the request.
  - `stream` (optional): `boolean`; default=false — If true, the response is returned as a server-sent events (SSE) stream.
  - `text` (optional): `boolean`; default=false — If true, returns full page text with default settings. If false, disables text return.
  - `outputSchema` (optional): `object` — A [JSON Schema Draft 7](https://json-schema.org/draft-07) specification for the desired answer structure. When provided, the answer is returned as a structured object matching the schema instead of a plain string.
- `AnswerResponse` object; required fields: `answer`.
  - `requestId` (optional): `string` — Unique identifier for the request.
  - `answer` (required): `oneOf(string | object)` — The generated answer based on search results. Returns a string by default, or a structured object matching the provided outputSchema.
  - `citations` (optional): `array<object>` — Search results used to generate the answer.
  - `costDollars` (optional): `CostDollarsOutput`
- `CostDollarsOutput` object; required fields: none.
  - `total` (optional): `number`; format="float" — Estimated total dollar cost for the completed request. This response value is not an invoice record.
  - `search` (optional): `object` — Endpoint-dependent estimated search cost breakdown by retrieval mode. Instant, fast, and auto search responses may include neural search cost. Deep search modes may be reflected only in total.
CLI implication: expose `answer` for answer-shaped output with citations; support `--stream`, `--text`, `--output-schema`, and `--system-prompt`; prefer `/search` when caller needs raw ranked results or rich per-result extraction control. Sources: https://exa.ai/docs/exa-spec.json; https://exa.ai/docs/llms-full.txt.

### `POST /findSimilar` — deprecated. Source: https://exa.ai/docs/exa-spec.json; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-openapi-spec.yaml.
- `FindSimilarRequest` object; required fields: `url`.
  - `includeDomains` (optional): `anyOf(array<string> | null)`
  - `excludeDomains` (optional): `anyOf(array<string> | null)`
  - `startCrawlDate` (optional): `anyOf(string | null)`
  - `endCrawlDate` (optional): `anyOf(string | null)`
  - `startPublishedDate` (optional): `anyOf(string | null)`
  - `endPublishedDate` (optional): `anyOf(string | null)`
  - `numResults` (optional): `anyOf(integer | null)`
  - `contents` (optional): `anyOf(ContentsOptions | null)`
  - `url` (required): `string`; minLen=3 — The url for which you would like to find similar links.
  - `category` (optional): `anyOf(string | null)`
  - `excludeSourceDomain` (optional): `anyOf(boolean | null)`
- `ContentsOptions` object; required fields: none.
  - `text` (optional): `anyOf(oneOf(boolean | object) | null)`
  - `highlights` (optional): `anyOf(oneOf(boolean | object) | null)`
  - `summary` (optional): `anyOf(object | null)`
  - `extras` (optional): `anyOf(object | null)`
  - `context` (optional): `anyOf(oneOf(boolean | object) | null)`
  - `livecrawl` (optional): `anyOf(string | null)`
  - `livecrawlTimeout` (optional): `anyOf(integer | null)`
  - `maxAgeHours` (optional): `anyOf(integer | null)`
  - `subpages` (optional): `anyOf(integer | null)`
  - `subpageTarget` (optional): `anyOf(oneOf(string | array<string>) | null)`
- `FindSimilarResponse` object; required fields: none.
  - `requestId` (optional): `string` — Unique identifier for the request.
  - `context` (optional): `string` — Deprecated. Combined context string from search results. Use highlights or text instead.
  - `results` (optional): `array<SearchResultOutput>` — A list of search results containing title, URL, published date, and author.
  - `costDollars` (optional): `CostDollarsOutput`
- `SearchResultOutput` object; required fields: `url`, `title`.
  - `title` (required): `string` — The title of the search result.
  - `url` (required): `string`; format="uri" — The URL of the search result.
  - `publishedDate` (optional): `string`; format="date-time" — An estimate of the creation date, from parsing HTML content. Format is YYYY-MM-DD.
  - `author` (optional): `anyOf(string | null)` — If available, the author of the content.
  - `id` (optional): `string` — The temporary ID for the document. Useful for the /contents endpoint.
  - `image` (optional): `string`; format="uri" — The URL of an image associated with the search result, if available.
  - `favicon` (optional): `string`; format="uri" — The URL of the favicon for the search result's domain.
  - `text` (optional): `string` — The full content text of the search result.
  - `highlights` (optional): `array<string>` — Array of highlights extracted from the search result content.
  - `highlightScores` (optional): `array<number>` — Array of cosine similarity scores for each highlighted snippet.
  - `summary` (optional): `string` — Summary of the webpage.
  - `subpages` (optional): `array<object>` — Array of subpages for the search result.
  - `entities` (optional): `array<oneOf(object | object | object)>` — Structured entity data for company, person, or publication search results. Returned for supported entity-backed categories.
  - `extras` (optional): `object` — Results from extras.
- `CostDollarsOutput` object; required fields: none.
  - `total` (optional): `number`; format="float" — Estimated total dollar cost for the completed request. This response value is not an invoice record.
  - `search` (optional): `object` — Endpoint-dependent estimated search cost breakdown by retrieval mode. Instant, fast, and auto search responses may include neural search cost. Deep search modes may be reflected only in total.
CLI implication: if implemented, name as `find-similar` but mark deprecated and print a migration hint to `/search` with a query describing the source URL; OpenAPI marks the operation deprecated and says to prefer `/search`. Source: https://exa.ai/docs/exa-spec.json; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-openapi-spec.yaml.

## Agent API surfaces

`/agent` is asynchronous; create returns a run object, then callers poll `GET /agent/runs/{id}` or stream/replay events. Source: https://exa.ai/docs/exa-spec.json; https://exa.ai/docs/llms-full.txt.
- `CreateAgentRunRequest` object; required fields: `query`.
  - `query` (required): `string`; minLen=1 — Natural-language question or instructions for the request.
  - `systemPrompt` (optional): `string` — Additional instructions that guide generated output or agent behavior. Use this for source preferences, novelty constraints, duplication constraints, or other behavior guidance.
  - `input` (optional): `object` — Records to process and records or entities to exclude from the answer.
  - `outputSchema` (optional): `anyOf(object | null)`
  - `effort` (optional): `AgentEffort`
  - `previousRunId` (optional): `AgentRunId` — Completed run ID to continue from. Must belong to the same team.
  - `metadata` (optional): `object` — Caller-provided metadata stored with the run.
  - `dataSources` (optional): `array<AgentDataSource>` — Exa Connect data providers to enable for the run. Each entry enables all of that provider's tools.
  - `budget` (optional): `object` — Accepted for compatibility and currently ignored.
- `AgentEffort`: string default="auto"; enum=["minimal", "low", "medium", "high", "xhigh", "auto"]
- `AgentRunId`: string minLen=1; maxLen=200
- `AgentDataSource` object; required fields: `provider`.
  - `provider` (required): `AgentDataSourceProvider` — Exa Connect data provider to enable for the run. All provider tools are available by default.
- `AgentDataSourceProvider`: string enum=["fiber_ai", "financial_datasets", "similarweb", "baselayer", "affiliate", "particle_news", "jinko"]
- `AgentRun` object; required fields: `createdAt`, `status`, `output`, `costDollars`, `request`, `object`, `id`, `usage`, `stopReason`, `completedAt`.
  - `id` (required): `AgentRunId`
  - `object` (required): `string`
  - `status` (required): `AgentRunStatus`
  - `stopReason` (required): `anyOf(AgentStopReason | null)` — Why the run stopped. `null` while the run is queued or running.
  - `createdAt` (required): `string`; format="date-time" — When the run was created
  - `completedAt` (required): `anyOf(string | null)`; format="date-time"
  - `request` (required): `anyOf(AgentRunRequest | null)`
  - `output` (required): `AgentRunOutput`
  - `usage` (required): `AgentUsage`
  - `costDollars` (required): `AgentCostDollars`
- `AgentRunId`: string minLen=1; maxLen=200
- `AgentRunStatus`: string enum=["queued", "running", "completed", "failed", "cancelled"]
- `AgentStopReason`: string enum=["schema_satisfied", "budget_reached", "error", "cancelled"]
- `AgentRunRequest` object; required fields: none.
  - `query` (optional): `string`; minLen=1 — Natural-language question or instructions for the request.
  - `systemPrompt` (optional): `string` — Additional instructions that guide generated output or agent behavior. Use this for source preferences, novelty constraints, duplication constraints, or other behavior guidance.
  - `input` (optional): `object`
  - `outputSchema` (optional): `anyOf(object | null)`
  - `effort` (optional): `AgentEffort`
  - `previousRunId` (optional): `AgentRunId`
  - `metadata` (optional): `object` — Caller-provided key-value metadata for your own tracking.
  - `dataSources` (optional): `array<AgentDataSourceOutput>` — Exa Connect data providers configured for the run.
- `AgentEffort`: string default="auto"; enum=["minimal", "low", "medium", "high", "xhigh", "auto"]
- `AgentDataSourceOutput` object; required fields: `provider`.
  - `provider` (required): `AgentDataSourceProvider` — Exa Connect data provider to enable for the run. All provider tools are available by default.
- `AgentRunOutput` object; required fields: `text`, `grounding`, `structured`.
  - `text` (required): `string` — Natural-language answer or summary.
  - `structured` (required): `anyOf(JsonValue | null)` — Validated JSON matching `outputSchema`, or `null` when no schema was provided.
  - `grounding` (required): `array<AgentGrounding>` — Field-level citations emitted by the run.
- `JsonValue`: oneOf(null | boolean | number | string | array<JsonValue> | object)
- `AgentGrounding` object; required fields: `citations`, `field`.
  - `field` (required): `string` — Output field the citations support.
  - `citations` (required): `array<AgentCitation>`
  - `confidence` (optional): `anyOf(string | null)`
- `AgentUsage` object; required fields: `searches`, `agentComputeUnits`, `emails`, `phoneNumbers`.
  - `agentComputeUnits` (required): `number`; min=0
  - `searches` (required): `integer`; min=0
  - `emails` (required): `integer`; min=0
  - `phoneNumbers` (required): `integer`; min=0
  - `dataSources` (optional): `AgentDataSourceUsage`
- `AgentDataSourceUsage`: object
- `AgentCostDollars` object; required fields: `phoneNumbers`, `agentCompute`, `search`, `emails`, `total`.
  - `total` (required): `number`; min=0
  - `agentCompute` (required): `number`; min=0
  - `search` (required): `number`; min=0
  - `emails` (required): `number`; min=0
  - `phoneNumbers` (required): `number`; min=0
  - `dataSources` (optional): `AgentDataSourceCost`
- `AgentDataSourceCost`: object
- `AgentRunEvent` object; required fields: `event`, `data`, `createdAt`, `id`.
  - `id` (required): `string` — Event ID within the run.
  - `event` (required): `string`; enum=["agent_run.created", "agent_run.started", "agent_run.completed", "agent_run.failed", "agent_run.cancelled"]
  - `data` (required): `JsonValue`
  - `createdAt` (required): `string`; format="date-time" — When the event was created
- `JsonValue`: oneOf(null | boolean | number | string | array<JsonValue> | object)
- `AgentErrorResponse` object; required fields: `error`.
  - `error` (required): `AgentError`
- `AgentError` object; required fields: `code`, `message`, `type`.
  - `type` (required): `string`; enum=["INVALID_REQUEST", "AUTHENTICATION_ERROR", "RATE_LIMIT_ERROR", "NOT_FOUND", "SERVER_ERROR"]
  - `code` (required): `string`; enum=["INVALID_REQUEST", "TEAM_NOT_FOUND", "RUN_NOT_FOUND", "PREVIOUS_RUN_NOT_FOUND", "PREVIOUS_RUN_NOT_COMPLETED", "CONCURRENCY_LIMIT_REACHED", "INVALID_OUTPUT_SCHEMA", "INVALID_DATA_SOURCE", "TIMEOUT", "SERVER_ERROR"]
  - `message` (required): `string`
Agent run request fields include required `query`, optional `systemPrompt`, `input`, `outputSchema`, `effort`, `previousRunId`, `metadata`, `dataSources`, and compatibility-only `budget` currently documented as ignored. Source: https://exa.ai/docs/exa-spec.json; https://exa.ai/docs/llms-full.txt.
Agent effort values documented in the official docs are `minimal`, `low`, `medium`, `high`, `xhigh`, and `auto`; `auto` lets Exa choose effort. Source: https://exa.ai/docs/llms-full.txt.
Agent run IDs use the `agent_run_` prefix; follow-up runs use `previousRunId`, which must be completed and from the same team. Source: https://exa.ai/docs/exa-spec.json; https://exa.ai/docs/llms-full.txt.
Event streaming: `POST /agent/runs` with `Accept: text/event-stream` streams lifecycle events; `GET /agent/runs/{id}/events` returns JSON pagination or SSE replay using `Last-Event-ID`. Source: https://exa.ai/docs/exa-spec.json; https://exa.ai/docs/llms-full.txt.
Connect `dataSources` provider IDs documented in official Connect docs include `fiber_ai`, `similarweb`, `baselayer`, `affiliatecom`, `particle`, `financialdatasets`, `jinko`, and additional partners; provider cost/usage is returned under per-provider usage/cost maps when nonzero. Source: https://exa.ai/docs/llms-full.txt; https://raw.githubusercontent.com/exa-labs/exa-js/master/README.md.

## Standalone Monitors API surfaces

Standalone monitors live at `/monitors` and are distinct from Websets `/v0/monitors`; they run recurring Exa searches on a schedule and deliver webhook events. Source: https://exa.ai/docs/exa-spec.json; https://exa.ai/docs/llms-full.txt.
- `CreateSearchMonitorParameters` object; required fields: `search`, `webhook`.
  - `name` (optional): `string` — An optional name for the monitor
  - `search` (required): `SearchMonitorSearch`
  - `trigger` (optional): `SearchMonitorTrigger`
  - `outputSchema` (optional): `SearchMonitorOutputSchema`
  - `metadata` (optional): `object` — Optional key-value metadata. Echoed back in webhook deliveries so you can route updates to systems like Slack.
  - `webhook` (required): `SearchMonitorWebhook`
- `SearchMonitorSearch` object; required fields: `query`.
  - `query` (required): `string`; minLen=1 — The query string for the search.
  - `numResults` (optional): `integer`; default=10; min=1; max=100 — Number of results to return. Limits vary by search type. The maximum public limit is 100 results. Contact sales (hello@exa.ai) to discuss higher limits.
  - `contents` (optional): `SearchMonitorContents`
- `SearchMonitorContents` object; required fields: none.
  - `text` (optional): `oneOf(boolean | object)` — Text extraction options for each result.
  - `highlights` (optional): `oneOf(boolean | object)` — Text snippets the LLM identifies as most relevant from each page.
  - `summary` (optional): `oneOf(boolean | object)` — Return an LLM-generated summary. Pass `true` for defaults, or an object with `query` and `maxTokens`.
  - `extras` (optional): `object` — Extra parameters to pass.
  - `context` (optional): `oneOf(boolean | object)` — Deprecated: Use highlights or text instead. Returns page contents as a combined context string.
  - `livecrawl` (optional): `oneOf(string | string)` — Crawl strategy for fetching page content
  - `livecrawlTimeout` (optional): `integer`; default=10000; max=90000 — The timeout for livecrawling in milliseconds.
  - `maxAgeHours` (optional): `integer`; min=-1; max=720 — Maximum age of cached content in hours. Positive values use cached content if it is less than this many hours old; 0 fetches fresh content and is the supported way to apply text rendering options to newly fetched pages; -1 always uses cache; omitted uses fallback fetching when cached content is unavailable. Maximum supported value is 720 hours.
  - `filterEmptyResults` (optional): `boolean` — Filter out results with no content
  - `subpages` (optional): `integer`; default=0; min=0; max=100 — The number of subpages to crawl. The actual number crawled may be limited by system constraints.
  - `subpageTarget` (optional): `oneOf(string | array<string>)` — Term to find specific subpages of search results. Can be a single string or an array of strings.
- `SearchMonitorTrigger` object; required fields: `period`, `type`.
  - `type` (required): `string`; default="interval" — The type of trigger. Currently only `interval` is supported.
  - `period` (required): `string` — A duration string specifying how often the monitor runs (e.g., "1h", "6h", "1d", "7d"). Single-unit only. Minimum interval is 1 hour. The schedule is anchored to the monitor's creation time (e.g., a daily monitor created at 2:30 PM runs daily around 2:30 PM).
- `SearchMonitorOutputSchema`: anyOf(oneOf(object | object) | null)
- `SearchMonitorWebhook` object; required fields: `url`.
  - `url` (required): `string`; format="uri" — The HTTPS URL to receive webhook events. Must not point to localhost or private IP ranges.
  - `events` (optional): `array<string>` — Which events to subscribe to. Defaults to all events if not specified.
- `BatchMonitorsRequest` object; required fields: `action`, `filter`.
  - `action` (required): `string`; enum=["delete", "pause", "unpause"] — The action to perform on matching monitors. `delete` permanently removes them, `pause` sets their status to paused, and `unpause` sets their status to active.
  - `filter` (required): `object` — At least one filter field must be provided to prevent accidental bulk operations.
  - `dry_run` (optional): `boolean`; default=true — When `true`, returns the monitors that would be affected without performing the action. Defaults to `true`.
  - `limit` (optional): `integer`; default=50; min=1; max=500 — Maximum number of monitors to process in a single request. Defaults to 50, maximum 500.
Create monitor requires `search` and `webhook`; trigger defaults are interval-based, `period` is a single-unit duration with minimum 1 hour, and the schedule is anchored to creation time. Source: https://exa.ai/docs/exa-spec.json; https://exa.ai/docs/llms-full.txt.
Batch monitor action supports `delete`, `pause`, and `unpause`; `dry_run` defaults to `true`, and at least one filter must be provided. Source: https://exa.ai/docs/exa-spec.json.
CLI implication: make `monitors batch` dry-run by default and require explicit `--execute`/`--yes` for deletes; namespace Websets monitors separately. Source: https://exa.ai/docs/exa-spec.json; agent-ergonomics implication from CLI design task.

## Research v1 and replacement guidance

- `ResearchCreateRequestDtoClass` object; required fields: `instructions`.
  - `model` (optional): `string`; default="exa-research"; enum=["exa-research-fast", "exa-research", "exa-research-pro"] — Research model to use. exa-research is faster and cheaper, while exa-research-pro provides more thorough analysis and stronger reasoning.
  - `instructions` (required): `string`; maxLen=50000 — Instructions for what you would like research on. A good prompt clearly defines what information you want to find, how research should be conducted, and what the output should look like.
  - `outputSchema` (optional): `object` — JSON Schema to enforce structured output. When provided, the research output will be validated against this schema and returned as parsed JSON.
- `ResearchDtoClass`: oneOf(object | object | object | object | object)
`POST /research/v1` creates research requests with required `instructions`, optional `model` default `exa-research`, and optional `outputSchema`; model enum is `exa-research-fast`, `exa-research`, `exa-research-pro`. Source: https://exa.ai/docs/exa-spec.json.
`GET /research/v1/{researchId}` supports `stream=true` for SSE and `events=true` to include detailed event log. Source: https://exa.ai/docs/exa-spec.json.
Official current guidance says use `deep-reasoning` when you want a current Exa-first path instead of legacy `/research/v1` structured output flows. Source: https://exa.ai/docs/llms-full.txt.

## Websets API surfaces

Websets are exposed under `/v0/...` in the docs OpenAPI JSON, but raw cURL examples use `https://api.exa.ai/websets/v0/...`; path/server handling differs by source, so a CLI should centralize base URL construction and test it against the official current docs before release. Sources: https://exa.ai/docs/exa-spec.json; https://exa.ai/docs/llms-full.txt; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-websets-spec.yaml.
- `CreateWebsetParameters` object; required fields: none.
  - `title` (optional): `string`; minLen=1; nullable=True — Optional name that appears anywhere the Webset is displayed. Leave empty to have Exa generate one automatically.
  - `search` (optional): `object` — Create initial search for the Webset.
  - `import` (optional): `array<object>` — Import data from existing Websets and Imports into this Webset.
  - `enrichments` (optional): `array<CreateEnrichmentParameters>` — Add enrichments to extract additional data from found items. Enrichments automatically search for and extract specific information (like contact details, funding data, employee counts, etc.) from each item added to your Webset.
  - `exclude` (optional): `array<object>` — Global exclusion sources (existing imports or websets) that apply to all operations within this Webset. Any results found within these sources will be omitted across all search and import operations.
  - `externalId` (optional): `string`; maxLen=300 — The external identifier for the webset. You can use this to reference the Webset by your own internal identifiers.
  - `metadata` (optional): `object` — Set of key-value pairs you want to associate with this object.
- `CreateEnrichmentParameters` object; required fields: `description`.
  - `description` (required): `string`; minLen=1; maxLen=5000 — Provide a description of the enrichment task you want to perform to each Webset Item.
  - `format` (optional): `string`; enum=["text", "date", "number", "options", "email", "phone", "url"] — Format of the enrichment response. We automatically select the best format based on the description. If you want to explicitly specify the format, you can do so here.
  - `options` (optional): `array<object>` — When the format is options, the different options for the enrichment agent to choose from.
  - `metadata` (optional): `object` — Set of key-value pairs you want to associate with this object.
- `PreviewWebsetParameters` object; required fields: `search`.
  - `search` (required): `object`
- `CreateWebsetSearchParameters` object; required fields: `count`, `query`.
  - `count` (required): `number`; min=1 — Number of Items the Search will attempt to find. The actual number of Items found may be less than this number depending on the query complexity.
  - `query` (required): `string`; minLen=1; maxLen=5000 — Natural language search query describing what you are looking for. Be specific and descriptive about your requirements, characteristics, and any constraints that help narrow down the results. Any URLs provided will be crawled and used as additional context for the search.
  - `entity` (optional): `Entity` — Entity the search will return results for. It is not required to provide it, we automatically detect the entity from all the information provided in the query. Only use this when you need more fine control.
  - `criteria` (optional): `array<CreateCriterionParameters>` — Criteria every item is evaluated against. It's not required to provide your own criteria, we automatically detect the criteria from all the information provided in the query. Only use this when you need more fine control.
  - `maxPeoplePerCompany` (optional): `integer`; min=1 — Optional soft cap for people searches. When set, the search will try to include at most this many matching people from the same current employer company.
  - `exclude` (optional): `array<object>` — Sources (existing imports or websets) to exclude from search results. Any results found within these sources will be omitted to prevent finding them during search.
  - `scope` (optional): `array<object>` — Limit the search to specific sources (existing imports). Any results found within these sources matching the search criteria will be included in the Webset.
  - `recall` (optional): `boolean` — Whether to provide an estimate of how many total relevant results could exist for this search. Result of the analysis will be available in the `recall` field within the search request.
  - `behavior` (optional): `WebsetSearchBehavior`; default="override" — How this search interacts with existing items in the Webset: - **override**: Replace existing items and evaluate all items against new criteria - **append**: Add new items to existing ones, keeping items that match the new criteria
  - `metadata` (optional): `object` — Set of key-value pairs you want to associate with this object.
- `Entity`: oneOf(CompanyEntity | PersonEntity | ArticleEntity | ResearchPaperEntity | CustomEntity)
- `CreateCriterionParameters` object; required fields: `description`.
  - `description` (required): `string`; minLen=1; maxLen=1000 — The description of the criterion
- `WebsetSearchBehavior`: string enum=["override", "append"]
- `CreateEnrichmentParameters` object; required fields: `description`.
  - `description` (required): `string`; minLen=1; maxLen=5000 — Provide a description of the enrichment task you want to perform to each Webset Item.
  - `format` (optional): `string`; enum=["text", "date", "number", "options", "email", "phone", "url"] — Format of the enrichment response. We automatically select the best format based on the description. If you want to explicitly specify the format, you can do so here.
  - `options` (optional): `array<object>` — When the format is options, the different options for the enrichment agent to choose from.
  - `metadata` (optional): `object` — Set of key-value pairs you want to associate with this object.
- `CreateWebhookParameters` object; required fields: `url`, `events`.
  - `events` (required): `array<EventType>` — The events to trigger the webhook
  - `url` (required): `string`; format="uri" — The URL to send the webhook to
  - `metadata` (optional): `object` — Set of key-value pairs you want to associate with this object.
- `EventType`: string enum=["webset.created", "webset.deleted", "webset.paused", "webset.idle", "webset.search.created", "webset.search.canceled", "webset.search.completed", "webset.search.updated", "import.created", "import.completed", "webset.item.created", "webset.item.enriched", "monitor.created", "monitor.updated", "monitor.deleted", "monitor.run.created", "monitor.run.completed", "webset.export.created", "webset.export.completed"]
- `CreateMonitorParameters` object; required fields: `websetId`, `cadence`, `behavior`.
  - `websetId` (required): `string` — The id of the Webset
  - `cadence` (required): `object` — How often the monitor will run
  - `behavior` (required): `object` — Behavior to perform when monitor runs
  - `metadata` (optional): `object`
- `CreateImportParameters`: oneOf(object)
Websets create supports optional `title`, initial `search`, `import`, `enrichments`, `exclude`, `externalId`, and `metadata`; `externalId` conflicts return 409. Source: https://exa.ai/docs/exa-spec.json.
Webset list defaults: `limit` defaults to 25 with max 100; Webset items list defaults to 20 max 100; Websets webhooks/events/imports/monitors list defaults are generally 25 with max 200 where specified. Source: https://exa.ai/docs/exa-spec.json.
Webset search create requires `count` and `query`, can auto-detect `entity` and `criteria`, supports `maxPeoplePerCompany`, `exclude`, `scope`, `recall`, `behavior` default `override`, and metadata. Source: https://exa.ai/docs/exa-spec.json.
Webset search `behavior` options are documented as `override` (replace/evaluate existing items) and `append` (add to existing items), with default `override`. Source: https://exa.ai/docs/exa-spec.json.
Enrichment create requires `description`; optional `format` enum is `text`, `date`, `number`, `options`, `email`, `phone`, `url`; when format is `options`, provide `options`. Source: https://exa.ai/docs/exa-spec.json.
Deleting a Webset removes the Webset and Items; deleting an Item cancels enrichment for it; deleting or canceling an Enrichment cancels running enrichment and cannot be resumed after cancellation. Source: https://exa.ai/docs/exa-spec.json.
CLI implication: all Websets destructive verbs need dry-run/confirmation wrappers; all long-running create/search/enrichment/monitor/import operations need `--wait`, `--poll-interval`, and `--events` modes. Source: https://exa.ai/docs/exa-spec.json; https://exa.ai/docs/llms-full.txt; agent-ergonomics implication from CLI design task.

## Docs-only `/context` and OpenAI-compatible surfaces

`POST /context` is the Exa Code endpoint for coding agents; request has required `query` (max 2000 chars in current docs) and `tokensNum` as `"dynamic"` or integer 50-100000; response fields include `requestId`, `query`, formatted `response`, `resultsCount`, `costDollars`, `searchTime`, and `outputTokens`. Source: https://exa.ai/docs/llms-full.txt; https://context7.com/llmstxt/exa_ai_llms_txt.
CLI implication: expose `context` separately from `search`; default `tokensNum` to `dynamic`; output is a formatted context blob, not ranked result rows. Source: https://exa.ai/docs/llms-full.txt; https://context7.com/llmstxt/exa_ai_llms_txt.
`POST /chat/completions` is OpenAI Chat Completions-compatible; use base URL `https://api.exa.ai`, Bearer auth, model names `exa`, `exa-research`, or `exa-research-pro`, and `extra_body` for Exa-specific fields such as `text`. Source: https://exa.ai/docs/llms-full.txt; https://context7.com/llmstxt/exa_ai_llms_txt.
`POST /responses` is OpenAI Responses-compatible for research-style flows; documented model names are `exa-research` and `exa-research-pro`. Source: https://exa.ai/docs/llms-full.txt; https://context7.com/llmstxt/exa_ai_llms_txt.
Compatibility endpoints are secondary for new Exa-first integrations; native Exa endpoints expose clearer Exa-specific semantics. Source: https://exa.ai/docs/llms-full.txt.

## SDK surface and casing

Python SDK package is `exa-py` (`pip install exa-py`, Python 3.9+), with current primary methods `search`, `stream_search`, `get_contents`, `answer`, `stream_answer`, `agent.runs.create/get/list/poll_until_finished`, `monitors.*`, and `websets.*`. Sources: https://raw.githubusercontent.com/exa-labs/exa-py/master/README.md; https://exa.ai/docs/llms-full.txt.
TypeScript SDK package is `exa-js` (`npm install exa-js`), with current primary methods `search`, `streamSearch`, `getContents`, `answer`, `streamAnswer`, `agent.runs.create/get/list/pollUntilFinished`, `monitors.*`, and `websets.*`. Sources: https://raw.githubusercontent.com/exa-labs/exa-js/master/README.md; https://exa.ai/docs/llms-full.txt.
Raw HTTP and TypeScript use camelCase (`numResults`, `outputSchema`, `systemPrompt`, `maxCharacters`, `maxAgeHours`); Python core endpoint methods use snake_case (`num_results`, `output_schema`, `system_prompt`, `max_characters`, `max_age_hours`). Source: https://exa.ai/docs/llms-full.txt; https://raw.githubusercontent.com/exa-labs/exa-py/master/README.md; https://raw.githubusercontent.com/exa-labs/exa-js/master/README.md.
Python docs currently say `search()` returns text contents with a 10,000-character default unless disabled with `contents=False`; treat this as SDK behavior, not raw HTTP default. Source: https://exa.ai/docs/llms-full.txt.
Both SDKs expose helper methods such as `search_and_contents` / `searchAndContents`, but official current guidance treats underlying endpoint families (`/search`, `/contents`) as the normative design basis. Sources: https://raw.githubusercontent.com/exa-labs/exa-py/master/README.md; https://raw.githubusercontent.com/exa-labs/exa-js/master/README.md; https://exa.ai/docs/llms-full.txt.

## Defaults, constraints, deprecations, and feature flags to encode in CLI help

- `/search.query` is required with min length 1. Source: https://exa.ai/docs/exa-spec.json.
- `/answer.query` is required with min length 1; `stream` default is false; `text` default is false. Source: https://exa.ai/docs/exa-spec.json.
- Standalone monitors list `limit` defaults to 50 with min 1 max 100. Source: https://exa.ai/docs/exa-spec.json.
- Standalone monitor run list `limit` defaults to 50 with min 1 max 100. Source: https://exa.ai/docs/exa-spec.json.
- Standalone monitor batch `dry_run` defaults to true and `limit` defaults to 50 with max 500. Source: https://exa.ai/docs/exa-spec.json.
- Agent list and event list `limit` defaults to 20 with min 1 max 100. Source: https://exa.ai/docs/exa-spec.json.
- Research list `limit` defaults to 10 with min 1 max 50. Source: https://exa.ai/docs/exa-spec.json.
- Websets list `limit` defaults to 25 with min 1 max 100; items list defaults to 20 with min 1 max 100. Source: https://exa.ai/docs/exa-spec.json.
- Webhooks/events/imports/Websets monitors list `limit` defaults to 25 with max 200 where specified. Source: https://exa.ai/docs/exa-spec.json.
- `/findSimilar` is deprecated; prefer `/search` with a query describing the source. Source: https://exa.ai/docs/exa-spec.json.
- `context` response is formatted code context, not a ranked search result list. Source: https://exa.ai/docs/llms-full.txt.
- `useAutoprompt` and legacy highlight sizing fields are deprecated/legacy mistake surfaces; avoid exposing them as primary flags. Source: https://exa.ai/docs/llms-full.txt.
- `livecrawl: "true"` as a string is a documented mistake; prefer `maxAgeHours` for new examples. Source: https://exa.ai/docs/llms-full.txt.
- `budget.maxCostDollars` on Agent is accepted for compatibility but currently ignored; do not present it as a hard cap. Source: https://exa.ai/docs/llms-full.txt.
- Search object output schema currently enforces max depth 2 and max total properties 10 in SDK docs. Source: https://raw.githubusercontent.com/exa-labs/exa-py/master/README.md ; https://raw.githubusercontent.com/exa-labs/exa-js/master/README.md.

## Agent-first CLI command design recommendations from the API map

1. Use native Exa families as top-level commands: `search`, `contents`, `answer`, `context`, `agent`, `monitors`, `websets`, `research-v1`, `openai-compat`, and `team`. Source basis: https://exa.ai/docs/exa-spec.json; https://exa.ai/docs/llms-full.txt.
2. Keep `find-similar` available only as deprecated compatibility, with an exact migration hint to `search --query "pages similar to <url> ..."`; OpenAPI marks `/findSimilar` deprecated. Source: https://exa.ai/docs/exa-spec.json.
3. Make all JSON payload construction inspectable: every command should support `--print-request`/`--dry-run-request` because several APIs have similar names but different nesting (`/search.contents.text` vs `/contents.text`). Source: https://exa.ai/docs/exa-spec.json; https://exa.ai/docs/llms-full.txt.
4. Add `--wait`, `--poll-interval`, `--stream`, `--events`, and `--cursor` consistently for Agent, Research, Monitors, Websets operations that are async, paginated, or evented. Source: https://exa.ai/docs/exa-spec.json; https://exa.ai/docs/llms-full.txt.
5. Default destructive/bulk Websets and Monitors commands to preview/dry-run where the API supports it, and add CLI-level confirmation where the API does not; API deletes/cancels are irreversible or cannot resume in several places. Source: https://exa.ai/docs/exa-spec.json.
6. Encode SDK casing explicitly in docs/examples: raw HTTP + TS camelCase, Python snake_case for core endpoints, with Websets/Monitors namespace exceptions. Source: https://exa.ai/docs/llms-full.txt; https://raw.githubusercontent.com/exa-labs/exa-py/master/README.md; https://raw.githubusercontent.com/exa-labs/exa-js/master/README.md.
7. Provide `capabilities --json` for the CLI that returns endpoint families, auth headers, command-to-path mapping, defaults, feature/deprecation flags, pagination fields, and streaming support; this follows from the CLI goal and the current API surface complexity. Source basis: https://exa.ai/docs/exa-spec.json; https://exa.ai/docs/llms-full.txt.

## What still needs runtime validation

- Confirm Websets base path at runtime (`/v0/...` from OpenAPI path entries vs `https://api.exa.ai/websets/v0/...` from raw docs examples) before hard-coding the CLI base URL. Sources showing ambiguity: https://exa.ai/docs/exa-spec.json; https://exa.ai/docs/llms-full.txt; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-websets-spec.yaml.
- Confirm auth acceptance for native endpoints (`x-api-key`) and OpenAI-compatible endpoints (`Authorization: Bearer`) with real credentials in a non-mutating smoke test; docs show both conventions by surface. Sources: https://exa.ai/docs/exa-spec.json; https://exa.ai/docs/llms-full.txt.
- Confirm exact enum values not expanded in the docs JSON viewer for `AgentEffort`, `AgentRunStatus`, `AgentStopReason`, `AgentDataSourceProvider`, `Entity`, `WebsetSearchBehavior`, and some Websets response status fields against the YAML/spec or generated SDK types before generating strict CLI validators. Source: https://exa.ai/docs/exa-spec.json; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-openapi-spec.yaml; https://raw.githubusercontent.com/exa-labs/openapi-spec/master/exa-websets-spec.yaml.
- Confirm SDK default `search()` text contents behavior against installed SDK versions if the CLI wraps SDKs rather than raw HTTP; current docs describe it as Python SDK behavior. Source: https://exa.ai/docs/llms-full.txt.
- Confirm rate-limit and billing behavior with a real account; response schemas include `costDollars`, team info includes concurrency usage/limits, but actual quota policy is account-dependent. Source: https://exa.ai/docs/exa-spec.json.

## Appendix A — Response schemas used by multiple endpoints

### `SearchResponse`. Source: https://exa.ai/docs/exa-spec.json.
- `SearchResponse`: oneOf(object | object)

### `ContentsResponse`. Source: https://exa.ai/docs/exa-spec.json.
- `ContentsResponse` object; required fields: none.
  - `requestId` (optional): `string` — Unique identifier for the request.
  - `results` (optional): `array<SearchResultOutput>`
  - `context` (optional): `string` — Deprecated. Combined context string from search results. Use highlights or text instead.
  - `statuses` (optional): `array<object>` — Status information for each requested URL or document ID.
  - `costDollars` (optional): `CostDollarsOutput`
- `SearchResultOutput` object; required fields: `url`, `title`.
  - `title` (required): `string` — The title of the search result.
  - `url` (required): `string`; format="uri" — The URL of the search result.
  - `publishedDate` (optional): `string`; format="date-time" — An estimate of the creation date, from parsing HTML content. Format is YYYY-MM-DD.
  - `author` (optional): `anyOf(string | null)` — If available, the author of the content.
  - `id` (optional): `string` — The temporary ID for the document. Useful for the /contents endpoint.
  - `image` (optional): `string`; format="uri" — The URL of an image associated with the search result, if available.
  - `favicon` (optional): `string`; format="uri" — The URL of the favicon for the search result's domain.
  - `text` (optional): `string` — The full content text of the search result.
  - `highlights` (optional): `array<string>` — Array of highlights extracted from the search result content.
  - `highlightScores` (optional): `array<number>` — Array of cosine similarity scores for each highlighted snippet.
  - `summary` (optional): `string` — Summary of the webpage.
  - `subpages` (optional): `array<object>` — Array of subpages for the search result.
  - `entities` (optional): `array<oneOf(object | object | object)>` — Structured entity data for company, person, or publication search results. Returned for supported entity-backed categories.
  - `extras` (optional): `object` — Results from extras.
- `CostDollarsOutput` object; required fields: none.
  - `total` (optional): `number`; format="float" — Estimated total dollar cost for the completed request. This response value is not an invoice record.
  - `search` (optional): `object` — Endpoint-dependent estimated search cost breakdown by retrieval mode. Instant, fast, and auto search responses may include neural search cost. Deep search modes may be reflected only in total.

### `AnswerResponse`. Source: https://exa.ai/docs/exa-spec.json.
- `AnswerResponse` object; required fields: `answer`.
  - `requestId` (optional): `string` — Unique identifier for the request.
  - `answer` (required): `oneOf(string | object)` — The generated answer based on search results. Returns a string by default, or a structured object matching the provided outputSchema.
  - `citations` (optional): `array<object>` — Search results used to generate the answer.
  - `costDollars` (optional): `CostDollarsOutput`
- `CostDollarsOutput` object; required fields: none.
  - `total` (optional): `number`; format="float" — Estimated total dollar cost for the completed request. This response value is not an invoice record.
  - `search` (optional): `object` — Endpoint-dependent estimated search cost breakdown by retrieval mode. Instant, fast, and auto search responses may include neural search cost. Deep search modes may be reflected only in total.

### `FindSimilarResponse`. Source: https://exa.ai/docs/exa-spec.json.
- `FindSimilarResponse` object; required fields: none.
  - `requestId` (optional): `string` — Unique identifier for the request.
  - `context` (optional): `string` — Deprecated. Combined context string from search results. Use highlights or text instead.
  - `results` (optional): `array<SearchResultOutput>` — A list of search results containing title, URL, published date, and author.
  - `costDollars` (optional): `CostDollarsOutput`
- `SearchResultOutput` object; required fields: `url`, `title`.
  - `title` (required): `string` — The title of the search result.
  - `url` (required): `string`; format="uri" — The URL of the search result.
  - `publishedDate` (optional): `string`; format="date-time" — An estimate of the creation date, from parsing HTML content. Format is YYYY-MM-DD.
  - `author` (optional): `anyOf(string | null)` — If available, the author of the content.
  - `id` (optional): `string` — The temporary ID for the document. Useful for the /contents endpoint.
  - `image` (optional): `string`; format="uri" — The URL of an image associated with the search result, if available.
  - `favicon` (optional): `string`; format="uri" — The URL of the favicon for the search result's domain.
  - `text` (optional): `string` — The full content text of the search result.
  - `highlights` (optional): `array<string>` — Array of highlights extracted from the search result content.
  - `highlightScores` (optional): `array<number>` — Array of cosine similarity scores for each highlighted snippet.
  - `summary` (optional): `string` — Summary of the webpage.
  - `subpages` (optional): `array<object>` — Array of subpages for the search result.
  - `entities` (optional): `array<oneOf(object | object | object)>` — Structured entity data for company, person, or publication search results. Returned for supported entity-backed categories.
  - `extras` (optional): `object` — Results from extras.
- `CostDollarsOutput` object; required fields: none.
  - `total` (optional): `number`; format="float" — Estimated total dollar cost for the completed request. This response value is not an invoice record.
  - `search` (optional): `object` — Endpoint-dependent estimated search cost breakdown by retrieval mode. Instant, fast, and auto search responses may include neural search cost. Deep search modes may be reflected only in total.

### `SearchResultOutput`. Source: https://exa.ai/docs/exa-spec.json.
- `SearchResultOutput` object; required fields: `url`, `title`.
  - `title` (required): `string` — The title of the search result.
  - `url` (required): `string`; format="uri" — The URL of the search result.
  - `publishedDate` (optional): `string`; format="date-time" — An estimate of the creation date, from parsing HTML content. Format is YYYY-MM-DD.
  - `author` (optional): `anyOf(string | null)` — If available, the author of the content.
  - `id` (optional): `string` — The temporary ID for the document. Useful for the /contents endpoint.
  - `image` (optional): `string`; format="uri" — The URL of an image associated with the search result, if available.
  - `favicon` (optional): `string`; format="uri" — The URL of the favicon for the search result's domain.
  - `text` (optional): `string` — The full content text of the search result.
  - `highlights` (optional): `array<string>` — Array of highlights extracted from the search result content.
  - `highlightScores` (optional): `array<number>` — Array of cosine similarity scores for each highlighted snippet.
  - `summary` (optional): `string` — Summary of the webpage.
  - `subpages` (optional): `array<object>` — Array of subpages for the search result.
  - `entities` (optional): `array<oneOf(object | object | object)>` — Structured entity data for company, person, or publication search results. Returned for supported entity-backed categories.
  - `extras` (optional): `object` — Results from extras.

### `CostDollarsOutput`. Source: https://exa.ai/docs/exa-spec.json.
- `CostDollarsOutput` object; required fields: none.
  - `total` (optional): `number`; format="float" — Estimated total dollar cost for the completed request. This response value is not an invoice record.
  - `search` (optional): `object` — Endpoint-dependent estimated search cost breakdown by retrieval mode. Instant, fast, and auto search responses may include neural search cost. Deep search modes may be reflected only in total.

### `AgentRunList`. Source: https://exa.ai/docs/exa-spec.json.
- `AgentRunList` object; required fields: `hasMore`, `data`, `object`, `nextCursor`.
  - `object` (required): `string`
  - `data` (required): `array<AgentRun>`
  - `hasMore` (required): `boolean` — Whether there are more results
  - `nextCursor` (required): `anyOf(AgentRunId | null)`
- `AgentRun` object; required fields: `createdAt`, `status`, `output`, `costDollars`, `request`, `object`, `id`, `usage`, `stopReason`, `completedAt`.
  - `id` (required): `AgentRunId`
  - `object` (required): `string`
  - `status` (required): `AgentRunStatus`
  - `stopReason` (required): `anyOf(AgentStopReason | null)` — Why the run stopped. `null` while the run is queued or running.
  - `createdAt` (required): `string`; format="date-time" — When the run was created
  - `completedAt` (required): `anyOf(string | null)`; format="date-time"
  - `request` (required): `anyOf(AgentRunRequest | null)`
  - `output` (required): `AgentRunOutput`
  - `usage` (required): `AgentUsage`
  - `costDollars` (required): `AgentCostDollars`
- `AgentRunId`: string minLen=1; maxLen=200

### `AgentRunEventList`. Source: https://exa.ai/docs/exa-spec.json.
- `AgentRunEventList` object; required fields: `hasMore`, `data`, `object`, `nextCursor`.
  - `object` (required): `string`
  - `data` (required): `array<AgentRunEvent>`
  - `hasMore` (required): `boolean` — Whether there are more results
  - `nextCursor` (required): `anyOf(string | null)`
- `AgentRunEvent` object; required fields: `event`, `data`, `createdAt`, `id`.
  - `id` (required): `string` — Event ID within the run.
  - `event` (required): `string`; enum=["agent_run.created", "agent_run.started", "agent_run.completed", "agent_run.failed", "agent_run.cancelled"]
  - `data` (required): `JsonValue`
  - `createdAt` (required): `string`; format="date-time" — When the event was created

### `ListSearchMonitorsResponse`. Source: https://exa.ai/docs/exa-spec.json.
- `ListSearchMonitorsResponse` object; required fields: `hasMore`, `data`.
  - `data` (required): `array<SearchMonitor>` — The list of monitors
  - `hasMore` (required): `boolean` — Whether there are more results
  - `nextCursor` (optional): `anyOf(string | null)` — Cursor for the next page
- `SearchMonitor` object; required fields: `updatedAt`, `webhook`, `createdAt`, `status`, `search`, `metadata`, `id`, `nextRunAt`, `trigger`, `outputSchema`, `name`.
  - `id` (required): `string` — The unique identifier for the monitor
  - `name` (required): `anyOf(string | null)` — An optional display name
  - `status` (required): `string`; enum=["active", "paused", "disabled"] — The status of the monitor. `active` monitors run on schedule and can be triggered manually. `paused` monitors can only be triggered manually. `disabled` monitors are auto-disabled after 10 consecutive authentication failures.
  - `search` (required): `SearchMonitorSearchOutput`
  - `trigger` (required): `anyOf(SearchMonitorTriggerOutput | null)` — The interval-based schedule for automatic runs. Null if no schedule is set.
  - `outputSchema` (required): `SearchMonitorOutputSchemaOutput`
  - `metadata` (required): `anyOf(object | null)` — Optional key-value metadata for your own tracking. Echoed back in webhook deliveries so you can route updates to systems like Slack.
  - `webhook` (required): `SearchMonitorWebhookOutput`
  - `nextRunAt` (required): `anyOf(string | null)`; format="date-time" — When the next scheduled run will occur. Null if no trigger is set.
  - `createdAt` (required): `string`; format="date-time" — When the monitor was created
  - `updatedAt` (required): `string`; format="date-time" — When the monitor was last updated

### `SearchMonitor`. Source: https://exa.ai/docs/exa-spec.json.
- `SearchMonitor` object; required fields: `updatedAt`, `webhook`, `createdAt`, `status`, `search`, `metadata`, `id`, `nextRunAt`, `trigger`, `outputSchema`, `name`.
  - `id` (required): `string` — The unique identifier for the monitor
  - `name` (required): `anyOf(string | null)` — An optional display name
  - `status` (required): `string`; enum=["active", "paused", "disabled"] — The status of the monitor. `active` monitors run on schedule and can be triggered manually. `paused` monitors can only be triggered manually. `disabled` monitors are auto-disabled after 10 consecutive authentication failures.
  - `search` (required): `SearchMonitorSearchOutput`
  - `trigger` (required): `anyOf(SearchMonitorTriggerOutput | null)` — The interval-based schedule for automatic runs. Null if no schedule is set.
  - `outputSchema` (required): `SearchMonitorOutputSchemaOutput`
  - `metadata` (required): `anyOf(object | null)` — Optional key-value metadata for your own tracking. Echoed back in webhook deliveries so you can route updates to systems like Slack.
  - `webhook` (required): `SearchMonitorWebhookOutput`
  - `nextRunAt` (required): `anyOf(string | null)`; format="date-time" — When the next scheduled run will occur. Null if no trigger is set.
  - `createdAt` (required): `string`; format="date-time" — When the monitor was created
  - `updatedAt` (required): `string`; format="date-time" — When the monitor was last updated
- `SearchMonitorSearchOutput` object; required fields: `query`.
  - `query` (required): `string`; minLen=1 — The query string for the search.
  - `numResults` (optional): `integer`; default=10; min=1; max=100 — Number of results to return. Limits vary by search type. The maximum public limit is 100 results. Contact sales (hello@exa.ai) to discuss higher limits.
  - `contents` (optional): `SearchMonitorContentsOutput`
- `SearchMonitorTriggerOutput` object; required fields: `period`, `type`.
  - `type` (required): `string`; default="interval" — The type of trigger. Currently only `interval` is supported.
  - `period` (required): `string` — A duration string specifying how often the monitor runs (e.g., "1h", "6h", "1d", "7d"). Single-unit only. Minimum interval is 1 hour. The schedule is anchored to the monitor's creation time (e.g., a daily monitor created at 2:30 PM runs daily around 2:30 PM).
- `SearchMonitorOutputSchemaOutput`: anyOf(oneOf(object | object) | null)
- `SearchMonitorWebhookOutput` object; required fields: `url`.
  - `url` (required): `string`; format="uri" — The HTTPS URL to receive webhook events. Must not point to localhost or private IP ranges.
  - `events` (optional): `array<string>` — Which events to subscribe to. Defaults to all events if not specified.

### `SearchMonitorRun`. Source: https://exa.ai/docs/exa-spec.json.
- `SearchMonitorRun` object; required fields: `cancelledAt`, `updatedAt`, `createdAt`, `status`, `output`, `failReason`, `id`, `monitorId`, `startedAt`, `completedAt`, `failedAt`, `durationMs`.
  - `id` (required): `string` — The unique identifier for the run
  - `monitorId` (required): `string` — The monitor this run belongs to
  - `status` (required): `string`; enum=["pending", "running", "completed", "failed", "cancelled"] — The status of the run
  - `output` (required): `anyOf(SearchMonitorRunOutput | null)` — The output of the run. Null until the run completes.
  - `failReason` (required): `anyOf(string | null)` — The reason the run failed. Null unless status is `failed`. `source_not_available` means the search requested a domain Exa cannot return (remove it from the search), and `forbidden` means the request was otherwise not permitted.
  - `startedAt` (required): `anyOf(string | null)`; format="date-time" — When the run started executing
  - `completedAt` (required): `anyOf(string | null)`; format="date-time" — When the run completed successfully
  - `failedAt` (required): `anyOf(string | null)`; format="date-time" — When the run failed
  - `cancelledAt` (required): `anyOf(string | null)`; format="date-time" — When the run was cancelled
  - `durationMs` (required): `anyOf(integer | null)` — Total execution time in milliseconds
  - `createdAt` (required): `string`; format="date-time" — When the run was created
  - `updatedAt` (required): `string`; format="date-time" — When the run was last updated
- `SearchMonitorRunOutput` object; required fields: none.
  - `results` (optional): `anyOf(array<object> | null)` — The search results
  - `content` (optional): `oneOf(JsonValue | null)` — Synthesized content from the search results. Shape depends on `outputSchema.type`.
  - `grounding` (optional): `anyOf(array<object> | null)` — Field-level citations with confidence levels

### `ListWebsetsResponse`. Source: https://exa.ai/docs/exa-spec.json.
- `ListWebsetsResponse` object; required fields: `hasMore`, `data`, `nextCursor`.
  - `data` (required): `array<Webset>` — The list of websets
  - `hasMore` (required): `boolean` — Whether there are more results to paginate through
  - `nextCursor` (required): `string`; nullable=True — The cursor to paginate through the next set of results
- `Webset` object; required fields: `updatedAt`, `title`, `dashboardUrl`, `createdAt`, `status`, `externalId`, `searches`, `enrichments`, `object`, `id`, `monitors`, `imports`.
  - `id` (required): `string` — The unique identifier for the webset
  - `object` (required): `string`; default="webset"
  - `status` (required): `string`; enum=["idle", "pending", "running", "paused"] — The status of the webset
  - `externalId` (required): `string`; nullable=True — The external identifier for the webset
  - `title` (required): `string`; nullable=True — The title of the webset
  - `searches` (required): `array<WebsetSearch>` — The searches that have been performed on the webset.
  - `imports` (required): `array<Import>` — Imports that have been performed on the webset.
  - `enrichments` (required): `array<WebsetEnrichment>` — The Enrichments to apply to the Webset Items.
  - `monitors` (required): `array<Monitor>` — The Monitors for the Webset.
  - `excludes` (optional): `array<object>` — The Excludes sources (existing imports or websets) that apply to all operations within this Webset. Any results found within these sources will be omitted across all search and import operations.
  - `metadata` (optional): `object`; default={} — Set of key-value pairs you want to associate with this object.
  - `dashboardUrl` (required): `string`; format="uri" — The URL to view the webset in the Exa dashboard
  - `createdAt` (required): `string`; format="date-time" — The date and time the webset was created
  - `updatedAt` (required): `string`; format="date-time" — The date and time the webset was updated

### `GetWebsetResponse`. Source: https://exa.ai/docs/exa-spec.json.
- `GetWebsetResponse`: allOf(Webset & object)

### `Webset`. Source: https://exa.ai/docs/exa-spec.json.
- `Webset` object; required fields: `updatedAt`, `title`, `dashboardUrl`, `createdAt`, `status`, `externalId`, `searches`, `enrichments`, `object`, `id`, `monitors`, `imports`.
  - `id` (required): `string` — The unique identifier for the webset
  - `object` (required): `string`; default="webset"
  - `status` (required): `string`; enum=["idle", "pending", "running", "paused"] — The status of the webset
  - `externalId` (required): `string`; nullable=True — The external identifier for the webset
  - `title` (required): `string`; nullable=True — The title of the webset
  - `searches` (required): `array<WebsetSearch>` — The searches that have been performed on the webset.
  - `imports` (required): `array<Import>` — Imports that have been performed on the webset.
  - `enrichments` (required): `array<WebsetEnrichment>` — The Enrichments to apply to the Webset Items.
  - `monitors` (required): `array<Monitor>` — The Monitors for the Webset.
  - `excludes` (optional): `array<object>` — The Excludes sources (existing imports or websets) that apply to all operations within this Webset. Any results found within these sources will be omitted across all search and import operations.
  - `metadata` (optional): `object`; default={} — Set of key-value pairs you want to associate with this object.
  - `dashboardUrl` (required): `string`; format="uri" — The URL to view the webset in the Exa dashboard
  - `createdAt` (required): `string`; format="date-time" — The date and time the webset was created
  - `updatedAt` (required): `string`; format="date-time" — The date and time the webset was updated
- `WebsetSearch` object; required fields: `maxPeoplePerCompany`, `updatedAt`, `progress`, `createdAt`, `websetId`, `status`, `count`, `exclude`, `canceledAt`, `object`, `id`, `canceledReason`, `query`, `scope`, `criteria`, `entity`, `recall`.
  - `id` (required): `string` — The unique identifier for the search
  - `object` (required): `string`; default="webset_search"
  - `status` (required): `string`; enum=["created", "pending", "running", "completed", "canceled"] — The status of the search
  - `websetId` (required): `string` — The unique identifier for the Webset this search belongs to
  - `query` (required): `string`; minLen=1; maxLen=5000 — The query used to create the search.
  - `entity` (required): `Entity`; nullable=True — The entity the search will return results for. When no entity is provided during creation, we will automatically select the best entity based on the query.
  - `criteria` (required): `array<object>` — The criteria the search will use to evaluate the results. If not provided, we will automatically generate them for you.
  - `count` (required): `number`; min=1 — The number of results the search will attempt to find. The actual number of results may be less than this number depending on the search complexity.
  - `maxPeoplePerCompany` (required): `integer`; min=1; nullable=True — The soft cap requested for matching people from the same current employer company, or null when no cap was requested.
  - `behavior` (optional): `WebsetSearchBehavior`; default="override" — The behavior of the search when it is added to a Webset. - `override`: the search will replace the existing Items found in the Webset and evaluate them against the new criteria. Any Items that don't match the new criteria will be discarded. - `append`: the search will add the new Items found to the existing Webset. Any Items that don't match the new criteria will be discarded.
  - `exclude` (required): `array<object>` — Sources (existing imports or websets) used to omit certain results to be found during the search.
  - `scope` (required): `array<object>` — The scope of the search. By default, there is no scope - thus searching the web. If provided during creation, the search will only be performed on the sources provided.
  - `progress` (required): `object` — The progress of the search
  - `recall` (required): `object`; nullable=True — Recall metrics for the search, null if not yet computed or requested.
  - `metadata` (optional): `object`; default={} — Set of key-value pairs you want to associate with this object.
  - `canceledAt` (required): `string`; format="date-time"; nullable=True — The date and time the search was canceled
  - `canceledReason` (required): `WebsetSearchCanceledReason`; nullable=True — The reason the search was canceled
  - `createdAt` (required): `string`; format="date-time" — The date and time the search was created
  - `updatedAt` (required): `string`; format="date-time" — The date and time the search was updated
- `Import` object; required fields: `failedMessage`, `updatedAt`, `title`, `createdAt`, `status`, `count`, `object`, `metadata`, `id`, `failedReason`, `format`, `failedAt`, `entity`.
  - `id` (required): `string` — The unique identifier for the Import
  - `object` (required): `string`; enum=["import"] — The type of object
  - `status` (required): `string`; enum=["pending", "processing", "completed", "failed", "canceled"] — The status of the Import
  - `format` (required): `string`; enum=["csv", "webset"] — The format of the import.
  - `entity` (required): `Entity`; nullable=True — The type of entity the import contains.
  - `title` (required): `string` — The title of the import
  - `count` (required): `number` — The number of entities in the import
  - `metadata` (required): `object` — Set of key-value pairs you want to associate with this object.
  - `failedReason` (required): `string`; enum=["invalid_format", "invalid_file_content", "missing_identifier"]; nullable=True — The reason the import failed
  - `failedAt` (required): `string`; format="date-time"; nullable=True — When the import failed
  - `failedMessage` (required): `string`; nullable=True — A human readable message of the import failure
  - `createdAt` (required): `string`; format="date-time" — When the import was created
  - `updatedAt` (required): `string`; format="date-time" — When the import was last updated
- `WebsetEnrichment` object; required fields: `updatedAt`, `title`, `instructions`, `createdAt`, `status`, `websetId`, `object`, `id`, `options`, `format`, `description`.
  - `id` (required): `string` — The unique identifier for the enrichment
  - `object` (required): `string`; default="webset_enrichment"
  - `status` (required): `string`; enum=["pending", "canceled", "completed"] — The status of the enrichment
  - `websetId` (required): `string` — The unique identifier for the Webset this enrichment belongs to.
  - `title` (required): `string`; nullable=True — The title of the enrichment. This will be automatically generated based on the description and format.
  - `description` (required): `string` — The description of the enrichment task provided during the creation of the enrichment.
  - `format` (required): `WebsetEnrichmentFormat`; nullable=True — The format of the enrichment response.
  - `options` (required): `array<object>`; nullable=True — When the format is options, the different options for the enrichment agent to choose from.
  - `instructions` (required): `string`; nullable=True — The instructions for the enrichment Agent. This will be automatically generated based on the description and format.
  - `metadata` (optional): `object`; default={} — The metadata of the enrichment
  - `createdAt` (required): `string`; format="date-time" — The date and time the enrichment was created
  - `updatedAt` (required): `string`; format="date-time" — The date and time the enrichment was updated
- `Monitor` object; required fields: `cadence`, `updatedAt`, `createdAt`, `status`, `websetId`, `object`, `metadata`, `id`, `lastRun`, `nextRunAt`, `behavior`.
  - `id` (required): `string` — The unique identifier for the Monitor
  - `object` (required): `string`; enum=["monitor"] — The type of object
  - `status` (required): `string`; enum=["enabled", "disabled"] — The status of the Monitor
  - `websetId` (required): `string` — The id of the Webset the Monitor belongs to
  - `cadence` (required): `object` — How often the monitor will run
  - `behavior` (required): `object` — Behavior to perform when monitor runs
  - `lastRun` (required): `MonitorRun`; nullable=True — The last run of the monitor
  - `nextRunAt` (required): `string`; format="date-time"; nullable=True — Date and time when the next run will occur in
  - `metadata` (required): `object` — Set of key-value pairs you want to associate with this object.
  - `createdAt` (required): `string`; format="date-time" — When the monitor was created
  - `updatedAt` (required): `string`; format="date-time" — When the monitor was last updated

### `WebsetItem`. Source: https://exa.ai/docs/exa-spec.json.
- `WebsetItem` object; required fields: `properties`, `updatedAt`, `evaluations`, `createdAt`, `websetId`, `enrichments`, `object`, `id`, `sourceId`, `source`.
  - `id` (required): `string` — The unique identifier for the Webset Item
  - `object` (required): `string`; default="webset_item"
  - `source` (required): `string`; enum=["search", "import"] — The source of the Item
  - `sourceId` (required): `string` — The unique identifier for the source
  - `sourceEntityId` (optional): `string` — The original identifier used to resolve this item (e.g., email, name, or URL). Only relevant when the source is import.
  - `websetId` (required): `string` — The unique identifier for the Webset this Item belongs to.
  - `properties` (required): `oneOf(WebsetItemPersonProperties | WebsetItemCompanyProperties | WebsetItemArticleProperties | WebsetItemResearchPaperProperties | WebsetItemCustomProperties)` — The properties of the Item
  - `evaluations` (required): `array<WebsetItemEvaluation>` — The criteria evaluations of the item
  - `enrichments` (required): `array<EnrichmentResult>`; nullable=True — The enrichments results of the Webset item
  - `createdAt` (required): `string`; format="date-time" — The date and time the item was created
  - `updatedAt` (required): `string`; format="date-time" — The date and time the item was last updated
- `WebsetItemCustomProperties` object; required fields: `content`, `url`, `type`, `custom`, `description`.
  - `type` (required): `string`; default="custom"
  - `url` (required): `string`; format="uri" — The URL of the Item
  - `description` (required): `string` — Short description of the Item
  - `content` (required): `string`; nullable=True — The text content of the Item
  - `custom` (required): `object`
- `WebsetItemResearchPaperProperties` object; required fields: `content`, `url`, `type`, `researchPaper`, `description`.
  - `type` (required): `string`; default="research_paper"
  - `url` (required): `string`; format="uri" — The URL of the research paper
  - `description` (required): `string` — Short description of the relevance of the research paper
  - `content` (required): `string`; nullable=True — The text content of the research paper
  - `researchPaper` (required): `object`
- `WebsetItemArticleProperties` object; required fields: `content`, `url`, `article`, `type`, `description`.
  - `type` (required): `string`; default="article"
  - `url` (required): `string`; format="uri" — The URL of the article
  - `description` (required): `string` — Short description of the relevance of the article
  - `content` (required): `string`; nullable=True — The text content for the article
  - `article` (required): `object`
- `WebsetItemCompanyProperties` object; required fields: `content`, `company`, `url`, `type`, `description`.
  - `type` (required): `string`; default="company"
  - `url` (required): `string`; format="uri" — The URL of the company website
  - `description` (required): `string` — Short description of the relevance of the company
  - `content` (required): `string`; nullable=True — The text content of the company website
  - `company` (required): `object`
- `WebsetItemPersonProperties` object; required fields: `person`, `url`, `type`, `description`.
  - `type` (required): `string`; default="person"
  - `url` (required): `string`; format="uri" — The URL of the person profile
  - `description` (required): `string` — Short description of the relevance of the person
  - `person` (required): `object`
- `WebsetItemEvaluation` object; required fields: `criterion`, `satisfied`, `reasoning`.
  - `criterion` (required): `string` — The description of the criterion
  - `reasoning` (required): `string` — The reasoning for the result of the evaluation
  - `satisfied` (required): `string`; enum=["yes", "no", "unclear"] — The satisfaction of the criterion
  - `references` (optional): `array<object>`; default=[] — The references used to generate the result.
- `EnrichmentResult` object; required fields: `result`, `status`, `object`, `format`, `references`, `enrichmentId`, `reasoning`.
  - `object` (required): `string`; default="enrichment_result"
  - `status` (required): `string`; enum=["pending", "completed", "canceled"] — The status of the enrichment result.
  - `format` (required): `WebsetEnrichmentFormat`
  - `result` (required): `array<string>`; nullable=True — The result of the enrichment.
  - `reasoning` (required): `string`; nullable=True — The reasoning for the result when an Agent is used.
  - `references` (required): `array<object>` — The references used to generate the result.
  - `enrichmentId` (required): `string` — The id of the Enrichment that generated the result

### `ListWebsetItemResponse`. Source: https://exa.ai/docs/exa-spec.json.
- `ListWebsetItemResponse` object; required fields: `hasMore`, `data`, `nextCursor`.
  - `data` (required): `array<WebsetItem>` — The list of webset items
  - `hasMore` (required): `boolean` — Whether there are more Items to paginate through
  - `nextCursor` (required): `string`; nullable=True — The cursor to paginate through the next set of Items
- `WebsetItem` object; required fields: `properties`, `updatedAt`, `evaluations`, `createdAt`, `websetId`, `enrichments`, `object`, `id`, `sourceId`, `source`.
  - `id` (required): `string` — The unique identifier for the Webset Item
  - `object` (required): `string`; default="webset_item"
  - `source` (required): `string`; enum=["search", "import"] — The source of the Item
  - `sourceId` (required): `string` — The unique identifier for the source
  - `sourceEntityId` (optional): `string` — The original identifier used to resolve this item (e.g., email, name, or URL). Only relevant when the source is import.
  - `websetId` (required): `string` — The unique identifier for the Webset this Item belongs to.
  - `properties` (required): `oneOf(WebsetItemPersonProperties | WebsetItemCompanyProperties | WebsetItemArticleProperties | WebsetItemResearchPaperProperties | WebsetItemCustomProperties)` — The properties of the Item
  - `evaluations` (required): `array<WebsetItemEvaluation>` — The criteria evaluations of the item
  - `enrichments` (required): `array<EnrichmentResult>`; nullable=True — The enrichments results of the Webset item
  - `createdAt` (required): `string`; format="date-time" — The date and time the item was created
  - `updatedAt` (required): `string`; format="date-time" — The date and time the item was last updated

### `WebsetEnrichment`. Source: https://exa.ai/docs/exa-spec.json.
- `WebsetEnrichment` object; required fields: `updatedAt`, `title`, `instructions`, `createdAt`, `status`, `websetId`, `object`, `id`, `options`, `format`, `description`.
  - `id` (required): `string` — The unique identifier for the enrichment
  - `object` (required): `string`; default="webset_enrichment"
  - `status` (required): `string`; enum=["pending", "canceled", "completed"] — The status of the enrichment
  - `websetId` (required): `string` — The unique identifier for the Webset this enrichment belongs to.
  - `title` (required): `string`; nullable=True — The title of the enrichment. This will be automatically generated based on the description and format.
  - `description` (required): `string` — The description of the enrichment task provided during the creation of the enrichment.
  - `format` (required): `WebsetEnrichmentFormat`; nullable=True — The format of the enrichment response.
  - `options` (required): `array<object>`; nullable=True — When the format is options, the different options for the enrichment agent to choose from.
  - `instructions` (required): `string`; nullable=True — The instructions for the enrichment Agent. This will be automatically generated based on the description and format.
  - `metadata` (optional): `object`; default={} — The metadata of the enrichment
  - `createdAt` (required): `string`; format="date-time" — The date and time the enrichment was created
  - `updatedAt` (required): `string`; format="date-time" — The date and time the enrichment was updated
- `WebsetEnrichmentFormat`: string enum=["text", "date", "number", "options", "email", "phone", "url"]

### `Webhook`. Source: https://exa.ai/docs/exa-spec.json.
- `Webhook` object; required fields: `updatedAt`, `secret`, `status`, `createdAt`, `url`, `object`, `id`, `events`.
  - `id` (required): `string` — The unique identifier for the webhook
  - `object` (required): `string`; default="webhook"
  - `status` (required): `string`; enum=["active", "inactive"] — The status of the webhook
  - `events` (required): `array<EventType>` — The events to trigger the webhook
  - `url` (required): `string`; format="uri" — The URL to send the webhook to
  - `secret` (required): `string`; nullable=True — The secret to verify the webhook signature. Only returned on Webhook creation.
  - `metadata` (optional): `object`; default={} — The metadata of the webhook
  - `createdAt` (required): `string`; format="date-time" — The date and time the webhook was created
  - `updatedAt` (required): `string`; format="date-time" — The date and time the webhook was last updated
- `EventType`: string enum=["webset.created", "webset.deleted", "webset.paused", "webset.idle", "webset.search.created", "webset.search.canceled", "webset.search.completed", "webset.search.updated", "import.created", "import.completed", "webset.item.created", "webset.item.enriched", "monitor.created", "monitor.updated", "monitor.deleted", "monitor.run.created", "monitor.run.completed", "webset.export.created", "webset.export.completed"]

### `Event`. Source: https://exa.ai/docs/exa-spec.json.
- `Event`: oneOf(object | object | object | object | object | object | object | object | object | object | object | object | object | object | object | object | object)

### `Monitor`. Source: https://exa.ai/docs/exa-spec.json.
- `Monitor` object; required fields: `cadence`, `updatedAt`, `createdAt`, `status`, `websetId`, `object`, `metadata`, `id`, `lastRun`, `nextRunAt`, `behavior`.
  - `id` (required): `string` — The unique identifier for the Monitor
  - `object` (required): `string`; enum=["monitor"] — The type of object
  - `status` (required): `string`; enum=["enabled", "disabled"] — The status of the Monitor
  - `websetId` (required): `string` — The id of the Webset the Monitor belongs to
  - `cadence` (required): `object` — How often the monitor will run
  - `behavior` (required): `object` — Behavior to perform when monitor runs
  - `lastRun` (required): `MonitorRun`; nullable=True — The last run of the monitor
  - `nextRunAt` (required): `string`; format="date-time"; nullable=True — Date and time when the next run will occur in
  - `metadata` (required): `object` — Set of key-value pairs you want to associate with this object.
  - `createdAt` (required): `string`; format="date-time" — When the monitor was created
  - `updatedAt` (required): `string`; format="date-time" — When the monitor was last updated
- `MonitorRun` object; required fields: `updatedAt`, `createdAt`, `status`, `canceledAt`, `object`, `failedReason`, `id`, `type`, `monitorId`, `completedAt`, `failedAt`.
  - `id` (required): `string` — The unique identifier for the Monitor Run
  - `object` (required): `string`; enum=["monitor_run"] — The type of object
  - `status` (required): `string`; enum=["created", "running", "completed", "canceled", "failed"] — The status of the Monitor Run
  - `monitorId` (required): `string` — The monitor that the run is associated with
  - `type` (required): `string`; enum=["search", "refresh"] — The type of the Monitor Run
  - `completedAt` (required): `string`; format="date-time"; nullable=True — When the run completed
  - `failedAt` (required): `string`; format="date-time"; nullable=True — When the run failed
  - `failedReason` (required): `string`; nullable=True — The reason the run failed
  - `canceledAt` (required): `string`; format="date-time"; nullable=True — When the run was canceled
  - `createdAt` (required): `string`; format="date-time" — When the run was created
  - `updatedAt` (required): `string`; format="date-time" — When the run was last updated

### `MonitorRun`. Source: https://exa.ai/docs/exa-spec.json.
- `MonitorRun` object; required fields: `updatedAt`, `createdAt`, `status`, `canceledAt`, `object`, `failedReason`, `id`, `type`, `monitorId`, `completedAt`, `failedAt`.
  - `id` (required): `string` — The unique identifier for the Monitor Run
  - `object` (required): `string`; enum=["monitor_run"] — The type of object
  - `status` (required): `string`; enum=["created", "running", "completed", "canceled", "failed"] — The status of the Monitor Run
  - `monitorId` (required): `string` — The monitor that the run is associated with
  - `type` (required): `string`; enum=["search", "refresh"] — The type of the Monitor Run
  - `completedAt` (required): `string`; format="date-time"; nullable=True — When the run completed
  - `failedAt` (required): `string`; format="date-time"; nullable=True — When the run failed
  - `failedReason` (required): `string`; nullable=True — The reason the run failed
  - `canceledAt` (required): `string`; format="date-time"; nullable=True — When the run was canceled
  - `createdAt` (required): `string`; format="date-time" — When the run was created
  - `updatedAt` (required): `string`; format="date-time" — When the run was last updated

### `Import`. Source: https://exa.ai/docs/exa-spec.json.
- `Import` object; required fields: `failedMessage`, `updatedAt`, `title`, `createdAt`, `status`, `count`, `object`, `metadata`, `id`, `failedReason`, `format`, `failedAt`, `entity`.
  - `id` (required): `string` — The unique identifier for the Import
  - `object` (required): `string`; enum=["import"] — The type of object
  - `status` (required): `string`; enum=["pending", "processing", "completed", "failed", "canceled"] — The status of the Import
  - `format` (required): `string`; enum=["csv", "webset"] — The format of the import.
  - `entity` (required): `Entity`; nullable=True — The type of entity the import contains.
  - `title` (required): `string` — The title of the import
  - `count` (required): `number` — The number of entities in the import
  - `metadata` (required): `object` — Set of key-value pairs you want to associate with this object.
  - `failedReason` (required): `string`; enum=["invalid_format", "invalid_file_content", "missing_identifier"]; nullable=True — The reason the import failed
  - `failedAt` (required): `string`; format="date-time"; nullable=True — When the import failed
  - `failedMessage` (required): `string`; nullable=True — A human readable message of the import failure
  - `createdAt` (required): `string`; format="date-time" — When the import was created
  - `updatedAt` (required): `string`; format="date-time" — When the import was last updated
- `Entity`: oneOf(CompanyEntity | PersonEntity | ArticleEntity | ResearchPaperEntity | CustomEntity)

### `WebsetsTeamInfo`. Source: https://exa.ai/docs/exa-spec.json.
- `WebsetsTeamInfo` object; required fields: `concurrency`, `object`, `id`, `limits`, `name`.
  - `object` (required): `string` — The object type, always `"team"`.
  - `id` (required): `string` — Unique identifier for the team.
  - `name` (required): `string` — Name of the team.
  - `concurrency` (required): `object` — Current concurrency usage.
  - `limits` (required): `object` — Concurrency limits for the team.

## Appendix B — Raw path list checksum

This is the complete path list from the docs OpenAPI JSON used for this report. Source: https://exa.ai/docs/exa-spec.json.
- `/search`
- `/contents`
- `/answer`
- `/findSimilar`
- `/monitors`
- `/monitors/batch`
- `/monitors/{id}`
- `/monitors/{id}/trigger`
- `/monitors/{id}/runs`
- `/monitors/{id}/runs/{runId}`
- `/agent/runs`
- `/agent/runs/{id}`
- `/agent/runs/{id}/cancel`
- `/agent/runs/{id}/events`
- `/v0/teams/me`
- `/research/v1`
- `/research/v1/{researchId}`
- `/v0/websets`
- `/v0/websets/{id}`
- `/v0/websets/{id}/cancel`
- `/v0/websets/preview`
- `/v0/websets/{webset}/items/{id}`
- `/v0/websets/{webset}/items`
- `/v0/websets/{webset}/enrichments`
- `/v0/websets/{webset}/enrichments/{id}`
- `/v0/websets/{webset}/enrichments/{id}/cancel`
- `/v0/webhooks`
- `/v0/webhooks/{id}`
- `/v0/webhooks/{id}/attempts`
- `/v0/events`
- `/v0/events/{id}`
- `/v0/websets/{webset}/searches`
- `/v0/websets/{webset}/searches/{id}`
- `/v0/websets/{webset}/searches/{id}/cancel`
- `/v0/monitors`
- `/v0/monitors/{id}`
- `/v0/monitors/{monitor}/runs`
- `/v0/monitors/{monitor}/runs/{id}`
- `/v0/imports`
- `/v0/imports/{id}`
