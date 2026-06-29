# Lane D: Exa auth, keys, accounts, pricing, limits, errors, retries, security

Research date: 2026-06-29. Sources used are current primary Exa docs/pages and official Exa SDK repos only.

## Executive takeaways for an agent-first CLI

- Authentication is API-key based. Send the key in `x-api-key`; Exa also documents `Authorization: Bearer <key>` as an alternative. Prefer `x-api-key` because every cURL/admin example uses it. Source: https://exa.ai/docs/reference/search, https://exa.ai/docs/reference/error-codes, ctx7 `/llmstxt/exa_ai_llms_txt` docs excerpt from Exa reference pages.
- Standard SDK env var is `EXA_API_KEY`. TypeScript docs show `new Exa()` reads `EXA_API_KEY`; exa-js README shows `new Exa(process.env.EXA_API_KEY)`; exa-py docs show `Exa()` reads `EXA_API_KEY` or accepts `api_key=`. Sources: https://exa.ai/docs/sdks/typescript-sdk-specification, https://github.com/exa-labs/exa-js/blob/master/README.md, https://github.com/exa-labs/exa-py/blob/master/docs/python-sdk-specification.mdx.
- Default documented API limits: `/search` 10 QPS, `/contents` 100 QPS, `/answer` 10 QPS, legacy `/research/v1` 15 concurrent tasks. Agent concurrency is documented separately as one fifth of account QPS; default pay-as-you-go means two active Agent runs. Sources: https://exa.ai/docs/reference/rate-limits, https://exa.ai/docs/reference/agent-api-guide.
- Errors are mostly JSON with `requestId`, `error`, and `tag`, but 429 rate-limit errors use only `{ "error": ... }`; the CLI must handle both shapes. Source: https://exa.ai/docs/reference/error-codes.
- Pricing is usage-sensitive and endpoint/parameter sensitive; show `costDollars` when Exa returns it and support preflight cost warnings for Agent effort/contact enrichment. Sources: https://exa.ai/pricing, https://exa.ai/docs/reference/agent-api-guide, https://exa.ai/docs/reference/search.

## Authentication and key handling

### API auth headers

- Search examples call `https://api.exa.ai/search` with `x-api-key: YOUR-EXA-API-KEY` and `Content-Type: application/json`. Source: https://exa.ai/docs/reference/search.
- Exa's documented security scheme accepts `x-api-key` and, alternatively, `Authorization: Bearer <key>`. Source: ctx7 `/llmstxt/exa_ai_llms_txt` docs excerpt from Exa reference pages.
- Admin/team-management endpoints are on `https://admin-api.exa.ai/team-management/...` and require `x-api-key: YOUR-SERVICE-KEY`. Sources: https://exa.ai/docs/reference/team-management/create-api-key, https://exa.ai/docs/reference/team-management/list-api-keys.

CLI implication:

1. Resolve credentials in this order: explicit `--api-key` only for one-shot CI use, `EXA_API_KEY`, then OS keyring/configured profile. Never persist a key supplied by `--api-key` unless the user explicitly runs a login/store command.
2. Redact all secrets in diagnostics as `exa_...last4` or `...last4`; never echo the full key in JSON, logs, shell snippets, or error reports.
3. Use `x-api-key` by default for first-party API requests. If a user asks for bearer compatibility, keep it as an advanced transport option, not the default.

### Official SDK env var behavior

- Python: official exa-py docs show `from exa_py import Exa; exa = Exa()  # Reads EXA_API_KEY from environment`, or explicit `Exa(api_key="your-api-key")`. Source: https://github.com/exa-labs/exa-py/blob/master/docs/python-sdk-specification.mdx.
- TypeScript: official TypeScript SDK docs show `const exa = new Exa(); // Reads EXA_API_KEY from environment`, or explicit `new Exa("your-api-key")`. Source: https://exa.ai/docs/sdks/typescript-sdk-specification.
- exa-js README quick start uses `new Exa(process.env.EXA_API_KEY)`. Source: https://github.com/exa-labs/exa-js/blob/master/README.md.
- Agent quickstart instructs setting `EXA_API_KEY` on macOS/Linux with `export EXA_API_KEY="your-api-key"` and on Windows with `setx EXA_API_KEY "your-api-key"`. Source: https://exa.ai/docs/reference/agent-api-guide.

CLI implication: `exa-agent-cli doctor --json` should report whether `EXA_API_KEY` is present without printing it, and whether a stored credential exists. If both are present, default to env var and warn on stderr that env overrides stored profile.

## Accounts, API-key lifecycle, budgets, usage

