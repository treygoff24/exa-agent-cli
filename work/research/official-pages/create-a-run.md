> ## Documentation Index
> Fetch the complete documentation index at: https://exa.ai/docs/llms.txt
> Use this file to discover all available pages before exploring further.

# Create a run

> Create an asynchronous Agent run. The response returns the run object immediately unless you request server-sent events.

Create a run with a natural-language `query`. Add `outputSchema` for validated structured JSON, `input.data` for rows to process, `input.exclusion` for records or entities to avoid, or `previousRunId` to continue from a completed run.

Set `Accept: text/event-stream` to stream run events as the run is created, started, and completed.

<Note>
  **Connect:** Pass `dataSources` to give the agent access to third-party data providers during the run. See the [Connect guide](/reference/agent-api-guide#connect-data-sources) for details.
</Note>

<Card title="Get your Exa API key" icon="key" horizontal href="https://dashboard.exa.ai/api-keys" />


## OpenAPI

````yaml post /agent/runs
openapi: 3.1.0
info:
  title: Exa Public API
  version: 2.0.0
servers:
  - url: https://api.exa.ai
security:
  - apiKey: []
  - bearer: []
tags: []
paths:
  /agent/runs:
    post:
      tags:
        - Agent
      summary: Create a run
      description: >-
        Create an asynchronous Agent run. By default, the API returns the run
        object immediately. Set `Accept: text/event-stream` to stream run
        lifecycle events until the run reaches a terminal status.
      operationId: createAgentRun
      parameters:
        - $ref: '#/components/parameters/AcceptHeader'
        - $ref: '#/components/parameters/ExaBetaHeader'
      requestBody:
        required: true
        content:
          application/json:
            schema:
              $ref: '#/components/schemas/CreateAgentRunRequest'
            examples:
              simple:
                summary: Simple research run
                value:
                  query: >-
                    What are the most important AI infrastructure funding rounds
                    announced this week?
              structuredOutput:
                summary: Structured output
                value:
                  query: >-
                    Find recent Series A or Series B AI infrastructure funding
                    rounds.
                  outputSchema:
                    type: object
                    required:
                      - companies
                    properties:
                      companies:
                        type: array
                        maxItems: 10
                        items:
                          type: object
                          required:
                            - name
                            - round
                            - amount
                            - sourceUrl
                          properties:
                            name:
                              type: string
                            round:
                              type: string
                            amount:
                              type: string
                            sourceUrl:
                              type: string
                              format: uri
              inputRows:
                summary: Process input rows
                value:
                  query: >-
                    For each company, find one current executive and cite a
                    source.
                  input:
                    data:
                      - company: Apple
                        domain: apple.com
                      - company: Microsoft
                        domain: microsoft.com
                    exclusion:
                      - company: Apple
                        person: Tim Cook
              contactFields:
                summary: Contact fields in structured output
                value:
                  query: >-
                    Find engineering leaders at AI infrastructure companies that
                    raised a Series A or B in the last 6 months.
                  effort: auto
                  outputSchema:
                    type: object
                    required:
                      - people
                    properties:
                      people:
                        type: array
                        maxItems: 10
                        items:
                          type: object
                          required:
                            - name
                            - linkedin_url
                          properties:
                            name:
                              type: string
                            contact_email:
                              type: string
                              format: email
                            linkedin_url:
                              type: string
                              format: uri
              dataSources:
                summary: Connect data sources
                value:
                  query: >-
                    Find 10 fast-growing B2B SaaS companies and their estimated
                    web traffic.
                  dataSources:
                    - provider: similarweb
                  outputSchema:
                    type: object
                    required:
                      - companies
                    properties:
                      companies:
                        type: array
                        maxItems: 10
                        items:
                          type: object
                          required:
                            - name
                            - domain
                            - monthlyVisits
                          properties:
                            name:
                              type: string
                            domain:
                              type: string
                            monthlyVisits:
                              type: number
                              description: from Similarweb
      responses:
        '200':
          description: Agent run created
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/AgentRun'
              examples:
                running:
                  summary: Run accepted
                  value:
                    id: agent_run_01j7x9v0m2n4p6q8r0s2t4v6w8
                    object: agent_run
                    status: running
                    stopReason: null
                    createdAt: '2026-05-07T18:31:00.000Z'
                    completedAt: null
                    request:
                      query: >-
                        What are the most important AI infrastructure funding
                        rounds announced this week?
                    output:
                      text: ''
                      structured: null
                      grounding: []
                    usage:
                      agentComputeUnits: 0
                      searches: 0
                      emails: 0
                      phoneNumbers: 0
                    costDollars:
                      total: 0
                      agentCompute: 0
                      search: 0
                      emails: 0
                      phoneNumbers: 0
            text/event-stream:
              schema:
                $ref: '#/components/schemas/AgentRunEvent'
              examples:
                lifecycle:
                  summary: Run lifecycle events
                  value: >-
                    id: 1

                    event: agent_run.created

                    data:
                    {"id":"agent_run_01j...","status":"queued","createdAt":"2026-05-07T21:21:52.051Z"}


                    id: 2

                    event: agent_run.started

                    data: {"id":"agent_run_01j...","status":"running"}


                    id: 3

                    event: agent_run.completed

                    data:
                    {"id":"agent_run_01j...","object":"agent_run","status":"completed"}
        '400':
          description: Invalid request.
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/AgentErrorResponse'
        '401':
          description: Team context or authentication was not found.
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/AgentErrorResponse'
        '429':
          description: Agent run concurrency limit reached.
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/AgentErrorResponse'
        '500':
          description: Server error or run timeout.
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/AgentErrorResponse'
      x-codeSamples:
        - lang: python
          label: Create a run
          source: |-
            from exa_py import Exa

            exa = Exa(api_key="YOUR_EXA_API_KEY")
            run = exa.agent.runs.create(
                query="What are the most important AI infrastructure funding rounds announced this week?",
            )
            print(run)
        - lang: typescript
          label: Create a run
          source: |-
            import Exa from "exa-js";

            const exa = new Exa();
            const run = await exa.agent.runs.create({
              query: "What are the most important AI infrastructure funding rounds announced this week?"
            });

            console.log(run);
        - lang: bash
          label: Create a run
          source: |-
            curl -s -X POST "https://api.exa.ai/agent/runs" \
              -H "Content-Type: application/json" \
              -H "x-api-key: $EXA_API_KEY" \
              -d '{
                "query": "What are the most important AI infrastructure funding rounds announced this week?"
              }'
        - lang: bash
          label: Stream a run
          source: |-
            curl -N -X POST "https://api.exa.ai/agent/runs" \
              -H "Content-Type: application/json" \
              -H "Accept: text/event-stream" \
              -H "x-api-key: $EXA_API_KEY" \
              -d '{
                "query": "Find five recently launched developer tools for evaluating AI agents."
              }'
