> ## Documentation Index
> Fetch the complete documentation index at: https://exa.ai/docs/llms.txt
> Use this file to discover all available pages before exploring further.

# Websets Reference (For Your Coding Agent)

> Self-contained reference for coding agents. Websets API architecture, request/response shapes, event flow, and integration patterns.

## Overview

**Base URL:** `https://api.exa.ai/websets/v0`

**Auth:** Pass your API key via the `x-api-key` header. Get one at [https://dashboard.exa.ai/api-keys](https://dashboard.exa.ai/api-keys)

Websets is an asynchronous search system. You define a query, criteria for verification, and optional enrichments. The system searches, verifies each result, and returns structured items over time. Results are available via polling or webhooks.

## Installation

```bash theme={null}
pip install exa-py    # Python
npm install exa-js    # JavaScript
```

## Minimal Working Example

```python theme={null}
from exa_py import Exa
from exa_py.websets.types import CreateWebsetParameters, CreateEnrichmentParameters
import os

exa = Exa(api_key=os.getenv("EXA_API_KEY"))

webset = exa.websets.create(
    params=CreateWebsetParameters(
        search={
            "query": "Top AI research labs focusing on large language models",
            "count": 5
        },
        enrichments=[
            CreateEnrichmentParameters(
                description="LinkedIn profile of VP of Engineering",
                format="text",
            ),
        ],
    )
)
print(f"Webset ID: {webset.id}")
print(f"Dashboard: {webset.dashboard_url}")

# Wait for completion
webset = exa.websets.wait_until_idle(webset.id)

# Get results
items = exa.websets.items.list(webset_id=webset.id)
for item in items.data:
    print(item.model_dump_json(indent=2))
```

```javascript theme={null}
import Exa from "exa-js";
const exa = new Exa(process.env.EXA_API_KEY);

const webset = await exa.websets.create({
  search: {
    query: "Top AI research labs focusing on large language models",
    count: 10
  },
  enrichments: [
    { description: "Estimate founding year", format: "number" }
  ],
});
console.log(`Webset ID: ${webset.id}`);

const idleWebset = await exa.websets.waitUntilIdle(webset.id, {
  timeout: 60000,
  pollInterval: 2000
});

const items = await exa.websets.items.list(webset.id, { limit: 10 });
for (const item of items.data) {
  console.log(JSON.stringify(item, null, 2));
}
```

```bash theme={null}
# Create a webset
curl -s -X POST "https://api.exa.ai/websets/v0/websets/" \
  -H "accept: application/json" \
  -H "content-type: application/json" \
  -H "x-api-key: ${EXA_API_KEY}" \
  -d '{
    "search": {
      "query": "Top AI research labs focusing on large language models",
      "count": 5
    },
    "enrichments": [
      {"description": "Find founding year", "format": "number"}
    ]
  }'

# Check status
curl "https://api.exa.ai/websets/v0/websets/{WEBSET_ID}" \
  -H "accept: application/json" \
  -H "x-api-key: ${EXA_API_KEY}"

# List items
curl "https://api.exa.ai/websets/v0/websets/{WEBSET_ID}/items" \
  -H "accept: application/json" \
  -H "x-api-key: ${EXA_API_KEY}"

# Get webset with items expanded
curl "https://api.exa.ai/websets/v0/websets/{WEBSET_ID}?expand=items" \
  -H "accept: application/json" \
  -H "x-api-key: ${EXA_API_KEY}"
```

## SDK Sub-Client Reference

The SDKs provide sub-clients for all API resources. Here are the key operations beyond the minimal example above.

**Python SDK note:** All response attributes use `snake_case`. JSON field `hasMore` → `has_more`, `nextCursor` → `next_cursor`, `createdAt` → `created_at`, `externalId` → `external_id`, `websetId` → `webset_id`.

```python theme={null}
# Add a search to an existing webset
search = exa.websets.searches.create(
    webset_id=webset.id,
    params={"query": "AI companies in Asia", "count": 25}
)

# Add an enrichment
enrichment = exa.websets.enrichments.create(
    webset_id=webset.id,
    params=CreateEnrichmentParameters(description="CEO email", format="email")
)

# Create a webhook (secret is only in this response — store it immediately)
webhook = exa.websets.webhooks.create(
    params={"url": "https://your-server.com/hook", "events": ["webset.idle"]}
)
print(webhook.secret)  # Only returned once!

# Create a monitor (weekly search on Mondays at 9am ET)
monitor = exa.websets.monitors.create(params={
    "websetId": webset.id,
    "cadence": {"cron": "0 9 * * 1", "timezone": "America/New_York"},
    "behavior": {"type": "search", "config": {"parameters": {"query": "...", "count": 10}}}
})

# Import your own URLs
import_obj = exa.websets.imports.create(
    params={"websetId": webset.id, "urls": ["https://example.com/a", "https://example.com/b"]}
)

# Get a webset with items embedded (up to 100)
webset = exa.websets.get(webset.id, expand=["items"])

# Access the initial search created with the webset (no separate list call needed)
first_search = webset.searches[0]
print(first_search.id, first_search.progress.completion)  # 0-100%

# Map enrichment IDs to descriptions (enrichment results only have enrichmentId, not description)
desc_map = {e.id: e.description for e in webset.enrichments}

# Paginate through all items
cursor = None
while True:
    page = exa.websets.items.list(webset_id=webset.id, cursor=cursor)
    for item in page.data:
        print(item.properties.url)                  # URL is nested under properties
        print(item.properties.company.name)          # Entity fields nested under properties
        for enr in item.enrichments:
            print(desc_map[enr.enrichment_id], enr.result)  # Resolve ID → description
    if not page.has_more:                            # snake_case, not hasMore
        break
    cursor = page.next_cursor                        # snake_case, not nextCursor
```

```javascript theme={null}
// Add a search to an existing webset
const search = await exa.websets.searches.create(webset.id, {
  query: "AI companies in Asia", count: 25
});

// Add an enrichment
const enrichment = await exa.websets.enrichments.create(webset.id, {
  description: "CEO email", format: "email"
});

// Create a webhook
const webhook = await exa.websets.webhooks.create({
  url: "https://your-server.com/hook", events: ["webset.idle"]
});
console.log(webhook.secret); // Only returned once!

// Create a monitor (weekly search on Mondays at 9am ET)
const monitor = await exa.websets.monitors.create({
  websetId: webset.id,
  cadence: { cron: "0 9 * * 1", timezone: "America/New_York" },
  behavior: { type: "search", config: { parameters: { query: "...", count: 10 } } }
});

// Import your own URLs
const importObj = await exa.websets.imports.create({
  websetId: webset.id, urls: ["https://example.com/a", "https://example.com/b"]
});

// Paginate through items
let cursor;
do {
  const page = await exa.websets.items.list(webset.id, { limit: 25, cursor });
  for (const item of page.data) {
    console.log(item.properties.url);               // URL nested under properties
    console.log(item.properties.company?.name);      // Entity fields nested under properties
    for (const enr of item.enrichments ?? []) {
      console.log(enr.result);                       // Always string[] or null
    }
  }
  cursor = page.hasMore ? page.nextCursor : undefined;
} while (cursor);
```

***

## How Websets Work

### Lifecycle

1. **Create** — You POST a search config (query, count, optional criteria/enrichments/entity type). A webset is created with status `running`.
2. **Search** — The system searches and verifies each result against your criteria. Matching items are added to the webset. Each item triggers a `webset.item.created` event.
3. **Enrichment** — If enrichments are configured, each item is processed. `webset.item.enriched` events fire as enrichment results arrive.
4. **Idle** — When all searches and enrichments complete, the webset status becomes `idle` and a `webset.idle` event fires.

### Key Concepts

* **Search**: Defines what to look for (query + count). Multiple searches can be added to one webset.
* **Criteria**: Verification rules. Each result is checked against criteria before becoming an item. Max 5 criteria per search.
* **Entity**: Optional type hint (e.g. `"company"`, `"person"`, `"article"`, `"research_paper"`, `"custom"`) that shapes how results are found and verified. Auto-detected if not specified.
* **Enrichments**: Additional data extraction applied to each item (e.g. "Find the CEO name"). Max 10 per webset.
* **Monitors**: Scheduled re-runs that keep websets updated. Supports cron expressions.
* **Webhooks**: Real-time HTTP callbacks for events.
* **Imports**: Bring your own URLs and run enrichments on them.
* **Exports**: Bulk download of webset items as CSV/JSON.

***

## API Endpoints — Full Reference

### Websets

#### POST `/websets/` — Create a Webset

**Request body:**

```json theme={null}
{
  "search": {
    "query": "AI companies in Europe that raised Series A funding",
    "count": 50,
    "criteria": [
      {"description": "Company is an AI startup"},
      {"description": "Company has raised Series A funding"}
    ],
    "entity": {"type": "company"},
    "behaviour": "override"
  },
  "enrichments": [
    {"description": "Find the CEO name", "format": "text"},
    {"description": "Estimate founding year", "format": "number"},
    {"description": "Industry vertical", "format": "options", "options": [{"label": "Healthcare"}, {"label": "Finance"}, {"label": "Education"}, {"label": "Other"}]}
  ],
  "externalId": "my-unique-id",
  "metadata": {"project": "market-research"}
}
```

**Field details:**

| Field                        | Type                | Required                  | Description                                                                                                                                                                              |
| ---------------------------- | ------------------- | ------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `search.query`               | string (min 1 char) | Yes                       | Natural language query. Any URL in the query will be crawled and used as context.                                                                                                        |
| `search.count`               | number (>= 1)       | No (default 10)           | Target number of items to find. Actual results may be fewer depending on query complexity.                                                                                               |
| `search.criteria`            | array (1–5 items)   | No                        | Verification rules. Auto-detected from query if omitted. Each has a `description` string.                                                                                                |
| `search.entity`              | object              | No                        | One of: `{"type": "company"}`, `{"type": "person"}`, `{"type": "article"}`, `{"type": "research_paper"}`, `{"type": "custom", "description": "Job Postings"}`. Auto-detected if omitted. |
| `search.maxPeoplePerCompany` | integer (>= 1)      | No                        | Optional soft cap for people searches. When set, the search tries to include at most this many matching people from the same current employer company.                                   |
| `search.behaviour`           | string              | No (default `"override"`) | `"override"`: reuses existing items, re-evaluates against new criteria, discards non-matching.                                                                                           |
| `search.metadata`            | object              | No                        | Arbitrary key-value pairs for the search.                                                                                                                                                |
| `enrichments`                | array (max 10)      | No                        | Each enrichment has `description` (required), `format`, `options`, `metadata`.                                                                                                           |
| `enrichments[].description`  | string (min 1 char) | Yes                       | What data to extract.                                                                                                                                                                    |
| `enrichments[].format`       | string              | No                        | One of: `text`, `number`, `date`, `url`, `email`, `phone`, `options`. Auto-detected if omitted.                                                                                          |
| `enrichments[].options`      | array (1–20 items)  | Conditional               | Required when format is `options`. Each has a `label` string.                                                                                                                            |
| `enrichments[].metadata`     | object              | No                        | Arbitrary key-value pairs.                                                                                                                                                               |
| `externalId`                 | string              | No                        | Your own identifier. Can be used in place of webset ID in all GET/PATCH/DELETE calls. Returns 409 if duplicate.                                                                          |
| `metadata`                   | object              | No                        | Arbitrary key-value pairs for the webset.                                                                                                                                                |

**Response:** A `Webset` object (see Object Schemas below).

#### GET `/websets/{id}` — Get a Webset

* `{id}` can be the webset ID or `externalId`.
* Query param `?expand=items` includes up to 100 items in the response.

**Response:** A `Webset` object. When expanded, includes an `items` array of `WebsetItem` objects.

#### GET `/websets/` — List All Websets

**Query params:**

| Param    | Type   | Description                                              |
| -------- | ------ | -------------------------------------------------------- |
| `cursor` | string | Pagination cursor from previous response's `nextCursor`. |
| `limit`  | number | Results per page (max 200).                              |

**Response:** `{ "data": [Webset, ...], "hasMore": boolean, "nextCursor": string | null }`

#### POST `/websets/{id}` — Update a Webset

**Request body:**

```json theme={null}
{
  "metadata": {"project": "updated-value"}
}
```

Only `metadata` can be updated. **Response:** Updated `Webset` object.

#### DELETE `/websets/{id}` — Delete a Webset

Deletes the webset and all associated items, searches, and enrichments. **Response:** The deleted `Webset` object.

#### POST `/websets/{id}/cancel` — Cancel Running Operations

Cancels all running searches and enrichments on the webset. **Response:** The `Webset` object with updated status.

#### POST `/websets/preview` — Preview Search Results

Runs a search without creating a webset. Same request body as create. Useful for testing queries before committing.

***

### Items

#### GET `/websets/{websetId}/items` — List Items

**Query params:**

| Param    | Type   | Description        |
| -------- | ------ | ------------------ |
| `cursor` | string | Pagination cursor. |
| `limit`  | number | Results per page.  |

**Response:** `{ "data": [WebsetItem, ...], "hasMore": boolean, "nextCursor": string | null }`

#### GET `/websets/{websetId}/items/{itemId}` — Get a Single Item

**Response:** A `WebsetItem` object.

#### DELETE `/websets/{websetId}/items/{itemId}` — Delete an Item

**Response:** The deleted `WebsetItem` object.

***

### Searches

#### POST `/websets/{websetId}/searches` — Add a Search

Add a new search to an existing webset. Request body is the same shape as `search` in the create webset request:

```json theme={null}
{
  "query": "VPs of engineering at AI startups in Asia with Series B funding",
  "count": 25,
  "maxPeoplePerCompany": 2,
  "criteria": [
    {"description": "Person is a VP of engineering"}
  ],
  "entity": {"type": "person"},
  "behaviour": "override"
}
```

Searches run sequentially (not in parallel with other searches), but can run in parallel with enrichments.

**Response:** A `WebsetSearch` object.

#### GET `/websets/{websetId}/searches/{searchId}` — Get Search Status

**Response:** A `WebsetSearch` object with `progress` field showing `found` count and `completion` percentage (0–100).

#### POST `/websets/{websetId}/searches/{searchId}/cancel` — Cancel a Search

**Response:** The canceled `WebsetSearch` object.

***

### Enrichments

#### POST `/websets/{websetId}/enrichments` — Add an Enrichment

```json theme={null}
{
  "description": "Find the company's LinkedIn page URL",
  "format": "url",
  "metadata": {"source": "linkedin"}
}
```

Enrichments are applied to all existing items and any future items added to the webset.

**Response:** A `WebsetEnrichment` object.

#### GET `/websets/{websetId}/enrichments/{enrichmentId}` — Get Enrichment Status

**Response:** A `WebsetEnrichment` object.

#### PATCH `/websets/{websetId}/enrichments/{enrichmentId}` — Update an Enrichment

```json theme={null}
{
  "description": "Updated enrichment description",
  "format": "text"
}
```

**Response:** Updated `WebsetEnrichment` object.

#### DELETE `/websets/{websetId}/enrichments/{enrichmentId}` — Delete an Enrichment

**Response:** The deleted `WebsetEnrichment` object.

#### POST `/websets/{websetId}/enrichments/{enrichmentId}/cancel` — Cancel a Running Enrichment

**Response:** The canceled `WebsetEnrichment` object.

***

### Exports

#### POST `/websets/{websetId}/exports` — Schedule an Export

Generates a downloadable file of all items. Request body:

```json theme={null}
{
  "format": "csv"
}
```

**Response:** An export object with `id`, `status` (`pending` → `completed`), and `downloadUrl` (available when completed).

#### GET `/websets/{websetId}/exports/{exportId}` — Get Export Status

Poll until `status` is `completed`, then use the `downloadUrl`.

***

### Imports

Imports let you bring your own URLs (e.g. from a CSV) and run enrichments on them.

#### POST `/imports` — Create an Import

```json theme={null}
{
  "websetId": "ws_abc123",
  "urls": [
    "https://example.com/company-a",
    "https://example.com/company-b"
  ]
}
```

**Response:** An import object with `id` and `status`.

#### GET `/imports/{importId}` — Get Import Details

**Response:** Import object with status and progress.

#### GET `/imports` — List All Imports

**Query params:** `cursor`, `limit` (same pagination pattern).

**Response:** `{ "data": [Import, ...], "hasMore": boolean, "nextCursor": string | null }`

#### PATCH `/imports/{importId}` — Update an Import

#### DELETE `/imports/{importId}` — Delete an Import

***

### Monitors

Monitors run searches on a schedule to keep websets updated.

#### POST `/monitors` — Create a Monitor

```json theme={null}
{
  "websetId": "ws_abc123",
  "cadence": {
    "cron": "0 9 * * 1",
    "timezone": "America/New_York"
  },
  "behavior": {
    "type": "search",
    "config": {
      "parameters": {
        "query": "AI startups that raised Series A in the last week",
        "count": 10,
        "criteria": [
          {"description": "Company is an AI startup"},
          {"description": "Raised Series A in the last week"}
        ],
        "entity": {"type": "company"},
        "behavior": "append"
      }
    }
  }
}
```

**Field details:**

| Field                        | Type   | Required                 | Description                                                             |
| ---------------------------- | ------ | ------------------------ | ----------------------------------------------------------------------- |
| `websetId`                   | string | Yes                      | The webset to attach the monitor to.                                    |
| `cadence.cron`               | string | Yes                      | Standard 5-field Unix cron expression. Triggers at most once per day.   |
| `cadence.timezone`           | string | No (default `"Etc/UTC"`) | IANA timezone string.                                                   |
| `behavior.type`              | string | Yes                      | `"search"` (find new items) or `"refresh"` (re-process existing items). |
| `behavior.config.parameters` | object | Yes for `search`         | Same shape as the `search` object in create webset.                     |

**Response:** A monitor object.

#### GET `/monitors/{monitorId}` — Get Monitor Details

#### PATCH `/monitors/{monitorId}` — Update a Monitor

Update cadence, behavior, or metadata.

#### DELETE `/monitors/{monitorId}` — Delete a Monitor

#### GET `/monitors` — List All Monitors

**Query params:** `cursor`, `limit`.

**Response:** `{ "data": [Monitor, ...], "hasMore": boolean, "nextCursor": string | null }`

#### GET `/monitors/{monitorId}/runs` — List Monitor Runs

Returns the history of executions for this monitor.

#### GET `/monitors/{monitorId}/runs/{runId}` — Get a Monitor Run

***

### Webhooks

#### POST `/webhooks` — Create a Webhook

```json theme={null}
{
  "url": "https://your-server.com/webhook",
  "events": ["webset.item.created", "webset.idle"],
  "metadata": {"env": "production"}
}
```

| Field      | Type               | Required | Description                                                                                                |
| ---------- | ------------------ | -------- | ---------------------------------------------------------------------------------------------------------- |
| `url`      | string (URL)       | Yes      | Endpoint to receive webhook POST requests. Must be the final destination URL — redirects are not followed. |
| `events`   | array (1–12 items) | Yes      | Event types to subscribe to (see Event Types below).                                                       |
| `metadata` | object             | No       | Arbitrary key-value pairs.                                                                                 |

**Response:** A `Webhook` object. **Important:** The `secret` field is only returned on creation. Store it securely for signature verification.

> **Redirects are not followed.** Webhook deliveries are sent directly to the registered `url`. If your endpoint responds with a 3xx redirect, the delivery will be treated as a failure. Always register the final destination URL.

#### GET `/webhooks/{webhookId}` — Get Webhook Details

#### PATCH `/webhooks/{webhookId}` — Update a Webhook

Update `url`, `events`, or `metadata`.

#### DELETE `/webhooks/{webhookId}` — Delete a Webhook

#### GET `/webhooks` — List All Webhooks

**Query params:** `cursor`, `limit`.

#### GET `/webhooks/{webhookId}/attempts` — List Delivery Attempts

Returns the history of delivery attempts for this webhook, including response status codes and bodies.

**Response:** `{ "data": [WebhookAttempt, ...], "hasMore": boolean, "nextCursor": string | null }`

#### Webhook Signature Verification

Webhooks are signed with HMAC SHA256. The signature is in the `Exa-Signature` header:

```
Exa-Signature: t=1234567890,v1=abc123signature...
```

**Verification steps:**

1. Parse the header to extract `t` (timestamp) and `v1` (signature).
2. Construct the signed payload: `{timestamp}.{raw_request_body}`.
3. Compute HMAC SHA256 using the `secret` from webhook creation.
4. Compare your computed signature with `v1`.

```python theme={null}
import hmac
import hashlib

def verify_webhook(payload: bytes, signature_header: str, secret: str) -> bool:
    parts = dict(p.split("=", 1) for p in signature_header.split(","))
    timestamp = parts["t"]
    expected_sig = parts["v1"]
    signed_payload = f"{timestamp}.{payload.decode()}".encode()
    computed = hmac.new(secret.encode(), signed_payload, hashlib.sha256).hexdigest()
    return hmac.compare_digest(computed, expected_sig)
```

```javascript theme={null}
const crypto = require("crypto");

function verifyWebhook(payload, signatureHeader, secret) {
  const parts = Object.fromEntries(signatureHeader.split(",").map(p => p.split("=", 2)));
  const signedPayload = `${parts.t}.${payload}`;
  const computed = crypto.createHmac("sha256", secret).update(signedPayload).digest("hex");
  return crypto.timingSafeEqual(Buffer.from(computed), Buffer.from(parts.v1));
}
```

***

### Events

Events track state changes across the system. Retained for 60 days.

#### GET `/events` — List All Events

**Query params:** `cursor`, `limit`.

**Response:** `{ "data": [Event, ...], "hasMore": boolean, "nextCursor": string | null }`

#### GET `/events/{eventId}` — Get a Single Event

**Response:** An event object with `id`, `object` (`"event"`), `type`, `data`, and `createdAt`.

***

### Teams

#### GET `/teams/me` — Get Team Info

Returns your team's concurrency usage and limits.

***

## Object Schemas

### Webset

```json theme={null}
{
  "id": "ws_abc123",
  "object": "webset",
  "status": "idle",
  "externalId": "my-unique-id",
  "searches": [WebsetSearch],
  "enrichments": [WebsetEnrichment],
  "metadata": {},
  "createdAt": "2024-01-15T10:00:00Z",
  "updatedAt": "2024-01-15T10:05:00Z"
}
```

**Status values:** `running`, `idle`, `paused`

### WebsetSearch

```json theme={null}
{
  "id": "ws_search_abc",
  "object": "webset_search",
  "status": "completed",
  "query": "AI companies in Europe",
  "entity": {"type": "company"},
  "criteria": [
    {
      "description": "Company is an AI startup",
      "successRate": 85.5
    }
  ],
  "count": 50,
  "maxPeoplePerCompany": null,
  "progress": {
    "found": 42,
    "completion": 100.0
  },
  "metadata": {},
  "canceledAt": null,
  "canceledReason": null,
  "createdAt": "2024-01-15T10:00:00Z",
  "updatedAt": "2024-01-15T10:05:00Z"
}
```

**Status values:** `created`, `running`, `completed`, `canceled`

**Canceled reasons:** `webset_deleted`, `webset_canceled`

**Progress:** `found` = number of items discovered so far. `completion` = percentage (0–100).

### WebsetItem

```json theme={null}
{
  "id": "wsi_abc123",
  "object": "webset_item",
  "source": "search",
  "sourceId": "ws_search_abc",
  "websetId": "ws_abc123",
  "properties": {
    "type": "company",
    "url": "https://example.com",
    "description": "An AI company focused on NLP",
    "content": "Full text content of the page...",
    "company": {
      "name": "Example AI",
      "location": "London, UK",
      "employees": 150,
      "industry": "Artificial Intelligence",
      "about": "Example AI builds NLP tools.",
      "logoUrl": "https://example.com/logo.png"
    }
  },
  "evaluations": [
    {
      "criterion": "Company is an AI startup",
      "reasoning": "The company's website describes AI-powered products...",
      "satisfied": "yes",
      "references": [
        {
          "title": "About Example AI",
          "snippet": "We build cutting-edge AI tools...",
          "url": "https://example.com/about"
        }
      ]
    }
  ],
  "enrichments": [
    {
      "object": "enrichment_result",
      "enrichmentId": "enr_abc123",
      "format": "text",
      "result": ["Jane Smith"],
      "reasoning": "Found the CEO listed on the company's leadership page.",
      "references": [
        {
          "title": "Leadership",
          "url": "https://example.com/team"
        }
      ]
    }
  ],
  "createdAt": "2024-01-15T10:02:00Z",
  "updatedAt": "2024-01-15T10:04:00Z"
}
```

### Item Properties by Entity Type

**Company** (`properties.type = "company"`):

| Field               | Type    | Description                              |
| ------------------- | ------- | ---------------------------------------- |
| `url`               | string  | Company website URL                      |
| `description`       | string  | Short description of relevance           |
| `content`           | string? | Full text content of the company website |
| `company.name`      | string  | Company name                             |
| `company.location`  | string? | Main location                            |
| `company.employees` | number? | Employee count                           |
| `company.industry`  | string? | Industry                                 |
| `company.about`     | string? | Short description                        |
| `company.logoUrl`   | string? | Logo URL                                 |

**Person** (`properties.type = "person"`):

| Field               | Type    | Description                    |
| ------------------- | ------- | ------------------------------ |
| `url`               | string  | Profile URL                    |
| `description`       | string  | Short description of relevance |
| `person.name`       | string  | Full name                      |
| `person.location`   | string? | Location                       |
| `person.position`   | string? | Current work position          |
| `person.pictureUrl` | string? | Profile image URL              |

**Article** (`properties.type = "article"`):

| Field                 | Type    | Description                    |
| --------------------- | ------- | ------------------------------ |
| `url`                 | string  | Article URL                    |
| `description`         | string  | Short description of relevance |
| `content`             | string? | Full text content              |
| `article.author`      | string? | Author(s)                      |
| `article.publishedAt` | string? | Publication date               |

**Research Paper** (`properties.type = "research_paper"`):

| Field                       | Type    | Description                    |
| --------------------------- | ------- | ------------------------------ |
| `url`                       | string  | Paper URL                      |
| `description`               | string  | Short description of relevance |
| `content`                   | string? | Full text content              |
| `researchPaper.author`      | string? | Author(s)                      |
| `researchPaper.publishedAt` | string? | Publication date               |

**Custom** (`properties.type = "custom"`):

| Field                | Type    | Description       |
| -------------------- | ------- | ----------------- |
| `url`                | string  | Item URL          |
| `description`        | string  | Short description |
| `content`            | string? | Full text content |
| `custom.author`      | string? | Author(s)         |
| `custom.publishedAt` | string? | Publication date  |

### WebsetEnrichment

```json theme={null}
{
  "id": "enr_abc123",
  "object": "webset_enrichment",
  "status": "completed",
  "websetId": "ws_abc123",
  "title": "CEO Name",
  "description": "Find the CEO name",
  "format": "text",
  "options": null,
  "instructions": "Auto-generated instructions...",
  "metadata": {},
  "createdAt": "2024-01-15T10:00:00Z",
  "updatedAt": "2024-01-15T10:05:00Z"
}
```

**Status values:** `pending`, `completed`, `canceled`

**Format values:** `text`, `number`, `date`, `url`, `email`, `phone`, `options`

When `format` is `options`, the `options` array contains objects with a `label` field (max 20 options).

### EnrichmentResult (on each item)

```json theme={null}
{
  "object": "enrichment_result",
  "enrichmentId": "enr_abc123",
  "format": "text",
  "result": ["Jane Smith"],
  "reasoning": "Found on the leadership page",
  "references": [
    {"title": "Team Page", "snippet": "Jane Smith, CEO", "url": "https://..."}
  ]
}
```

`result` is always an array of strings (even for numbers/dates — they're stringified). `null` if the enrichment couldn't find the data.

### Evaluation (on each item)

```json theme={null}
{
  "criterion": "Company is an AI startup",
  "reasoning": "Website describes AI-powered products...",
  "satisfied": "yes",
  "references": [{"title": "...", "snippet": "...", "url": "..."}]
}
```

`satisfied` values: `yes`, `no`, `unclear`

### Webhook

```json theme={null}
{
  "id": "wh_abc123",
  "object": "webhook",
  "status": "active",
  "url": "https://your-server.com/webhook",
  "events": ["webset.item.created", "webset.idle"],
  "secret": "whsec_...",
  "metadata": {},
  "createdAt": "2024-01-15T10:00:00Z",
  "updatedAt": "2024-01-15T10:00:00Z"
}
```

**Status values:** `active`, `inactive`

**`secret` is only returned on creation.** Store it immediately for signature verification.

### WebhookAttempt

```json theme={null}
{
  "id": "wha_abc123",
  "object": "webhook_attempt",
  "eventId": "evt_abc123",
  "eventType": "webset.item.created",
  "webhookId": "wh_abc123",
  "url": "https://your-server.com/webhook",
  "successful": true,
  "responseStatusCode": 200,
  "responseHeaders": {},
  "responseBody": "OK",
  "attempt": 1,
  "attemptedAt": "2024-01-15T10:02:00Z"
}
```

***

## Event Types

| Event                     | When                                      | Data           |
| ------------------------- | ----------------------------------------- | -------------- |
| `webset.created`          | Webset is created                         | `Webset`       |
| `webset.deleted`          | Webset is deleted                         | `Webset`       |
| `webset.paused`           | Webset is paused                          | `Webset`       |
| `webset.idle`             | All operations complete                   | `Webset`       |
| `webset.search.created`   | A search starts                           | `WebsetSearch` |
| `webset.search.updated`   | Search progress updates                   | `WebsetSearch` |
| `webset.search.completed` | A search finishes                         | `WebsetSearch` |
| `webset.search.canceled`  | A search is canceled                      | `WebsetSearch` |
| `webset.item.created`     | A new item is added (passed verification) | `WebsetItem`   |
| `webset.item.enriched`    | An enrichment result is added to an item  | `WebsetItem`   |
| `webset.export.created`   | An export is scheduled                    | Export         |
| `webset.export.completed` | An export is ready to download            | Export         |
| `import.created`          | An import starts                          | Import         |
| `import.completed`        | An import finishes                        | Import         |
| `monitor.created`         | A monitor is created                      | Monitor        |
| `monitor.updated`         | A monitor's configuration is updated      | Monitor        |
| `monitor.deleted`         | A monitor is deleted                      | Monitor        |
| `monitor.run.created`     | A monitor run starts                      | MonitorRun     |
| `monitor.run.completed`   | A monitor run finishes                    | MonitorRun     |

**Event shape:**

```json theme={null}
{
  "id": "evt_abc123",
  "object": "event",
  "type": "webset.item.created",
  "data": { ... },
  "createdAt": "2024-01-15T10:02:00Z"
}
```

Events are retained for **60 days** before automatic deletion.

***

## Pagination

All list endpoints use cursor-based pagination:

```json theme={null}
{
  "data": [...],
  "hasMore": true,
  "nextCursor": "cursor_abc123"
}
```

Pass `nextCursor` as the `cursor` query parameter in the next request. Continue until `hasMore` is `false`.

**Python SDK:** Use `page.has_more` and `page.next_cursor` (snake\_case attributes). **JavaScript SDK:** Use `page.hasMore` and `page.nextCursor`.

***

## Patterns and Best Practices

* **Websets are async.** After creating, poll with GET or use webhooks. Don't expect results in the create response.
* **Use `wait_until_idle`** in SDKs to block until processing completes. Default timeout is 3600s (1 hour), poll interval 5s.
* **Multiple searches can run on one webset.** Use `POST /websets/{id}/searches` to add more. Searches run sequentially with each other but in parallel with enrichments.
* **Items are available immediately.** You can list items while the webset is still `running`.
* **Enrichment format controls output type.** Use `text`, `number`, `date`, `url`, `email`, `phone`, or `options`.
* **`options` format requires an `options` array** with 1–20 items, each having a `label` string.
* **Monitor cron triggers at most once per day.** This is a system constraint.
* **Use `expand=items` for convenience.** `GET /websets/{id}?expand=items` returns the webset and its latest 100 items in one call.
* **Use `externalId` for idempotency.** Set `externalId` on creation to prevent duplicate websets. Returns 409 if the ID already exists. You can then use `externalId` in place of `id` for all subsequent API calls.
* **Webhook secrets are shown once.** The `secret` field is only returned in the create webhook response. Store it immediately.
* **Enrichment results are arrays.** Even for single values, `result` is always `["value"]` or `null` if not found.
* **Criteria `successRate`** on search responses shows what percentage (0–100) of evaluated items matched that criterion.
* **Entity type auto-detection works well.** Only specify `entity` when you need fine control. For non-standard entities, use `{"type": "custom", "description": "Your entity type"}`.
* **Item data is nested under `properties`.** Access `item.properties.url`, `item.properties.company.name`, etc. — not `item.url`. Enrichment results are at `item.enrichments[].result` (always a `list[str]` or `null`).
* **Enrichment results have `enrichmentId`, not `description`.** To get the human-readable description, build a map from `webset.enrichments`: `{e.id: e.description for e in webset.enrichments}`, then look up `enr.enrichment_id`.
* **Initial search is on the webset object.** After `create()`, the search is at `webset.searches[0]` — no separate list call needed. Poll progress via `searches.get(webset_id, search_id)`.

## Full API Reference

For detailed request/response schemas for each endpoint, see the [Websets API Reference](/websets/api/overview).