- Exa has team-management endpoints to create, list, get, update, delete, and query usage for API keys. Sources: https://exa.ai/docs/reference/team-management/create-api-key, https://exa.ai/docs/reference/team-management/list-api-keys, https://exa.ai/docs/reference/team-management/get-api-key, https://exa.ai/docs/reference/team-management/update-api-key, https://exa.ai/docs/reference/team-management/delete-api-key, https://exa.ai/docs/reference/team-management/get-api-key-usage.
- Create/update key bodies include optional `name`, `rateLimit`, and `budgetCents`; responses include metadata such as `id`, `name`, `rateLimit`, `budgetCents`, `isOverBudget`, `teamId`, `userId`, and timestamps. Sources: https://exa.ai/docs/reference/team-management/create-api-key, https://exa.ai/docs/reference/team-management/update-api-key.
- List/get key metadata includes key IDs, names, rate limits, budgets, team IDs, and created timestamps, not the raw secret in shown responses. Sources: https://exa.ai/docs/reference/team-management/list-api-keys, https://exa.ai/docs/reference/team-management/get-api-key.
- Usage endpoint returns authoritative billing data per API key, including `total_cost_usd` and `cost_breakdown`; default period is last 30 days, and lookback is limited to 6 months/180 days. Source: https://exa.ai/docs/reference/team-management/get-api-key-usage.
- Delete API Key permanently removes a key and returns `{ "success": true }` on success. Source: https://exa.ai/docs/reference/team-management/delete-api-key.

CLI implication:

- Treat a "service key" for team-management as higher privilege than a normal query key. Store it under a separate credential name/scope, e.g. `exa:service:<team>` vs `exa:api:<profile>`.
- Support named profiles because API keys can be team/project/budget specific.
- Do not assume the admin API returns the raw newly-created secret; the shown Create response only includes key metadata. If runtime proves a secret is returned once, display it once and require explicit `--store` to persist.
- Provide budget-aware helpers: `keys list --json`, `keys usage <id> --json`, `keys revoke <id> --confirm <id>`.

## Pricing and billing units

### Account billing

- Exa uses pay-as-you-go credits; requests are blocked when the credit balance runs out until credits are added or auto recharge is enabled. Source: https://exa.ai/docs/reference/billing.
- New accounts receive $10 in free credits after onboarding; accounts with payment method on file receive $7 free credits at the start of each calendar month, expiring at month end. Source: https://exa.ai/docs/reference/billing.
- Having a higher balance does not increase rate limits; high-volume users should contact sales if they expect to exceed defaults. Source: https://exa.ai/docs/reference/billing.

### Endpoint pricing

Pricing page values as of 2026-06-29:

| Product / endpoint | Documented price | Notes | Source |
|---|---:|---|---|
| Free tier | up to 20,000 requests/month | Free monthly request allotment | https://exa.ai/pricing |
| Search | $7 / 1k requests | Base price with up to 10 results | https://exa.ai/pricing |
| Deep Search | $12 / 1k requests | Base price with up to 10 results | https://exa.ai/pricing |
| Deep-Reasoning Search | $15 / 1k requests | Base price with up to 10 results | https://exa.ai/pricing |
| Contents | $1 / 1k pages per content type | Page/content-type based | https://exa.ai/pricing |
| Monitors | $15 / 1k requests | Monitor searches at cadence | https://exa.ai/pricing |
| Answer | $5 / 1k requests | Base price with up to 10 results | https://exa.ai/pricing |
| Additional result above 10 | $1 / 1k requests | Applies to Search, Deep Search, Deep-Reasoning Search, Monitors, Answer; pricing table shows dash for Contents | https://exa.ai/pricing |
| AI page summaries | $1 / 1k pages | Applies across listed endpoints | https://exa.ai/pricing |

### Agent pricing

- Agent is async/high-compute/usage-based and returns `costDollars` in completed runs. Sources: https://exa.ai/docs/reference/agent-api-guide, https://exa.ai/docs/reference/exa-mcp.
- Agent usage components: Agent Compute Units at `$0.10 / ACU`, search tool calls at `$0.005 / search`, email contact enrichment at `$0.02 / email`, and phone contact enrichment at `$0.07 / phone number`. Sources: https://exa.ai/docs/reference/agent-api-guide, https://exa.ai/pricing.
- Fixed effort mode prices: `minimal` `$0.012/request`, `low` `$0.025/request`, `medium` `$0.10/request`, `high` `$0.50/request`, `xhigh` `$1.00/request`; default effort is `auto`. Sources: https://exa.ai/docs/reference/agent-api-guide, https://exa.ai/pricing.
- The public pricing page headline says Agent is `$0.012-$2.00/run`, while the Agent docs' fixed effort table tops out at `$1.00/request` for `xhigh` and separately bills contact enrichment. Treat pricing as docs-page/version-sensitive and display Exa-returned `costDollars` as authoritative after a run. Sources: https://exa.ai/pricing, https://exa.ai/docs/reference/agent-api-guide.

