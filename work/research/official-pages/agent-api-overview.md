> ## Documentation Index
> Fetch the complete documentation index at: https://exa.ai/docs/llms.txt
> Use this file to discover all available pages before exploring further.

# Overview

> Agent runs asynchronous, multi-step web research, list-building, and enrichment workflows with natural-language answers, structured outputs, and citations.

Agent creates long-running tasks that can search, read, reason, enrich rows, and return answers with source grounding. Use it when a workflow needs more than a single search or contents call: open-ended research, list building, structured extraction, entity enrichment, or follow-up questions over previous results.

For implementation examples and workflow guidance, start with the [Agent guide](/reference/agent-api-guide).

## When to use

* **Entity enrichment**
  * "Return structured intelligence on all input companies: recent brand partnerships, customer stories, and cloud provider investments"
* **KYC / KYB intelligence**
  * "Provide a business profile for PepsiCo: legal name, HQ, revenue, key brands, segments, sourced from SEC filings and IR pages"
* **List building**
  * "Find all engineering professors at UC Berkeley who specialize in AI or machine learning, with their lab name and recent publication"
* **Deep research**
  * "Research the global R\&D footprint of ArcelorMittal: every R\&D site, center, lab, and university partnership worldwide with facility details and sources"

## How it works

1. **Create** a run with [`POST /agent/runs`](/reference/agent-api/create-a-run).
2. The agent **queues and starts** the run, returning an `agent_run` object immediately unless you request streaming.
3. The run **searches, reads, reasons, and writes** until it completes, fails, is cancelled, or reaches the one-hour timeout.
4. You **poll** [`GET /agent/runs/{id}`](/reference/agent-api/get-a-run), **stream** creation events, or **replay** stored events with [`GET /agent/runs/{id}/events`](/reference/agent-api/list-run-events).
5. You can **continue** from a completed run by passing `previousRunId` to a new create request.

## Endpoints

| Method   | Path                      | Description                                                 |
| -------- | ------------------------- | ----------------------------------------------------------- |
| `POST`   | `/agent/runs`             | Create a run. Can return JSON or stream server-sent events. |
| `GET`    | `/agent/runs`             | List runs for your team.                                    |
| `GET`    | `/agent/runs/{id}`        | Get a run by ID.                                            |
| `POST`   | `/agent/runs/{id}/cancel` | Cancel a queued or running run.                             |
| `DELETE` | `/agent/runs/{id}`        | Delete a stored run.                                        |
| `GET`    | `/agent/runs/{id}/events` | List run events or replay them as server-sent events.       |

## Run lifecycle

Runs progress through these statuses:

```text theme={null}
queued -> running -> completed | failed | cancelled
```

Completed, failed, and cancelled runs are terminal. Running or queued runs have `stopReason: null`. Terminal runs use one of these stop reasons:

```text theme={null}
schema_satisfied | budget_reached | error | cancelled
```

## Output

Each run returns an `output` object:

| Field               | Description                                                       |
| ------------------- | ----------------------------------------------------------------- |
| `output.text`       | Natural-language answer or summary.                               |
| `output.structured` | Validated JSON when you provide `outputSchema`; otherwise `null`. |
| `output.grounding`  | Citations for the text answer or structured fields, when emitted. |

`outputSchema` supports JSON Schema draft-07, 2019-09, and 2020-12 via `$schema`. Standard formats are supported, plus `phone`.

To request contact information, include contact fields in `outputSchema` using standard JSON Schema string formats, for example `{ "type": "string", "format": "email" }`. Bound arrays with `maxItems` when possible so the maximum contact-enrichment cost is predictable.

Create requests also accept `effort`, which controls the run's cost and reasoning effort preference. Supported values are `minimal`, `low`, `medium`, `high`, `xhigh`, and `auto`; the default is `auto`.

## Events and streaming

Set `Accept: text/event-stream` when you create a run to stream lifecycle events as they happen. You can also replay stored events later with [`GET /agent/runs/{id}/events`](/reference/agent-api/list-run-events).

Events use standard SSE framing:

```text theme={null}
id: 1
event: agent_run.created
data: {"id":"agent_run_01j...","status":"queued","createdAt":"2026-05-07T21:21:52.051Z"}
```

Terminal event names are `agent_run.completed`, `agent_run.failed`, and `agent_run.cancelled`.

## Limits and pricing

Your Agent concurrency limit is one fifth of your account QPS. For pay-as-you-go accounts with default QPS, this means two active Agent runs at a time.

| Component           | Price             |
| ------------------- | ----------------- |
| Agent Compute Units | `1 ACU = $0.10`   |
| Search tool calls   | `$0.005 / search` |

<Note>
  Contact enrichment is separate from the core pricing components above: email contact enrichment is `$0.02 / email`, and phone number contact enrichment is `$0.07 / phone number`.
</Note>

### Effort

Use `effort` to set a cost and reasoning effort preference for a run. Supported values are `minimal`, `low`, `medium`, `high`, `xhigh`, and `auto`; the default is `auto`. If a fixed effort is set, each run is charged at the following request price:

| Effort    | Price              |
| --------- | ------------------ |
| `minimal` | `$0.012 / request` |
| `low`     | `$0.025 / request` |
| `medium`  | `$0.10 / request`  |
| `high`    | `$0.50 / request`  |
| `xhigh`   | `$1.00 / request`  |

### Choosing an effort mode

Fixed effort modes are best when you want predictable per-request cost on standard research tasks. Use `auto` for variable-scope tasks, especially list building or workflows where the number of entities can vary significantly from request to request.

| Effort    | Best for                                                           | Suggested schema complexity                                    | Runtime expectation        |
| --------- | ------------------------------------------------------------------ | -------------------------------------------------------------- | -------------------------- |
| `minimal` | Lowest-cost lookups, very narrow factual tasks, short answers      | One or two fields, shallow schema                              | Cheapest, least exhaustive |
| `low`     | Simple lookups, narrow factual tasks, short answers                | A few fields, shallow schema                                   | Fast, light research       |
| `medium`  | Default starting point for most standard research tasks            | Moderate field count, simple nested objects                    | Balanced quality/runtime   |
| `high`    | Harder research, more citations, stricter completeness             | Larger schemas or more nuanced fields                          | Slower, more thorough      |
| `xhigh`   | High-value tasks where completeness matters more than cost/latency | Complex schemas, many fields, difficult verification           | Slowest, most thorough     |
| `auto`    | Variable-scope work, list building, unknown task difficulty        | Flexible; useful when entity count or work required is unknown | Variable                   |

Use `medium` as the default starting point for standard single-entity research tasks. Move down to `low` or `minimal` when cost and latency matter more than completeness. Move up to `high` or `xhigh` when the output schema is larger, fields require verification, or the task needs deeper reasoning. Use `auto` when the task scope is not known ahead of time, such as list building or workflows where one request may return many entities.

Runtime varies by query difficulty, schema complexity, and external source availability. Treat effort modes as quality/cost/runtime tradeoffs rather than strict latency guarantees.

<Note>
  Exa Agent is not ZDR. If you require ZDR, [reach out to us](mailto:sales@exa.ai).
</Note>

## Next steps

* [Create a run](/reference/agent-api/create-a-run)
* [Get a run](/reference/agent-api/get-a-run)
* [List runs](/reference/agent-api/list-runs)
* [Read the Agent guide](/reference/agent-api-guide)
