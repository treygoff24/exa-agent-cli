> ## Documentation Index
> Fetch the complete documentation index at: https://exa.ai/docs/llms.txt
> Use this file to discover all available pages before exploring further.

# List run events

> List stored Agent run events or replay them as server-sent events.

By default, this endpoint returns a paginated JSON list of stored events. Set `Accept: text/event-stream` to replay stored events as SSE. For JSON pagination, use `cursor`. For SSE replay, use `Last-Event-ID`.

<Card title="Get your Exa API key" icon="key" horizontal href="https://dashboard.exa.ai/api-keys" />


## OpenAPI

````yaml get /agent/runs/{id}/events
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
  /agent/runs/{id}/events:
    get:
      tags:
        - Agent
      summary: List run events
      description: >-
        List stored events for an Agent run. Set `Accept: text/event-stream` to
        replay stored events as server-sent events. Use `cursor` for JSON
        pagination or `Last-Event-ID` for SSE replay.
      operationId: listAgentRunEvents
      parameters:
        - in: path
          name: id
          schema:
            $ref: '#/components/schemas/AgentRunId'
            description: Agent run ID.
          required: true
          description: Agent run ID.
        - in: query
          name: limit
          schema:
            type: integer
            minimum: 1
            maximum: 100
            description: Number of results per page
            default: 20
        - in: query
          name: cursor
          schema:
            type: string
            description: >-
              Cursor for pagination. Use the `nextCursor` value from the
              previous event list response.
        - $ref: '#/components/parameters/AcceptHeader'
        - $ref: '#/components/parameters/LastEventId'
      responses:
        '200':
          description: Agent run events
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/AgentRunEventList'
            text/event-stream:
              schema:
                $ref: '#/components/schemas/AgentRunEvent'
              examples:
                replay:
                  summary: Replayed run events
                  value: >
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
        '404':
          description: Run not found.
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
          label: List run events
          source: |-
            from exa_py import Exa

            exa = Exa(api_key="YOUR_EXA_API_KEY")
            run_id = "agent_run_01j..."
            events = exa.agent.runs.events.list(
                run_id,
                limit=20,
            )
            print(events)
        - lang: typescript
          label: List run events
          source: |-
            import Exa from "exa-js";

            const exa = new Exa();
            const runId = "agent_run_01j...";
            const events = await exa.agent.runs.events.list(runId, {
              limit: 20
            });

            console.log(events);
        - lang: bash
          label: List run events
          source: >-
            curl -s
            "https://api.exa.ai/agent/runs/agent_run_01j.../events?limit=20" \
              -H "x-api-key: $EXA_API_KEY"
        - lang: bash
          label: Replay run events
          source: |-
            curl -N "https://api.exa.ai/agent/runs/agent_run_01j.../events" \
              -H "Accept: text/event-stream" \
              -H "Last-Event-ID: 1" \
              -H "x-api-key: $EXA_API_KEY"
components:
  schemas:
    AgentRunId:
      type: string
      minLength: 1
      maxLength: 200
      pattern: ^[A-Za-z0-9_.:-]+$
      description: Agent run ID. New run IDs are returned with the `agent_run_` prefix.
      example: agent_run_01j7x9v0m2n4p6q8r0s2t4v6w8
    AgentRunEventList:
      type: object
      properties:
        object:
          type: string
          const: list
        data:
          type: array
          items:
            $ref: '#/components/schemas/AgentRunEvent'
        hasMore:
          type: boolean
          description: Whether there are more results
        nextCursor:
          anyOf:
            - type: string
            - type: 'null'
      required:
        - object
        - data
        - hasMore
        - nextCursor
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
    LastEventId:
      in: header
      name: Last-Event-ID
      schema:
        description: For SSE replay, return only events after this event ID.
        type: string
      description: For SSE replay, return only events after this event ID.
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