CLI implication:

- Add `--max-cost-usd` for Agent runs if feasible; otherwise, at minimum warn when `effort=auto`, `dataSources`, contact fields, or large `input.data` make cost variable.
- Encourage bounded schemas: Exa docs explicitly say to bound list sizes with `maxItems` where possible so maximum contact-enrichment cost is predictable. Source: https://exa.ai/docs/reference/agent-api-guide.
- Always print `costDollars` in JSON output and summarize it in human output after successful runs when present. Source: https://exa.ai/docs/reference/search, https://exa.ai/docs/reference/agent-api-guide.

## Rate limits and concurrency

- Default documented rate limits: `/search` 10 QPS, `/contents` 100 QPS, `/answer` 10 QPS, `/research/v1` legacy/deprecated 15 concurrent tasks. Source: https://exa.ai/docs/reference/rate-limits.
- Agent concurrency is one fifth of account QPS; for pay-as-you-go accounts with default QPS, this means two active Agent runs at a time. Source: https://exa.ai/docs/reference/agent-api-guide.
- API-key management docs expose per-key `rateLimit`, but currently conflict on unit wording: prose says requests per minute in places while schema/body rows say requests per second. Sources: https://exa.ai/docs/reference/team-management/create-api-key, https://exa.ai/docs/reference/team-management/update-api-key, https://exa.ai/docs/reference/team-management/list-api-keys.

CLI implication:

- Do not hard-code admin `rateLimit` units into UX without runtime verification. Label it as "rateLimit (as returned by Exa)" in JSON and, in human UI, say "unit per Exa API docs; currently inconsistent between minute/second in docs".
- Implement a default client-side limiter keyed by endpoint: 8 QPS `/search`, 80 QPS `/contents`, 8 QPS `/answer`, and Agent active-run limit 2 unless the user configures a known higher quota. This is an inference from documented defaults, not an Exa requirement.
- On 429, back off exponentially with jitter; Exa explicitly recommends exponential backoff and reducing request rate for 429. Source: https://exa.ai/docs/reference/error-codes.

## Error model and retry guidance

### Request-level HTTP statuses

Exa common API errors and documented handling:

| Status | Meaning / cause | Documented action | Source |
|---:|---|---|---|
| 400 | Invalid request params, malformed JSON, missing required fields | Check body/params/API key formatting | https://exa.ai/docs/reference/error-codes |
| 401 | Missing/invalid API key | Verify key is active and auth headers are correct | https://exa.ai/docs/reference/error-codes |
| 402 | Credits exhausted or API key/team budget exceeded | Top up credits or increase API-key/team budget | https://exa.ai/docs/reference/error-codes |
| 403 | Insufficient permissions, feature disabled, blocked content/policy | Check plan/permissions/content access | https://exa.ai/docs/reference/error-codes |
| 404 | Resource not found | Verify resource ID exists/access is allowed | https://exa.ai/docs/reference/error-codes |
| 409 | Resource conflict, e.g. duplicate Webset `externalId` | Use another identifier or update existing resource | https://exa.ai/docs/reference/error-codes |
| 422 | Well-formed but unprocessable request | Check details; verify URLs or rephrase query | https://exa.ai/docs/reference/error-codes |
| 429 | Rate limit exceeded | Exponential backoff and reduce rate | https://exa.ai/docs/reference/error-codes |
| 500 | Internal server error | Retry after brief wait; contact if persistent | https://exa.ai/docs/reference/error-codes |
| 501 | `/answer` only: model unable to generate answer | Rephrase/adjust params | https://exa.ai/docs/reference/error-codes |
| 502 | Upstream server issue | Retry after brief delay | https://exa.ai/docs/reference/error-codes |
| 503 | Service unavailable | Retry after delay/check maintenance | https://exa.ai/docs/reference/error-codes |

### Error shapes

- General error shape: `{ "requestId": "...", "error": "...", "tag": "..." }`. Include `requestId` when contacting support. Source: https://exa.ai/docs/reference/error-codes.
- 429 shape is simpler: `{ "error": "You've exceeded your Exa rate limit ..." }`, with no guaranteed `requestId` or `tag` in the documented example. Source: https://exa.ai/docs/reference/error-codes.
- Search/TypeScript SDK responses include `requestId`, `statuses`, and sometimes `costDollars`. Sources: https://exa.ai/docs/sdks/typescript-sdk-specification, https://exa.ai/docs/reference/search.

