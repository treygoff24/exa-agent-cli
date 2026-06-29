> ## Documentation Index
> Fetch the complete documentation index at: https://exa.ai/docs/llms.txt
> Use this file to discover all available pages before exploring further.

# Monitors API Reference

> Self-contained reference with all endpoints, parameters, and examples for coding agents.

## Overview

**Base URL:** `https://api.exa.ai/monitors`

**Auth:** Pass your API key via the `x-api-key` header. Get one at [https://dashboard.exa.ai/api-keys](https://dashboard.exa.ai/api-keys)

Monitors are scheduled, recurring Exa searches. You define a search query and an interval, and the system runs the search automatically and delivers results to your webhook. Each run automatically deduplicates against previous results so you only see new content.

## Installation

```bash theme={null}
pip install exa-py    # Python
npm install exa-js    # JavaScript
```

## Minimal Working Example

<CodeGroup>
  ```python Python theme={null}
  from exa_py import Exa
  import os, time
  exa = Exa(api_key=os.getenv("EXA_API_KEY"))
  # 1. Create monitor
  monitor = exa.monitors.create(params={
      "search": {
          "query": "AI startups that raised Series A funding",
          "numResults": 10
      },
      "webhook": {
          "url": "https://example.com/webhook"
      }
  })
  # Store the webhook secret for signature verification — only returned on creation
  # See: Webhook Signature Verification section
  print(monitor.webhook_secret)
  # 2. Trigger a run and poll for results
  exa.monitors.trigger(monitor.id)
  while True:
      runs = exa.monitors.runs.list(monitor.id)
      latest = runs.data[0]
      if latest.status in ("completed", "failed"):
          break
      time.sleep(2)
  # 3. Print results
  if latest.status == "completed":
      run = exa.monitors.runs.get(monitor.id, latest.id)
      if run.output and run.output.results:
          for result in run.output.results:
              print(f"- {result['title']}: {result['url']}")
  ```

  ```javascript JavaScript theme={null}
  import Exa from "exa-js";
  const exa = new Exa(process.env.EXA_API_KEY);
  // 1. Create monitor
  const monitor = await exa.monitors.create({
    search: {
      query: "AI startups that raised Series A funding",
      numResults: 10
    },
    webhook: {
      url: "https://example.com/webhook"
    }
  });
  // Store the webhook secret for signature verification — only returned on creation
  // See: Webhook Signature Verification section
  console.log(monitor.webhookSecret);
  // 2. Trigger a run and poll for results
  await exa.monitors.trigger(monitor.id);
  let latest;
  while (true) {
    const runs = await exa.monitors.runs.list(monitor.id);
    latest = runs.data[0];
    if (latest.status === "completed" || latest.status === "failed") break;
    await new Promise(r => setTimeout(r, 2000));
  }
  // 3. Print results
  if (latest.status === "completed" && latest.output) {
    const run = await exa.monitors.runs.get(monitor.id, latest.id);
    for (const result of run.output.results) {
      console.log(`- ${result.title}: ${result.url}`);
    }
  }
  ```

  ```bash cURL theme={null}
  # 1. Create monitor
  curl -X POST "https://api.exa.ai/monitors" \
    -H "Content-Type: application/json" \
    -H "x-api-key: $EXA_API_KEY" \
    -d '{
      "search": {
        "query": "AI startups that raised Series A funding",
        "numResults": 10
      },
      "webhook": {
        "url": "https://example.com/webhook"
      }
    }'
  # 2. Trigger a run (replace MONITOR_ID with the id from the create response)
  curl -X POST "https://api.exa.ai/monitors/{MONITOR_ID}/trigger" \
    -H "x-api-key: $EXA_API_KEY"
  # 3. List runs to check status
  curl "https://api.exa.ai/monitors/{MONITOR_ID}/runs" \
    -H "x-api-key: $EXA_API_KEY"
  ```
</CodeGroup>

***

## Endpoints

### POST `/monitors` — Create a Monitor

Creates a monitor and returns it with a one-time `webhookSecret`.

**Request body:**

| Field          | Type   | Required | Description                                                                                                                                               |
| -------------- | ------ | -------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `name`         | string | No       | Display name for the monitor.                                                                                                                             |
| `search`       | object | **Yes**  | Search configuration. See [Search Parameters](#search-parameters).                                                                                        |
| `trigger`      | object | No       | Interval schedule. See [Trigger](#trigger). Omit for manual-only monitors.                                                                                |
| `outputSchema` | object | No       | JSON Schema for structured output. See [Output Schema](#output-schema).                                                                                   |
| `metadata`     | object | No       | Arbitrary key-value pairs for your own tracking. Echoed back in webhook deliveries, useful for routing updates to Slack threads, tickets, or CRM records. |
| `webhook`      | object | **Yes**  | Webhook configuration. See [Webhook](#webhook).                                                                                                           |

**Response:** A [Monitor object](#monitor-object) with an additional `webhookSecret` field (string). Store this secret immediately — it is only returned once and is needed for [webhook signature verification](#webhook-signature-verification).

### GET `/monitors` — List Monitors

**Query params:**

| Param    | Type    | Default | Description                                                |
| -------- | ------- | ------- | ---------------------------------------------------------- |
| `status` | string  | —       | Filter by status: `active`, `paused`, or `disabled`.       |
| `cursor` | string  | —       | Pagination cursor from a previous response's `nextCursor`. |
| `limit`  | integer | `50`    | Results per page (1-100).                                  |

**Response:** `{ "data": [Monitor, ...], "hasMore": boolean, "nextCursor": string | null }`

### GET `/monitors/{id}` — Get a Monitor

**Response:** A [Monitor object](#monitor-object).

### PATCH `/monitors/{id}` — Update a Monitor

All fields are optional. For `search`, you can send a partial object (only the fields you want to change). Set `trigger` to `null` to remove the schedule.

**Request body:**

| Field          | Type           | Description                                                                  |
| -------------- | -------------- | ---------------------------------------------------------------------------- |
| `name`         | string         | Updated name.                                                                |
| `status`       | string         | `active` or `paused`.                                                        |
| `search`       | object         | Partial search params to merge.                                              |
| `trigger`      | object or null | New interval trigger, or `null` to remove.                                   |
| `outputSchema` | object or null | New output schema, or `null` to remove. See [Output Schema](#output-schema). |
| `metadata`     | object or null | New metadata, or `null` to remove. Echoed back in webhook deliveries.        |
| `webhook`      | object         | Partial webhook params to merge.                                             |

**Response:** The updated [Monitor object](#monitor-object).

### DELETE `/monitors/{id}` — Delete a Monitor

**Response:** The deleted [Monitor object](#monitor-object).

### POST `/monitors/{id}/trigger` — Trigger a Run

Starts a run immediately, regardless of the schedule. Works for `active` and `paused` monitors.

**Response:** `{ "triggered": true }`

### GET `/monitors/{id}/runs` — List Runs

**Query params:**

| Param    | Type    | Default | Description               |
| -------- | ------- | ------- | ------------------------- |
| `cursor` | string  | —       | Pagination cursor.        |
| `limit`  | integer | `50`    | Results per page (1-100). |

**Response:** `{ "data": [Run, ...], "hasMore": boolean, "nextCursor": string | null }`

### GET `/monitors/{id}/runs/{runId}` — Get a Run

**Response:** A [Run object](#run-object).

***

## Search Parameters

Nested under `search` in the create/update request.

| Parameter    | Type    | Default        | Description                                                                  |
| ------------ | ------- | -------------- | ---------------------------------------------------------------------------- |
| `query`      | string  | **(required)** | The search query to run. Supports natural language descriptions.             |
| `numResults` | integer | `10`           | Number of results per run (1-100).                                           |
| `contents`   | object  | —              | Content extraction options. See [Contents Parameters](#contents-parameters). |

### Contents Parameters

Nested under `search.contents`. All fields are optional.

| Parameter            | Type                | Description                                                                                                                                                                                        |
| -------------------- | ------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `text`               | boolean or object   | Return full page text as markdown. Object form: `{ maxCharacters, includeHtmlTags, verbosity, includeSections, excludeSections }`.                                                                 |
| `highlights`         | boolean or object   | Return key excerpts. Pass `true` for the highest-quality default. Object form: `{ query, maxCharacters }` — use `query` to guide which highlights are returned, `maxCharacters` to cap the budget. |
| `summary`            | boolean or object   | Return LLM-generated summary. Object form: `{ query, maxTokens }`.                                                                                                                                 |
| `extras`             | object              | Extract links and media: `{ links, imageLinks, richImageLinks, richLinks, codeBlocks }` (all integers 0-1000).                                                                                     |
| `context`            | boolean or object   | Return surrounding context. Object form: `{ maxCharacters }`.                                                                                                                                      |
| `livecrawl`          | string              | Crawl strategy: `"never"`, `"always"`, `"fallback"`, `"auto"`, or `"preferred"`.                                                                                                                   |
| `livecrawlTimeout`   | integer             | Livecrawl timeout in ms (0-90000).                                                                                                                                                                 |
| `maxAgeHours`        | integer             | Max age of cached content in hours. `0` = always livecrawl. `-1` = never livecrawl.                                                                                                                |
| `filterEmptyResults` | boolean             | Filter out results with no content.                                                                                                                                                                |
| `subpages`           | integer             | Number of subpages to crawl per result (0-100).                                                                                                                                                    |
| `subpageTarget`      | string or string\[] | Keywords to prioritize when selecting subpages.                                                                                                                                                    |

### Text Object Options

| Parameter         | Type      | Description                                                                                                  |
| ----------------- | --------- | ------------------------------------------------------------------------------------------------------------ |
| `maxCharacters`   | integer   | Character limit for returned text.                                                                           |
| `includeHtmlTags` | boolean   | Preserve HTML tags in output.                                                                                |
| `verbosity`       | string    | `"compact"`, `"standard"`, or `"full"`.                                                                      |
| `includeSections` | string\[] | Only include these page sections: `header`, `navigation`, `banner`, `body`, `sidebar`, `footer`, `metadata`. |
| `excludeSections` | string\[] | Exclude these page sections. Same options as above.                                                          |

### Highlights Object Options

Prefer `highlights: true` for the highest-quality default. Only supply this object when you specifically need to guide selection with a custom query or cap output size.

| Parameter       | Type    | Description                                                                                                                     |
| --------------- | ------- | ------------------------------------------------------------------------------------------------------------------------------- |
| `query`         | string  | Custom query that guides which highlights are returned.                                                                         |
| `maxCharacters` | integer | Cap on total highlight characters. Omit unless you have a specific budget — leaving it unset gives the highest-quality default. |

### Summary Object Options

| Parameter   | Type    | Description                     |
| ----------- | ------- | ------------------------------- |
| `query`     | string  | Custom query for the summary.   |
| `maxTokens` | integer | Maximum tokens for the summary. |

## Trigger

Nested under `trigger` in the create/update request.

| Field    | Type   | Required | Description                                                                                           |
| -------- | ------ | -------- | ----------------------------------------------------------------------------------------------------- |
| `type`   | string | **Yes**  | Must be `"interval"`.                                                                                 |
| `period` | string | **Yes**  | Duration string (e.g., `"1h"`, `"6h"`, `"1d"`, `"7d"`). Single-unit only. Minimum interval is 1 hour. |

The schedule is anchored to the monitor's creation time. For example, a monitor created at 2:30 PM with `"period": "1d"` will run daily at \~2:30 PM (with up to 30 minutes of jitter to spread load).

```json theme={null}
{
  "trigger": {
    "type": "interval",
    "period": "7d"
  }
}
```

## Webhook

Nested under `webhook` in the create/update request.

| Field    | Type      | Required | Description                                                         |
| -------- | --------- | -------- | ------------------------------------------------------------------- |
| `url`    | string    | **Yes**  | HTTPS URL. Must be a public endpoint (no localhost or private IPs). |
| `events` | string\[] | No       | Which events to deliver. Omit to receive all events.                |

### Webhook Events

| Event                   | Fired When                          |
| ----------------------- | ----------------------------------- |
| `monitor.created`       | A new monitor is created            |
| `monitor.updated`       | A monitor is updated                |
| `monitor.deleted`       | A monitor is deleted                |
| `monitor.run.created`   | A new run starts                    |
| `monitor.run.completed` | A run finishes (success or failure) |

### Webhook Payload

```json theme={null}
{
  "id": "event_abc123",
  "object": "event",
  "type": "monitor.run.completed",
  "data": {
    "id": "run_xyz789",
    "monitorId": "mon_abc123",
    "status": "completed",
    "metadata": {
      "slack_channel_id": "C123ABC",
      "slack_thread_id": "1745444400.123456",
      "user_id": "U123ABC"
    }
  },
  "createdAt": "2026-03-17T09:00:00Z"
}
```

For `monitor.run.created` and `monitor.run.completed`, `data` contains the run object plus a `metadata` field echoed from the parent monitor. For `monitor.created`, `monitor.updated`, and `monitor.deleted`, `data` contains the full monitor object.

### Slack Routing Pattern

Slack routing identifiers can be stored in monitor metadata and echoed in webhook deliveries to route updates back into the correct thread.

```json theme={null}
{
  "name": "Competitor Launches",
  "search": {
    "query": "New product launches by Acme competitors"
  },
  "metadata": {
    "slack_channel_id": "C123ABC",
    "slack_thread_id": "1745444400.123456",
    "user_id": "U123ABC"
  },
  "webhook": {
    "url": "https://example.com/exa-monitor-webhook",
    "events": ["monitor.run.completed"]
  }
}
```

The run payload includes `data.metadata.slack_channel_id` and `data.metadata.slack_thread_id`, which can be used to decide where to route the update. Exa does not post to Slack directly.

### Webhook Signature Verification

Webhook signature verification lets you confirm that incoming webhook requests actually came from Exa and haven't been tampered with. Without verification, any external party that discovers your webhook URL could send fake payloads to your endpoint. Use the `webhookSecret` returned from the create endpoint to verify signatures on every incoming request.

Every webhook delivery includes an `Exa-Signature` header:

```
Exa-Signature: t=1704729600,v1=5257a869e7ecebeda32affa62cdca3fa51cad7e77a0e56ff536d0ce8e108d8bd
```

To verify:

1. Extract `t` (timestamp) and `v1` (signature) from the header
2. Construct the signed payload: `{t}.{request_body}`
3. Compute HMAC-SHA256 of the signed payload using your webhook secret
4. Compare the computed signature with `v1` using constant-time comparison

<CodeGroup>
  ```python Python theme={null}
  import hmac
  import hashlib
  def verify_webhook(payload: bytes, header: str, secret: str) -> bool:
      parts = dict(part.split("=", 1) for part in header.split(","))
      timestamp = parts["t"]
      signature = parts["v1"]
      signed_payload = f"{timestamp}.{payload.decode()}"
      expected = hmac.new(
          secret.encode(), signed_payload.encode(), hashlib.sha256
      ).hexdigest()
      return hmac.compare_digest(expected, signature)
  ```

  ```javascript JavaScript theme={null}
  import crypto from "crypto";
  function verifyWebhook(payload, header, secret) {
    const parts = Object.fromEntries(
      header.split(",").map(p => p.split("=", 2))
    );
    const signedPayload = `${parts.t}.${payload}`;
    const expected = crypto
      .createHmac("sha256", secret)
      .update(signedPayload)
      .digest("hex");
    const a = Buffer.from(expected);
    const b = Buffer.from(parts.v1 || "");
    if (a.length !== b.length) return false;
    return crypto.timingSafeEqual(a, b);
  }
  ```
</CodeGroup>

***

## Object Schemas

### Monitor Object

```json theme={null}
{
  "id": "mon_abc123",
  "name": "Series A Tracker",
  "status": "active",
  "search": {
    "query": "AI startups that raised Series A funding",
    "numResults": 10,
    "contents": {
      "highlights": true
    }
  },
  "trigger": {
    "type": "interval",
    "period": "7d"
  },
  "outputSchema": null,
  "metadata": null,
  "webhook": {
    "url": "https://example.com/webhook",
    "events": ["monitor.run.completed"]
  },
  "nextRunAt": "2026-03-24T13:00:00.000Z",
  "createdAt": "2026-03-17T09:00:00.000Z",
  "updatedAt": "2026-03-17T09:00:00.000Z"
}
```

| Field          | Type           | Description                                                                         |
| -------------- | -------------- | ----------------------------------------------------------------------------------- |
| `id`           | string         | Unique monitor identifier.                                                          |
| `name`         | string or null | Display name.                                                                       |
| `status`       | string         | `"active"`, `"paused"`, or `"disabled"`. See [Monitor Statuses](#monitor-statuses). |
| `search`       | object         | The search configuration.                                                           |
| `trigger`      | object or null | The interval trigger, or `null` if manual-only.                                     |
| `outputSchema` | object or null | JSON Schema for structured output.                                                  |
| `metadata`     | object or null | Your custom key-value pairs.                                                        |
| `webhook`      | object         | `{ url, events }`.                                                                  |
| `nextRunAt`    | string or null | ISO 8601 timestamp of the next scheduled run. `null` if no trigger.                 |
| `createdAt`    | string         | ISO 8601 creation timestamp.                                                        |
| `updatedAt`    | string         | ISO 8601 last-update timestamp.                                                     |

### Run Object

```json theme={null}
{
  "id": "run_xyz789",
  "monitorId": "mon_abc123",
  "status": "completed",
  "output": {
    "results": [
      {
        "title": "Acme AI raises $25M Series A",
        "url": "https://example.com/article",
        "publishedDate": "2026-03-10"
      }
    ],
    "content": "Structured output here (when outputSchema is set)",
    "grounding": [
      {
        "field": "content",
        "citations": [
          { "url": "https://example.com/article", "title": "Acme AI raises $25M Series A" }
        ],
        "confidence": "high"
      }
    ]
  },
  "failReason": null,
  "startedAt": "2026-03-17T09:00:01.000Z",
  "completedAt": "2026-03-17T09:00:45.000Z",
  "failedAt": null,
  "cancelledAt": null,
  "durationMs": 44000,
  "createdAt": "2026-03-17T09:00:00.000Z",
  "updatedAt": "2026-03-17T09:00:45.000Z"
}
```

| Field              | Type            | Description                                                            |
| ------------------ | --------------- | ---------------------------------------------------------------------- |
| `id`               | string          | Unique run identifier.                                                 |
| `monitorId`        | string          | Parent monitor ID.                                                     |
| `status`           | string          | `"pending"`, `"running"`, `"completed"`, `"failed"`, or `"cancelled"`. |
| `output`           | object or null  | Search results and output. `null` until completed.                     |
| `output.results`   | array           | Array of search result objects (title, url, publishedDate, etc.).      |
| `output.content`   | any             | Structured output when `outputSchema` is set.                          |
| `output.grounding` | array           | Field-level citations with confidence. See [Grounding](#grounding).    |
| `failReason`       | string or null  | Why the run failed. See [Fail Reasons](#fail-reasons).                 |
| `startedAt`        | string or null  | ISO 8601 timestamp when execution began.                               |
| `completedAt`      | string or null  | ISO 8601 timestamp when execution finished.                            |
| `failedAt`         | string or null  | ISO 8601 timestamp if the run failed.                                  |
| `cancelledAt`      | string or null  | ISO 8601 timestamp if the run was cancelled.                           |
| `durationMs`       | integer or null | Total execution time in milliseconds.                                  |
| `createdAt`        | string          | ISO 8601 creation timestamp.                                           |
| `updatedAt`        | string          | ISO 8601 last-update timestamp.                                        |

### Grounding

Each entry in `output.grounding` provides source citations for a field in the output:

| Field        | Type   | Description                                                     |
| ------------ | ------ | --------------------------------------------------------------- |
| `field`      | string | The output field path (e.g. `"content"`, `"results[0].title"`). |
| `citations`  | array  | Sources: `{ url, title }`. Duplicate URLs are deduplicated.     |
| `confidence` | string | `"low"`, `"medium"`, or `"high"`.                               |

***

## Monitor Statuses

Monitors have three possible statuses. An `active` monitor runs on its interval schedule and accepts manual triggers. A `paused` monitor stops running on schedule but still accepts manual triggers via the trigger endpoint — useful for temporarily halting a monitor without deleting it. A `disabled` monitor does not run at all; this status is set automatically by the system and cannot be set via the API.

## Fail Reasons

| Reason                 | Description                             | Action                                                                                     |
| ---------------------- | --------------------------------------- | ------------------------------------------------------------------------------------------ |
| `api_key_invalid`      | API key is invalid or revoked.          | Update your API key. Monitor auto-disables after 10 consecutive failures with this reason. |
| `insufficient_credits` | Not enough credits.                     | Add credits to your account.                                                               |
| `invalid_params`       | Search parameters are invalid.          | Fix the monitor's search configuration.                                                    |
| `rate_limited`         | Too many concurrent requests.           | Reduce monitor frequency or wait.                                                          |
| `search_unavailable`   | Exa search backend is temporarily down. | Retries on next scheduled run.                                                             |
| `search_failed`        | Search execution failed.                | Check search parameters. Contact support if persistent.                                    |
| `internal_error`       | Unexpected error.                       | Contact support if persistent.                                                             |

## Output Schema

`outputSchema` controls how the search synthesizes results into structured output. It supports two modes:

### Text mode (default when no schema is provided)

When `outputSchema` is omitted or set to `{ "type": "text" }`, the run output's `content` field contains a plain text summary synthesized from the search results.

```json theme={null}
{
  "outputSchema": {
    "type": "text",
    "description": "A summary of recent AI funding rounds"
  }
}
```

The `description` field guides the synthesis. When `outputSchema` is omitted entirely, the system generates a text summary based on the search query.

### Object mode

When `type` is `"object"`, you provide a JSON Schema that defines the structure of the output. The search extracts and organizes information from results to match your schema.

```json theme={null}
{
  "outputSchema": {
    "type": "object",
    "description": "Structured competitor intelligence",
    "properties": {
      "headline": { "type": "string", "description": "One-line headline" },
      "category": {
        "type": "string",
        "enum": ["launch", "partnership", "hiring", "other"]
      },
      "summary": { "type": "string", "description": "2-3 sentence summary" }
    },
    "required": ["headline", "category", "summary"],
    "additionalProperties": false
  }
}
```

| Field                  | Type      | Required              | Description                                            |
| ---------------------- | --------- | --------------------- | ------------------------------------------------------ |
| `type`                 | string    | **Yes**               | `"text"` or `"object"`.                                |
| `description`          | string    | No                    | Guides the synthesis. Useful for both modes.           |
| `properties`           | object    | When `type: "object"` | JSON Schema properties definition.                     |
| `required`             | string\[] | No                    | Which properties are required in the output.           |
| `additionalProperties` | boolean   | No                    | Whether extra fields are allowed. Defaults to `false`. |

When `outputSchema` is set, completed runs include:

* `output.content` shaped to your schema
* `output.grounding` with field-level citations and confidence scores

***

## Automatic Deduplication

Monitors deduplicate results across runs using two layers:

**Date-based filtering.** Each run only fetches content published or crawled since the last run. The system uses the interval period to compute a time window with a 2x overlap buffer, so content published between runs is captured even with slight timing variations.

**Semantic deduplication.** The system tracks outputs from the last 5 runs and uses them to focus on new developments. This prevents the same stories or data points from appearing repeatedly.

## Error Handling

| HTTP Status | Meaning                                                    |
| ----------- | ---------------------------------------------------------- |
| 400         | Bad request. Invalid parameters or invalid trigger period. |
| 401         | Invalid or missing API key.                                |
| 404         | Monitor or run not found.                                  |
| 422         | Validation error. Check parameter types and constraints.   |
| 429         | Rate limit exceeded.                                       |

Error response shape:

```json theme={null}
{
  "error": "Error message describing the issue"
}
```

## Common Mistakes

<Warning>
  LLMs frequently generate these incorrect patterns:

  | Wrong                                          | Correct                                                                                                                                                                            |
  | ---------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
  | `searchParams: { query: ... }`                 | Use `search`, not `searchParams`. The API field is `search`.                                                                                                                       |
  | `includeText` / `excludeText` in search params | These fields do not exist on monitors. Use `contents` for content extraction options.                                                                                              |
  | `schedule: "1h"`                               | Use `trigger: { type: "interval", period: "1h" }`. The schedule is nested under `trigger`.                                                                                         |
  | Periods shorter than 1 hour                    | Minimum interval is 1 hour. Values like `"30m"` are rejected.                                                                                                                      |
  | `webhook: "https://..."`                       | `webhook` is an object: `{ url: "https://...", events: [...] }`, not a plain string.                                                                                               |
  | HTTP webhook URLs                              | Webhook URLs must use HTTPS. HTTP URLs are rejected.                                                                                                                               |
  | Localhost or private IP webhook URLs           | Webhook URLs must point to public endpoints.                                                                                                                                       |
  | Webhook URL that redirects                     | Redirects (3xx responses) are not followed. The URL must be the final destination or deliveries will fail.                                                                         |
  | Not storing `webhookSecret` on creation        | The webhook signing secret is only returned once in the create response and is needed for [signature verification](#webhook-signature-verification). It cannot be retrieved later. |
</Warning>

## Patterns and Gotchas

* **Do not set a `type` field on search params.** Monitors handle this internally. Runs typically take 5-60 seconds.
* **Store `webhookSecret` immediately.** It is only returned in the create response and is needed for [webhook signature verification](#webhook-signature-verification). It cannot be retrieved later.
* **Use `trigger` for automation, manual trigger for testing.** You can create a monitor without a trigger and use `POST /monitors/{id}/trigger` to run it on demand. This is useful for testing before adding a schedule.
* **Paused monitors still accept manual triggers.** Set status to `paused` to stop the interval schedule while keeping the monitor available for on-demand runs.
* **Monitor run time is anchored at creation time.** To create a monitor that runs at a specific time, it should be created when you want the monitor to run.
* **`outputSchema` controls structured output.** See [Output Schema](#output-schema) for details on `type: "text"` vs `type: "object"`.
* **Python SDK response attributes use snake\_case.** Access response fields with snake\_case: `monitor.webhook_secret`, `monitor.next_run_at`, `run.fail_reason`. Request dicts use camelCase keys matching the API (e.g., `{"numResults": 10}`). Alternatively, use typed Pydantic models (`CreateSearchMonitorParams`, `UpdateSearchMonitorParams`) with snake\_case field names.
* **Webhook events default to all.** If you omit `events` in the webhook config, all event types are delivered.
* **Use `metadata` for Slack routing.** Store Slack identifiers like `slack_channel_id` and `slack_thread_id` in monitor metadata; run webhooks echo them back in `data.metadata`.
* **Overlap prevention.** If a run is still in progress when the next scheduled time arrives, the in-progress run is cancelled.

## SDK Auto-Pagination Helpers

Both SDKs provide helpers that handle pagination automatically when listing monitors or runs.

<CodeGroup>
  ```python Python theme={null}
  # Iterate through all monitors
  for monitor in exa.monitors.list_all(status="active"):
      print(monitor.id)
  # Collect all monitors into a list
  all_monitors = exa.monitors.get_all(status="active")
  # Iterate through all runs for a monitor
  for run in exa.monitors.runs.list_all(monitor_id):
      print(run.id, run.status)
  # Collect all runs into a list
  all_runs = exa.monitors.runs.get_all(monitor_id)
  ```

  ```javascript JavaScript theme={null}
  // Iterate through all monitors (async generator)
  for await (const monitor of exa.monitors.listAll({ status: "active" })) {
    console.log(monitor.id);
  }
  // Collect all monitors into an array
  const allMonitors = await exa.monitors.getAll({ status: "active" });
  // Iterate through all runs for a monitor
  for await (const run of exa.monitors.runs.listAll(monitorId)) {
    console.log(run.id, run.status);
  }
  // Collect all runs into an array
  const allRuns = await exa.monitors.runs.getAll(monitorId);
  ```
</CodeGroup>

| SDK               | List (single page)           | Iterate all (auto-paginate)      | Collect all                     |
| ----------------- | ---------------------------- | -------------------------------- | ------------------------------- |
| Python            | `exa.monitors.list()`        | `exa.monitors.list_all()`        | `exa.monitors.get_all()`        |
| Python (runs)     | `exa.monitors.runs.list(id)` | `exa.monitors.runs.list_all(id)` | `exa.monitors.runs.get_all(id)` |
| JavaScript        | `exa.monitors.list()`        | `exa.monitors.listAll()`         | `exa.monitors.getAll()`         |
| JavaScript (runs) | `exa.monitors.runs.list(id)` | `exa.monitors.runs.listAll(id)`  | `exa.monitors.runs.getAll(id)`  |

## Complete Examples

### Monitor with structured output and contents

```json theme={null}
{
  "name": "Competitor Tracker",
  "search": {
    "query": "Acme Corp product launches and partnerships",
    "numResults": 5,
    "contents": {
      "highlights": true,
      "text": { "maxCharacters": 10000 }
    }
  },
  "outputSchema": {
    "type": "object",
    "properties": {
      "headline": { "type": "string" },
      "category": { "type": "string", "enum": ["launch", "partnership", "hiring", "other"] },
      "summary": { "type": "string" }
    },
    "required": ["headline", "category", "summary"]
  },
  "trigger": {
    "type": "interval",
    "period": "1d"
  },
  "webhook": {
    "url": "https://example.com/webhook",
    "events": ["monitor.run.completed"]
  }
}
```

### Manual-only monitor (no schedule)

```json theme={null}
{
  "name": "On-Demand Research",
  "search": {
    "query": "recent breakthroughs in quantum computing error correction",
    "numResults": 10
  },
  "webhook": {
    "url": "https://example.com/webhook"
  }
}
```

### Full lifecycle

<CodeGroup>
  ```python Python theme={null}
  from exa_py import Exa
  import os, time
  exa = Exa(api_key=os.getenv("EXA_API_KEY"))
  # 1. Create
  monitor = exa.monitors.create(params={
      "name": "Funding Tracker",
      "search": {
          "query": "AI startups that raised Series A funding",
          "numResults": 10,
          "contents": {
              "highlights": True
          }
      },
      "trigger": {
          "type": "interval",
          "period": "7d"
      },
      "webhook": {
          "url": "https://example.com/webhook",
          "events": ["monitor.run.completed"]
      }
  })
  print(f"Created: {monitor.id}")
  print(f"Secret: {monitor.webhook_secret}")  # Store this!
  # 2. Trigger a test run
  exa.monitors.trigger(monitor.id)
  # 3. Poll for completion
  while True:
      runs = exa.monitors.runs.list(monitor.id)
      latest = runs.data[0]
      if latest.status in ("completed", "failed"):
          break
      time.sleep(2)
  # 4. Fetch results
  if latest.status == "completed":
      run = exa.monitors.runs.get(monitor.id, latest.id)
      if run.output and run.output.results:
          for result in run.output.results:
              print(f"- {result['title']}: {result['url']}")
  else:
      print(f"Failed: {latest.fail_reason}")
  # 5. Pause when not needed
  exa.monitors.update(monitor.id, params={"status": "paused"})
  # 6. Delete when done
  exa.monitors.delete(monitor.id)
  ```

  ```javascript JavaScript theme={null}
  import Exa from "exa-js";
  const exa = new Exa(process.env.EXA_API_KEY);
  // 1. Create
  const monitor = await exa.monitors.create({
    name: "Funding Tracker",
    search: {
      query: "AI startups that raised Series A funding",
      numResults: 10,
      contents: {
        highlights: true
      }
    },
    trigger: {
      type: "interval",
      period: "7d"
    },
    webhook: {
      url: "https://example.com/webhook",
      events: ["monitor.run.completed"]
    }
  });
  console.log(`Created: ${monitor.id}`);
  console.log(`Secret: ${monitor.webhookSecret}`); // Store this!
  // 2. Trigger a test run
  await exa.monitors.trigger(monitor.id);
  // 3. Poll for completion
  let latest;
  while (true) {
    const runs = await exa.monitors.runs.list(monitor.id);
    latest = runs.data[0];
    if (latest.status === "completed" || latest.status === "failed") break;
    await new Promise(r => setTimeout(r, 2000));
  }
  // 4. Fetch results
  if (latest.status === "completed") {
    const run = await exa.monitors.runs.get(monitor.id, latest.id);
    if (run.output?.results) {
      for (const result of run.output.results) {
        console.log(`- ${result.title}: ${result.url}`);
      }
    }
  } else {
    console.log(`Failed: ${latest.failReason}`);
  }
  // 5. Pause when not needed
  await exa.monitors.update(monitor.id, { status: "paused" });
  // 6. Delete when done
  await exa.monitors.delete(monitor.id);
  ```
</CodeGroup>
