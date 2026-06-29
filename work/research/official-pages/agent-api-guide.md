> ## Documentation Index
> Fetch the complete documentation index at: https://exa.ai/docs/llms.txt
> Use this file to discover all available pages before exploring further.

# Exa Agent

> Run deep research, list-building, and enrichment workflows that return structured outputs.

Exa Agent is an async, high-compute, usage-based endpoint that handles list building, enrichment, and deep research tasks that require dozens of structured output fields and complex reasoning.

Each run can return a natural-language answer, schema-validated JSON, field-level grounding, metadata, and a cost breakdown. You can retrieve completed runs later, list past runs, replay events, or continue from a previously completed run.

<Tip>
  Prefer MCP? Use the [Exa Agent tools in Exa MCP](/reference/exa-mcp#exa-agent) to run Agent — including [Exa Connect](/reference/agent-api/connect/overview) data sources — from Claude, Cursor, and any other MCP client.
</Tip>

## When to use Exa Agent

Use Exa Agent when a workflow needs more than a single search or extraction call:

* Build lists from open-ended criteria, then enrich each result
* Research entities across many fields with citations
* Run multi-hop tasks like "find companies, then find their decision makers"
* Produce structured JSON from a long-running web research task
* Continue from a previous run with a follow-up request like "find 10 more results"

For simpler low-latency search, start with the [Search API](/reference/search-api-guide).

## Quickstart

This example starts a run that builds a structured list of people matching your criteria. It returns JSON in `output.structured`.

### 1. Install the Exa SDK

<CodeGroup>
  ```bash Python theme={null}
  pip install exa-py
  ```

  ```bash TypeScript theme={null}
  npm install exa-js
  ```
</CodeGroup>

### 2. Set your API key

<Tabs>
  <Tab title="macOS/Linux">
    ```bash theme={null}
    export EXA_API_KEY="your-api-key"
    ```
  </Tab>

  <Tab title="Windows">
    ```powershell theme={null}
    setx EXA_API_KEY "your-api-key"
    ```
  </Tab>
</Tabs>

### 3. Create a run

<CodeGroup>
  ```python Python theme={null}
  import json
  from exa_py import Exa

  exa = Exa()
  run = exa.agent.runs.create(
      query="Find engineering leaders at AI infrastructure companies that raised a Series A or B in the last 6 months.",
      output_schema={
          "type": "object",
          "properties": {
              "people": {
                  "type": "array",
                  "maxItems": 10,
                  "items": {
                      "type": "object",
                      "properties": {
                          "name": {"type": "string"},
                          "job_title": {"type": "string"},
                          "linkedin_url": {"type": "string", "format": "uri"},
                      },
                      "required": ["name", "job_title", "linkedin_url"],
                  },
              }
          },
          "required": ["people"],
      },
      effort="auto",
  )

  print(json.dumps(run.model_dump(), indent=2))
  ```

  ```typescript TypeScript theme={null}
  import Exa from "exa-js";

  const exa = new Exa();
  const run = await exa.agent.runs.create({
    query:
      "Find engineering leaders at AI infrastructure companies that raised a Series A or B in the last 6 months.",
    outputSchema: {
      type: "object",
      properties: {
        people: {
          type: "array",
          maxItems: 10,
          items: {
            type: "object",
            properties: {
              name: { type: "string" },
              job_title: { type: "string" },
              linkedin_url: { type: "string", format: "uri" }
            },
            required: ["name", "job_title", "linkedin_url"]
          }
        }
      },
      required: ["people"]
    },
    effort: "auto"
  });

  console.log(JSON.stringify(run, null, 2));
  ```

  ```bash cURL theme={null}
  curl -s -X POST "https://api.exa.ai/agent/runs" \
    -H "Content-Type: application/json" \
    -H "x-api-key: $EXA_API_KEY" \
    -d '{
      "query": "Find engineering leaders at AI infrastructure companies that raised a Series A or B in the last 6 months.",
      "effort": "auto",
      "outputSchema": {
        "type": "object",
        "properties": {
          "people": {
            "type": "array",
            "maxItems": 10,
            "items": {
              "type": "object",
              "properties": {
                "name": { "type": "string" },
                "job_title": { "type": "string" },
                "linkedin_url": { "type": "string", "format": "uri" }
              },
              "required": ["name", "job_title", "linkedin_url"]
            }
          }
        },
        "required": ["people"]
      }
    }'
  ```
</CodeGroup>

Add `Accept: text/event-stream` when creating a run to receive server-sent events as the run is queued, started, and completed:

<CodeGroup>
  ```python Python theme={null}
  from exa_py import Exa

  exa = Exa()
  events = exa.agent.runs.create(
      query="Find five recently launched developer tools for evaluating AI agents.",
      stream=True,
  )

  for event in events:
      print(event.event, event.data)
  ```

  ```typescript TypeScript theme={null}
  import Exa from "exa-js";

  const exa = new Exa();
  const events = await exa.agent.runs.create({
    query: "Find five recently launched developer tools for evaluating AI agents.",
    stream: true
  });

  for await (const event of events) {
    console.log(event.event, event.data);
  }
  ```
</CodeGroup>

```text theme={null}
id: 1
event: agent_run.created
data: {"id":"agent_run_01j...","status":"queued","createdAt":"2026-05-07T21:21:52.051Z"}

id: 2
event: agent_run.started
data: {"id":"agent_run_01j...","status":"running"}

id: 3
event: agent_run.completed
data: {"id":"agent_run_01j...","object":"agent_run","status":"completed"}
```

### 4. Poll for completion

If you do not stream events, save the returned `id` and poll the run until it reaches a terminal status.

<CodeGroup>
  ```python Python theme={null}
  import json
  from exa_py import Exa

  exa = Exa()
  run_id = "agent_run_01j..."
  run = exa.agent.runs.poll_until_finished(
      run_id,
      poll_interval=4000,
  )

  print(json.dumps(run.model_dump(), indent=2))
  ```

  ```typescript TypeScript theme={null}
  import Exa from "exa-js";

  const exa = new Exa();
  const runId = "agent_run_01j...";
  const run = await exa.agent.runs.pollUntilFinished(runId, {
    pollInterval: 4000
  });

  console.log(JSON.stringify(run, null, 2));
  ```

  ```bash cURL theme={null}
  RUN_ID="agent_run_01j..."

  while true; do
    RUN_JSON="$(curl -s "https://api.exa.ai/agent/runs/$RUN_ID" \
      -H "x-api-key: $EXA_API_KEY")"

    STATUS="$(echo "$RUN_JSON" | jq -r '.status')"
    echo "status=$STATUS"

    if [ "$STATUS" = "completed" ] || [ "$STATUS" = "failed" ] || [ "$STATUS" = "cancelled" ]; then
      echo "$RUN_JSON" | jq .
      break
    fi

    sleep 4
  done
  ```
</CodeGroup>

Completed runs include:

* `output.text`: a natural-language answer
* `output.structured`: validated JSON when you provide `outputSchema`
* `output.grounding`: citations for text or structured fields, when emitted
* `costDollars`: the run's cost breakdown

## Return structured JSON

Use `outputSchema` when you need `/agent` to return in specific format. When you specify an `outputSchema`, the returned object will contain an output matching your `outputSchema` in `output.structured`.

`outputSchema` supports the [JSON Schema specification](https://json-schema.org/).

To request contact information, describe the desired contact fields in `outputSchema`. Use standard JSON Schema shapes such as `{ "type": "string", "format": "email" }` for email addresses, `{ "type": "string", "format": "phone" }` for phone numbers, and `{ "type": "string", "format": "uri" }` for URLs. Bound list sizes with `maxItems` when possible so the maximum contact-enrichment cost is predictable.

<CodeGroup>
  ```python Python theme={null}
  import json
  from exa_py import Exa

  exa = Exa()
  run = exa.agent.runs.create(
      query="Find AI infrastructure companies that raised a Series A or B in the last 6 months.",
      effort="auto",
      output_schema={
          "type": "object",
          "properties": {
              "companies": {
                  "type": "array",
                  "items": {
                      "type": "object",
                      "properties": {
                          "name": {"type": "string"},
                          "round": {"type": "string"},
                          "website": {"type": "string"},
                      },
                      "required": ["name", "round"],
                  },
              }
          },
          "required": ["companies"],
      },
  )
  run = exa.agent.runs.poll_until_finished(
      run.id,
  )

  print(json.dumps(run.output.structured if run.output else None, indent=2))
  ```

  ```typescript TypeScript theme={null}
  import Exa from "exa-js";

  const exa = new Exa();
  const run = await exa.agent.runs.create({
    query:
      "Find AI infrastructure companies that raised a Series A or B in the last 6 months.",
    effort: "auto",
    outputSchema: {
      type: "object",
      properties: {
        companies: {
          type: "array",
          items: {
            type: "object",
            properties: {
              name: { type: "string" },
              round: { type: "string" },
              website: { type: "string" }
            },
            required: ["name", "round"]
          }
        }
      },
      required: ["companies"]
    }
  });
  const completedRun = await exa.agent.runs.pollUntilFinished(run.id);

  console.log(JSON.stringify(completedRun.output?.structured, null, 2));
  ```

  ```bash cURL theme={null}
  curl -s -X POST "https://api.exa.ai/agent/runs" \
    -H "Content-Type: application/json" \
    -H "x-api-key: $EXA_API_KEY" \
    -d '{
      "query": "Find AI infrastructure companies that raised a Series A or B in the last 6 months.",
      "effort": "auto",
      "outputSchema": {
        "type": "object",
        "properties": {
          "companies": {
            "type": "array",
            "items": {
              "type": "object",
              "properties": {
                "name": { "type": "string" },
                "round": { "type": "string" },
                "website": { "type": "string" }
              },
              "required": ["name", "round"]
            }
          }
        },
        "required": ["companies"]
      }
    }'
  ```
</CodeGroup>

## Process input rows

Use `input.data` when you have an existing set of data that you want to enrich. You can add more fields to each data entity, surface more entities based on the data you bring in, or both.

For complete row-enrichment examples, see [Agent examples](/reference/agent-api/examples#enrich-input-rows).

## Process exclusions

Use `input.exclusion` to exclude certain entries from being surfaced in the run. In the example below, we want to look for the top 10 cutest animals, but we exclude goats and pandas from the run because we already know how cute they are.

<CodeGroup>
  ```python Python theme={null}
  import json
  from exa_py import Exa

  exa = Exa()
  run = exa.agent.runs.create(
      query="Find the top 10 cutest animals. Return each animal's common name and a source URL.",
      input={
          "exclusion": [
              {"animal": "goat"},
              {"animal": "panda"},
          ]
      },
  )

  print(json.dumps(run.model_dump(), indent=2))
  ```

  ```typescript TypeScript theme={null}
  import Exa from "exa-js";

  const exa = new Exa();
  const run = await exa.agent.runs.create({
    query: "Find the top 10 cutest animals. Return each animal's common name and a source URL.",
    input: {
      exclusion: [
        { animal: "goat" },
        { animal: "panda" }
      ]
    }
  });

  console.log(JSON.stringify(run, null, 2));
  ```

  ```bash cURL theme={null}
  curl -s -X POST "https://api.exa.ai/agent/runs" \
    -H "Content-Type: application/json" \
    -H "x-api-key: $EXA_API_KEY" \
    -d '{
      "query": "Find the top 10 cutest animals. Return each animal'"'"'s common name and a source URL.",
      "input": {
        "exclusion": [
          { "animal": "goat" },
          { "animal": "panda" }
        ]
      }
    }'
  ```
</CodeGroup>

## Connect data sources

Use `dataSources` to attach premium data partners to a run. Each entry selects a `provider`. When a property in your `outputSchema` references a specific source (e.g., "from Similarweb"), Exa Agent calls the matching provider tool instead of a generic web search.

```json theme={null}
{
  "dataSources": [
    { "provider": "similarweb" },
    { "provider": "fiber_ai" }
  ]
}
```

See [Exa Connect](/reference/agent-api/connect/overview) for the full list of data partners, with examples for each.

## Continue from a previous run

Use `previousRunId` to ask follow-ups to the run's previous response. Follow-up runs will share the same run ID as the `previousRunId` supplied.

<CodeGroup>
  ```python Python theme={null}
  import json
  from exa_py import Exa

  exa = Exa()
  run = exa.agent.runs.create(
      query="Narrow that list to companies hiring in San Francisco.",
      previous_run_id="agent_run_01j...",
  )

  print(json.dumps(run.model_dump(), indent=2))
  ```

  ```typescript TypeScript theme={null}
  import Exa from "exa-js";

  const exa = new Exa();
  const run = await exa.agent.runs.create({
    query: "Narrow that list to companies hiring in San Francisco.",
    previousRunId: "agent_run_01j..."
  });

  console.log(JSON.stringify(run, null, 2));
  ```

  ```bash cURL theme={null}
  curl -s -X POST "https://api.exa.ai/agent/runs" \
    -H "Content-Type: application/json" \
    -H "x-api-key: $EXA_API_KEY" \
    -d '{
      "query": "Narrow that list to companies hiring in San Francisco.",
      "previousRunId": "agent_run_01j..."
    }'
  ```
</CodeGroup>

## Find a run ID

List recent runs and inspect their statuses:

<CodeGroup>
  ```python Python theme={null}
  from exa_py import Exa

  exa = Exa()
  runs = exa.agent.runs.list(
      limit=10,
  )

  for run in runs.data:
      query = (run.request or {}).get("query", "")
      print(f"{run.id}\t{run.status}\t{run.created_at}\t{query}")
  ```

  ```typescript TypeScript theme={null}
  import Exa from "exa-js";

  const exa = new Exa();
  const list = await exa.agent.runs.list({
    limit: 10
  });

  for (const run of list.data) {
    const query = run.request?.query ?? "";
    console.log(`${run.id}\t${run.status}\t${run.createdAt}\t${query}`);
  }
  ```

  ```bash cURL theme={null}
  curl -s "https://api.exa.ai/agent/runs?limit=10" \
    -H "x-api-key: $EXA_API_KEY" \
    | jq -r '.data[] | "\(.id)\t\(.status)\t\(.createdAt)\t\(.request.query)"'
  ```
</CodeGroup>

## Pricing

Costs are usage-based and priced by component:

| Component           | Price             |
| ------------------- | ----------------- |
| Agent Compute Units | `1 ACU = $0.10`   |
| Search tool calls   | `$0.005 / search` |

<Note>
  Contact enrichment is separate from the core pricing components above: email contact enrichment is `$0.02 / email`, and phone number contact enrichment is `$0.07 / phone number`.
</Note>

`usage.agentComputeUnits` measures model computation across the full run. More complex queries, or queries that contain a large `input.data` field will generally take more reasoning steps and make more tool calls, and will generally consumer more ACUs.

Your Agent concurrency limit is one fifth of your account QPS. For pay-as-you-go accounts with default QPS, this means two active Agent runs at a time.

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

## Next

* [Agent reference](/reference/agent-api/overview)
* [Agent examples](/reference/agent-api/examples)
* [Search API guide](/reference/search-api-guide)