### Tags

Notable retry/security tags:

- Auth/billing: `INVALID_API_KEY` 401, `NO_MORE_CREDITS` 402, `API_KEY_BUDGET_EXCEEDED` 402, `TEAM_BUDGET_EXCEEDED` 402, `ACCESS_DENIED` 403, `FEATURE_DISABLED` 403. Source: https://exa.ai/docs/reference/error-codes.
- Validation: `INVALID_REQUEST_BODY`, `INVALID_REQUEST`, `INVALID_URLS`, `INVALID_NUM_RESULTS`, `INVALID_FLAGS`, `INVALID_JSON_SCHEMA`, `NUM_RESULTS_EXCEEDED`, `NO_CONTENT_FOUND`. Source: https://exa.ai/docs/reference/error-codes.
- Server/retryable-ish: `DEFAULT_ERROR` 500 and `INTERNAL_ERROR` 500 both say retry after a brief wait. Source: https://exa.ai/docs/reference/error-codes.

### Contents per-URL statuses

- `/contents` can return HTTP 200 while individual URLs fail inside `statuses`; per-URL tags include `CRAWL_NOT_FOUND` 404, `CRAWL_TIMEOUT` 504, `CRAWL_LIVECRAWL_TIMEOUT` 504, `SOURCE_NOT_AVAILABLE` 403, `UNSUPPORTED_URL`, and `CRAWL_UNKNOWN_ERROR` 500+. Source: https://exa.ai/docs/reference/error-codes, https://exa.ai/docs/reference/contents-api-guide-for-coding-agents.

CLI implication:

- Parse both general and 429 error shapes.
- Classify retry decisions conservatively:
  - Never retry 400/401/402/403/404/409/422 automatically, except user-triggered correction flows.
  - Retry 429/500/502/503 and content `CRAWL_TIMEOUT`, `CRAWL_LIVECRAWL_TIMEOUT`, `CRAWL_UNKNOWN_ERROR` with capped exponential backoff + jitter.
  - For `/contents`, return partial success as success-with-statuses, not total failure, when HTTP status is 200.
- Surface `requestId` whenever present in stderr and JSON; never discard it.

## Idempotency and async Agent concerns

Documented facts:

- Agent runs are async; create returns an `agent_run_...` ID, streaming emits queued/running/completed events, and non-streaming clients poll by run ID until terminal status. Sources: https://exa.ai/docs/reference/agent-api-guide, https://exa.ai/docs/reference/exa-mcp.
- Completed Agent runs include output text/structured JSON/grounding and `costDollars`. Source: https://exa.ai/docs/reference/agent-api-guide.
- `previousRunId` continues/follows up from a previous run and follow-up runs share the same run ID as the supplied previous run. Source: https://exa.ai/docs/reference/agent-api-guide.
- Exa MCP exposes cancel, wait, get-output, and create-run tools for Agent. Source: https://exa.ai/docs/reference/exa-mcp.

Unresolved in docs:

- I did not find a documented idempotency-key header for create-run/search/contents/answer.
- I did not find a documented `Retry-After` header contract for 429.

CLI implication:

- Do not blindly retry Agent create-run after transport ambiguity; it may create duplicate billable runs. Instead, persist local pending-run records keyed by a deterministic hash of request body + profile + timestamp bucket and ask the user whether to resume/list recent runs or submit a new run.
- For polling/get-output/cancel, retries are safer because they operate on an existing run ID.
- For search/answer/contents, retries can duplicate billing but are usually acceptable for transient 429/5xx if the CLI honors `--max-retries`, `--retry-budget-usd`, or user-configured policy.

## Privacy, data handling, and security notes

- Exa states it is SOC 2 Type II certified and directs Enterprise users to Trust Center documentation; Zero Data Retention and HIPAA compliance are Enterprise/custom security options. Source: https://exa.ai/docs/reference/security.
- Exa Agent docs explicitly say: "Exa Agent is not ZDR. If you require ZDR, reach out to us." Source: https://exa.ai/docs/reference/agent-api-guide.
- Pricing page lists Zero Data Retention under Enterprise. Source: https://exa.ai/pricing.
- HIPAA/compliance mode appears in Search API guide as Enterprise-only `compliance: "hipaa"`; HIPAA search requests fail closed if the resolved path requires non-HIPAA-safe processing. Source: https://exa.ai/docs/reference/search-api-guide-for-coding-agents.
- Regional restrictions can cause Cloudflare WAF block pages instead of Exa JSON errors for sanctioned/restricted regions. Source: https://exa.ai/docs/reference/security.
- Security page says changing a password may not invalidate all auth tokens and API keys, in an out-of-scope examples list. Source: https://exa.ai/security.

