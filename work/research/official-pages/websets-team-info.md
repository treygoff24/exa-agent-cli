> ## Documentation Index
> Fetch the complete documentation index at: https://exa.ai/docs/llms.txt
> Use this file to discover all available pages before exploring further.

# Get Team Info

> Retrieve information about your team including concurrency usage and limits.

## Overview

The Get Team Info endpoint returns information about the authenticated team, including the team's current concurrency usage and configured limits. This is useful for monitoring your Websets API usage and understanding your rate limits.

## Response

The response includes:

* **object**: Always "team"
* **id**: Your team's unique identifier
* **name**: Your team's name
* **concurrency**: Current usage showing active and queued requests
* **limits**: Your team's concurrency limits

### Concurrency Fields

The `concurrency` object shows your current request state:

* **active**: Number of requests currently being processed
* **queued**: Number of requests waiting to be processed

### Limits Fields

The `limits` object shows your team's configured limits:

* **maxConcurrent**: Maximum number of requests that can be processed simultaneously (null means unlimited)
* **maxQueued**: Maximum number of requests that can wait in the queue (null means unlimited)


## OpenAPI

````yaml get /v0/teams/me
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
  /v0/teams/me:
    get:
      tags:
        - Teams
      summary: Get Team Info
      description: >-
        Returns information about the authenticated team, including current
        concurrency usage and limits.
      operationId: teams-me-get
      responses:
        '200':
          description: Team information retrieved successfully
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/WebsetsTeamInfo'
      x-codeSamples:
        - lang: bash
          label: Get team info
          source: |-
            curl -X GET 'https://api.exa.ai/websets/v0/teams/me' \
              -H 'x-api-key: YOUR-EXA-API-KEY'
        - lang: python
          label: Get team info
          source: |-
            import requests

            headers = {
                'x-api-key': 'YOUR-EXA-API-KEY'
            }

            response = requests.get(
                'https://api.exa.ai/websets/v0/teams/me',
                headers=headers
            )

            print(response.json())
        - lang: javascript
          label: Get team info
          source: >-
            const response = await
            fetch('https://api.exa.ai/websets/v0/teams/me', {
              method: 'GET',
              headers: {
                'x-api-key': 'YOUR-EXA-API-KEY'
              }
            });


            const result = await response.json();

            console.log(result);
components:
  schemas:
    WebsetsTeamInfo:
      type: object
      properties:
        object:
          type: string
          const: team
          description: The object type, always `"team"`.
        id:
          type: string
          description: Unique identifier for the team.
        name:
          type: string
          description: Name of the team.
        concurrency:
          type: object
          properties:
            active:
              type: integer
              description: Number of requests currently being processed.
            queued:
              type: integer
              description: Number of requests currently queued.
          required:
            - active
            - queued
          additionalProperties: false
          description: Current concurrency usage.
        limits:
          type: object
          properties:
            maxConcurrent:
              anyOf:
                - type: integer
                - type: 'null'
              description: >-
                Maximum number of concurrent requests allowed. Null means
                unlimited.
            maxQueued:
              anyOf:
                - type: integer
                - type: 'null'
              description: Maximum number of queued requests allowed. Null means unlimited.
          required:
            - maxConcurrent
            - maxQueued
          additionalProperties: false
          description: Concurrency limits for the team.
      required:
        - object
        - id
        - name
        - concurrency
        - limits
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