components:
  parameters:
    AcceptHeader:
      in: header
      name: Accept
      schema:
        description: Set to `text/event-stream` to receive server-sent events.
        type: string
        enum:
          - application/json
          - text/event-stream
      description: Set to `text/event-stream` to receive server-sent events.
    ExaBetaHeader:
      in: header
      name: Exa-Beta
      schema:
        description: >-
          Comma-separated beta feature tokens for opting into experimental
          features.
        type: string
      description: >-
        Comma-separated beta feature tokens for opting into experimental
        features.
  schemas:
    CreateAgentRunRequest:
      type: object
      properties:
        query:
          type: string
          minLength: 1
          description: Natural-language question or instructions for the request.
          example: >-
            What are the most important AI infrastructure funding rounds
            announced this week?
        systemPrompt:
          type: string
          description: >-
            Additional instructions that guide generated output or agent
            behavior. Use this for source preferences, novelty constraints,
            duplication constraints, or other behavior guidance.
          example: Prefer official sources and avoid duplicate results.
        input:
          type: object
          properties:
            data:
              type: array
              items:
                type: object
                propertyNames:
                  type: string
                additionalProperties:
                  $ref: '#/components/schemas/JsonValue'
                description: A JSON object record.
              description: Records the agent should process or enrich.
            exclusion:
              type: array
              items:
                type: object
                propertyNames:
                  type: string
                additionalProperties:
                  $ref: '#/components/schemas/JsonValue'
                description: A JSON object record.
              description: Records or entities the agent should avoid returning.
          description: >-
            Records to process and records or entities to exclude from the
            answer.
        outputSchema:
          anyOf:
            - type: object
              propertyNames:
                type: string
              additionalProperties:
                $ref: '#/components/schemas/JsonValue'
              description: >-
                JSON Schema for validated structured output in
                `output.structured`. Supports draft-07, 2019-09, and 2020-12 via
                `$schema`.
            - type: 'null'
        effort:
          $ref: '#/components/schemas/AgentEffort'
        previousRunId:
          $ref: '#/components/schemas/AgentRunId'
          description: Completed run ID to continue from. Must belong to the same team.
        metadata:
          type: object
          propertyNames:
            type: string
          additionalProperties:
            type: string
          description: Caller-provided metadata stored with the run.
          example:
            slack_channel_id: C123ABC
            slack_thread_id: '1745444400.123456'
            user_id: U123ABC
        dataSources:
          maxItems: 5
          type: array
          items:
            $ref: '#/components/schemas/AgentDataSource'
          description: >-
            Exa Connect data providers to enable for the run. Each entry enables
            all of that provider's tools.
        budget:
          type: object
          properties:
            maxCostDollars:
              type: number
              minimum: 0
              description: Accepted for compatibility and currently ignored.
          description: Accepted for compatibility and currently ignored.
          deprecated: true
      required:
        - query
    AgentRun:
      type: object
      properties:
        id:
          $ref: '#/components/schemas/AgentRunId'
        object:
          type: string
          const: agent_run
        status:
          $ref: '#/components/schemas/AgentRunStatus'
        stopReason:
          anyOf:
            - $ref: '#/components/schemas/AgentStopReason'
            - type: 'null'
          description: Why the run stopped. `null` while the run is queued or running.
        createdAt:
          type: string
          format: date-time
          description: When the run was created
        completedAt:
          anyOf:
            - type: string
              format: date-time
            - type: 'null'
          format: date-time
        request:
          anyOf:
            - $ref: '#/components/schemas/AgentRunRequest'
            - type: 'null'
        output:
          $ref: '#/components/schemas/AgentRunOutput'
        usage:
          $ref: '#/components/schemas/AgentUsage'
        costDollars:
          $ref: '#/components/schemas/AgentCostDollars'
      required:
        - id
        - object
        - status
        - stopReason
        - createdAt
        - completedAt
        - request
        - output
        - usage
        - costDollars
      additionalProperties: false
    AgentRunEvent:
      type: object
      properties:
        id:
          type: string
          description: Event ID within the run.
        event:
          type: string
          enum:
            - agent_run.created
            - agent_run.started
            - agent_run.completed
            - agent_run.failed
            - agent_run.cancelled
        data:
          $ref: '#/components/schemas/JsonValue'
        createdAt:
          type: string
          format: date-time
          description: When the event was created
      required:
        - id
        - event
        - data
        - createdAt
      additionalProperties: false
    AgentErrorResponse:
      type: object
      properties:
        error:
          $ref: '#/components/schemas/AgentError'
      required:
        - error
      additionalProperties: false
    JsonValue:
      description: Any JSON value.
      oneOf:
        - type: 'null'
        - type: boolean
        - type: number
        - type: string
        - type: array
          items:
            $ref: '#/components/schemas/JsonValue'
        - type: object
          propertyNames:
            type: string
          additionalProperties:
            $ref: '#/components/schemas/JsonValue'
    AgentEffort:
      type: string
      enum:
        - minimal
        - low
        - medium
        - high
        - xhigh
        - auto
      description: >-
        Cost and reasoning effort preference for the run. `auto` lets Exa choose
        the appropriate effort.
      default: auto
    AgentRunId:
      type: string
      minLength: 1
      maxLength: 200
      pattern: ^[A-Za-z0-9_.:-]+$
      description: Agent run ID. New run IDs are returned with the `agent_run_` prefix.
      example: agent_run_01j7x9v0m2n4p6q8r0s2t4v6w8
    AgentDataSource:
      type: object
      properties:
        provider:
          $ref: '#/components/schemas/AgentDataSourceProvider'
          description: >-
            Exa Connect data provider to enable for the run. All provider tools
            are available by default.
          example: fiber_ai
      required:
        - provider
    AgentRunStatus:
      type: string
      enum:
        - queued
        - running
        - completed
        - failed
        - cancelled
    AgentStopReason:
      type: string
      enum:
        - schema_satisfied
        - budget_reached
        - error
        - cancelled
    AgentRunRequest:
      type: object
      properties:
        query:
          type: string
          minLength: 1
          description: Natural-language question or instructions for the request.
          example: >-
            What are the most important AI infrastructure funding rounds
            announced this week?
        systemPrompt:
          type: string
          description: >-
            Additional instructions that guide generated output or agent
            behavior. Use this for source preferences, novelty constraints,
            duplication constraints, or other behavior guidance.
          example: Prefer official sources and avoid duplicate results.
        input:
          type: object
          properties:
            data:
              type: array
              items:
                type: object
                propertyNames:
                  type: string
                additionalProperties:
                  $ref: '#/components/schemas/JsonValue'
                description: A JSON object record.
              description: Records the agent should process or enrich.
            exclusion:
              type: array
              items:
                type: object
                propertyNames:
                  type: string
                additionalProperties:
                  $ref: '#/components/schemas/JsonValue'
                description: A JSON object record.
              description: Records or entities the agent should avoid returning.
          additionalProperties: false
        outputSchema:
          anyOf:
            - type: object
              propertyNames:
                type: string
              additionalProperties:
                $ref: '#/components/schemas/JsonValue'
              description: >-
                JSON Schema for validated structured output in
                `output.structured`. Supports draft-07, 2019-09, and 2020-12 via
                `$schema`.
            - type: 'null'
        effort:
          $ref: '#/components/schemas/AgentEffort'
        previousRunId:
          $ref: '#/components/schemas/AgentRunId'
        metadata:
          type: object
          propertyNames:
            type: string
          additionalProperties:
            type: string
          description: Caller-provided key-value metadata for your own tracking.
          example:
            slack_channel_id: C123ABC
            slack_thread_id: '1745444400.123456'
            user_id: U123ABC
        dataSources:
          type: array
          items:
            $ref: '#/components/schemas/AgentDataSourceOutput'
          description: Exa Connect data providers configured for the run.
      additionalProperties:
        $ref: '#/components/schemas/JsonValue'
      description: Canonicalized request fields stored with the run.
    AgentRunOutput:
      type: object
      properties:
        text:
          type: string
          description: Natural-language answer or summary.
        structured:
          anyOf:
            - $ref: '#/components/schemas/JsonValue'
            - type: 'null'
          description: >-
            Validated JSON matching `outputSchema`, or `null` when no schema was
            provided.
        grounding:
          type: array
          items:
            $ref: '#/components/schemas/AgentGrounding'
          description: Field-level citations emitted by the run.
      required:
        - text
        - structured
        - grounding
      additionalProperties: false
    AgentUsage:
      type: object
      properties:
        agentComputeUnits:
          type: number
          minimum: 0
        searches:
          type: integer
          minimum: 0
        emails:
          type: integer
          minimum: 0
        phoneNumbers:
          type: integer
          minimum: 0
        dataSources:
          $ref: '#/components/schemas/AgentDataSourceUsage'
      required:
        - agentComputeUnits
        - searches
        - emails
        - phoneNumbers
      additionalProperties: false
    AgentCostDollars:
      type: object
      properties:
        total:
          type: number
          minimum: 0
        agentCompute:
          type: number
          minimum: 0
        search:
          type: number
          minimum: 0
        emails:
          type: number
          minimum: 0
        phoneNumbers:
          type: number
          minimum: 0
        dataSources:
          $ref: '#/components/schemas/AgentDataSourceCost'
      required:
        - total
        - agentCompute
        - search
        - emails
        - phoneNumbers
      additionalProperties: false
    AgentError:
      type: object
      properties:
        type:
          type: string
          enum:
            - INVALID_REQUEST
            - AUTHENTICATION_ERROR
            - RATE_LIMIT_ERROR
            - NOT_FOUND
            - SERVER_ERROR
        code:
          type: string
          enum:
            - INVALID_REQUEST
            - TEAM_NOT_FOUND
            - RUN_NOT_FOUND
            - PREVIOUS_RUN_NOT_FOUND
            - PREVIOUS_RUN_NOT_COMPLETED
            - CONCURRENCY_LIMIT_REACHED
            - INVALID_OUTPUT_SCHEMA
            - INVALID_DATA_SOURCE
            - TIMEOUT
            - SERVER_ERROR
        message:
          type: string
      required:
        - type
        - code
        - message
      additionalProperties:
        $ref: '#/components/schemas/JsonValue'
    AgentDataSourceProvider:
      type: string
      enum:
        - fiber_ai
        - financial_datasets
        - similarweb
        - baselayer
        - affiliate
        - particle_news
        - jinko
      description: Identifier of an Exa Connect data provider.
    AgentDataSourceOutput:
      type: object
      properties:
        provider:
          $ref: '#/components/schemas/AgentDataSourceProvider'
          description: >-
            Exa Connect data provider to enable for the run. All provider tools
            are available by default.
          example: fiber_ai
      required:
        - provider
      additionalProperties: false
    AgentGrounding:
      type: object
      properties:
        field:
          type: string
          description: Output field the citations support.
          example: structured.companies[0].sourceUrl
        citations:
          type: array
          items:
            $ref: '#/components/schemas/AgentCitation'
        confidence:
          anyOf:
            - type: string
              enum:
                - low
                - medium
                - high
              description: Model-reported reliability for this field.
            - type: 'null'
      required:
        - field
        - citations
      additionalProperties: false
    AgentDataSourceUsage:
      type: object
      propertyNames:
        type: string
      additionalProperties:
        type: integer
        minimum: 0
      description: >-
        Per-provider tool call counts for Exa Connect data sources used during
        the run. Keys are provider names (e.g. `fiber_ai`, `similarweb`). Only
        providers with non-zero usage are included.
    AgentDataSourceCost:
      type: object
      propertyNames:
        type: string
      additionalProperties:
        type: number
        minimum: 0
      description: >-
        Per-provider cost in dollars for Exa Connect data sources used during
        the run. Keys are provider names (e.g. `fiber_ai`, `similarweb`). Only
        providers with non-zero usage are included.
    AgentCitation:
      type: object
      properties:
        url:
          type: string
          format: uri
          description: Source URL.
        title:
          type: string
          description: Source title.
      required:
        - url
      additionalProperties: false
  securitySchemes:
    apiKey:
      type: apiKey
      name: x-api-key
      in: header
      description: >-
        Pass your Exa API key in the x-api-key header. You can also authenticate
        with Authorization: Bearer <key>.
    bearer:
      type: http
      scheme: bearer
      description: >-
        Pass your Exa API key in the x-api-key header. You can also authenticate
        with Authorization: Bearer <key>.

````