CLI implication:

- Treat prompts, `input.data`, `outputSchema`, and fetched URLs as data sent to Exa; warn users before sending local files or sensitive row data.
- Include `--zdr-required` / config policy if the CLI will be used in sensitive environments; if set and endpoint is Agent, fail closed unless the configured account is known Enterprise/ZDR.
- For HIPAA use, expose a `--compliance hipaa` pass-through only where the endpoint supports it and fail clearly when unsupported.
- Never store Exa outputs or input payloads in world-readable cache files; use `0600` files and honor `--no-cache`.

## CLI credential storage implications

Recommended minimal design, combining Exa docs with the local SaaS CLI auth-flow skill security invariants:

1. Env var first for CI and ephemeral agent sessions: `EXA_API_KEY`.
2. OS keyring for interactive login/store: service `exa-agent-cli`, account `<profile>`.
3. Config file stores metadata only: active profile, key fingerprint/last4, team/key ID if known, created timestamp, and endpoint/base URL. Do not store raw key in plaintext config.
4. If no keyring is available, either fail with a clear message or require `--allow-plaintext-credentials` plus `0600` permissions. Prefer failing because Exa keys are bearer credentials.
5. Separate normal API key and service/admin key storage scopes.
6. `logout` should delete local credentials; `keys delete <id>` should call Exa admin delete endpoint and require explicit confirmation because it permanently removes a team key.

Agent-ergonomic surfaces to build:

- `exa-agent auth status --json`: `{ authenticated, source: "env|keyring|none", profile, key_fingerprint, can_admin, warnings[] }`.
- `exa-agent auth login --api-key-stdin`: reads from stdin, stores in keyring, never echoes.
- `exa-agent doctor --json`: verifies auth by a cheap authenticated endpoint when possible, reports 401/402/429 distinctly.
- `exa-agent limits --json`: static defaults + any observed/admin per-key metadata.
- `exa-agent cost explain --endpoint agent --effort medium --json`: deterministic pricing explanation using the current pricing table plus caveat that returned `costDollars` is authoritative.

## Open questions requiring runtime validation

- Whether the API returns `Retry-After` or other rate-limit headers on 429. Not documented in the sources above.
- Exact semantics/units of team-management `rateLimit`; docs conflict between requests per minute and requests per second.
- Whether Create API Key returns the raw secret in real responses; displayed docs only show metadata.
- Whether Exa SDKs have built-in retry policies; primary docs reviewed here document API retry guidance, not SDK retry internals.
- Whether a cheap account/profile endpoint exists for auth validation without billable search; team-management list/get may require a service key, not normal query key.

## Source index

- Exa Search reference: https://exa.ai/docs/reference/search
- Exa Agent guide: https://exa.ai/docs/reference/agent-api-guide
- Exa MCP guide: https://exa.ai/docs/reference/exa-mcp
- Exa Rate Limits: https://exa.ai/docs/reference/rate-limits
- Exa Error Codes: https://exa.ai/docs/reference/error-codes
- Exa Billing: https://exa.ai/docs/reference/billing
- Exa Pricing: https://exa.ai/pricing
- Exa Security docs: https://exa.ai/docs/reference/security
- Exa Vulnerability Disclosure/Security page: https://exa.ai/security
- Team Management Create API Key: https://exa.ai/docs/reference/team-management/create-api-key
- Team Management List API Keys: https://exa.ai/docs/reference/team-management/list-api-keys
- Team Management Get API Key: https://exa.ai/docs/reference/team-management/get-api-key
- Team Management Get API Key Usage: https://exa.ai/docs/reference/team-management/get-api-key-usage
- Team Management Update API Key: https://exa.ai/docs/reference/team-management/update-api-key
- Team Management Delete API Key: https://exa.ai/docs/reference/team-management/delete-api-key
- Python SDK spec: https://exa.ai/docs/sdks/python-sdk-specification and https://github.com/exa-labs/exa-py/blob/master/docs/python-sdk-specification.mdx
- TypeScript SDK spec: https://exa.ai/docs/sdks/typescript-sdk-specification
- exa-js README: https://github.com/exa-labs/exa-js/blob/master/README.md
