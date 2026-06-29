> ## Documentation Index
> Fetch the complete documentation index at: https://exa.ai/docs/llms.txt
> Use this file to discover all available pages before exploring further.

# Update API Key

> Update the name and rate limit of an existing API key.

<Card title="Get your Exa API key" icon="key" horizontal href="https://dashboard.exa.ai/api-keys" />

## Overview

The Update API Key endpoint allows you to modify an existing API key

## Path Parameters

* **id**: The unique identifier of the API key to update.

## Optional Parameters

* **name**: New descriptive name for the API key
* **rateLimit**: New rate limit in requests per minute


## OpenAPI

````yaml put /api-keys/{id}
openapi: 3.1.0
info:
  version: 1.0.0
  title: Team Management API
  description: >-
    API for managing API keys within teams. Provides CRUD operations for
    creating, listing, updating, and deleting API keys with team-based access
    controls.
servers:
  - url: https://admin-api.exa.ai/team-management
security:
  - apikey: []
paths:
  /api-keys/{id}:
    put:
      tags:
        - Team Management
      summary: Update API Key
      description: >-
        Updates an existing API key's name and/or rate limit. Only API keys
        belonging to the authenticated team can be updated.
      operationId: update-api-key
      parameters:
        - name: id
          in: path
          required: true
          schema:
            type: string
          description: The unique identifier of the API key to update.
      requestBody:
        required: true
        content:
          application/json:
            schema:
              type: object
              properties:
                name:
                  type: string
                  description: Optional new name for the API key
                  example: Updated Production Key
                rateLimit:
                  type: integer
                  description: >-
                    Optional new rate limit for the API key (requests per
                    second)
                  example: 2000
                budgetCents:
                  type:
                    - integer
                    - 'null'
                  minimum: 0
                  description: >-
                    Optional new spending budget for the API key, in cents. Set
                    to null to remove the budget.
                  example: 5000
              additionalProperties: false
      responses:
        '200':
          description: API key updated successfully
          content:
            application/json:
              schema:
                type: object
                properties:
                  apiKey:
                    type: object
                    properties:
                      id:
                        type: string
                        format: uuid
                      name:
                        type: string
                      rateLimit:
                        type:
                          - integer
                          - 'null'
                        description: Rate limit in requests per second
                      budgetCents:
                        type:
                          - integer
                          - 'null'
                        description: Spending budget for the API key, in cents
                      isOverBudget:
                        type: boolean
                        description: Whether the API key is currently over its budget
                      teamId:
                        type: string
                        format: uuid
                      userId:
                        type: string
                        format: uuid
                      createdAt:
                        type: string
                        format: date-time
                      updatedAt:
                        type: string
                        format: date-time
        '400':
          description: Bad Request - Invalid parameters
          content:
            application/json:
              schema:
                type: object
                properties:
                  error:
                    type: string
                    examples:
                      - api_key_id is required
                      - Invalid API key ID format.
        '401':
          description: Unauthorized - Invalid or missing service key
          content:
            application/json:
              schema:
                type: object
                properties:
                  error:
                    type: string
                    example: Unauthorized
        '403':
          description: Forbidden - API key belongs to a different team
          content:
            application/json:
              schema:
                type: object
                properties:
                  error:
                    type: string
                    example: You do not have permission to access this API key
        '404':
          description: Not Found - API key does not exist
          content:
            application/json:
              schema:
                type: object
                properties:
                  error:
                    type: string
                    example: API key not found
      security:
        - apikey: []
      x-codeSamples:
        - lang: bash
          label: Update API key name and rate limit
          source: >
            curl -X PUT 'https://admin-api.exa.ai/team-management/api-keys/{id}'
            \
              -H 'x-api-key: YOUR-SERVICE-KEY' \
              -H 'Content-Type: application/json' \
              -d '{
                "name": "Updated Production Key",
                "rateLimit": 2000
              }'
        - lang: python
          label: Update API key name and rate limit
          source: |
            import requests

            headers = {
                'x-api-key': 'YOUR-SERVICE-KEY',
                'Content-Type': 'application/json'
            }

            data = {
                'name': 'Updated Production Key',
                'rateLimit': 2000
            }

            response = requests.put(
                'https://admin-api.exa.ai/team-management/api-keys/{id}',
                headers=headers,
                json=data
            )

            print(response.json())
        - lang: javascript
          label: Update API key name and rate limit
          source: >
            const response = await
            fetch('https://admin-api.exa.ai/team-management/api-keys/{id}', {
              method: 'PUT',
              headers: {
                'x-api-key': 'YOUR-SERVICE-KEY',
                'Content-Type': 'application/json'
              },
              body: JSON.stringify({
                name: 'Updated Production Key',
                rateLimit: 2000
              })
            });


            const result = await response.json();

            console.log(result);
        - lang: bash
          label: Update only the name
          source: >
            curl -X PUT 'https://admin-api.exa.ai/team-management/api-keys/{id}'
            \
              -H 'x-api-key: YOUR-SERVICE-KEY' \
              -H 'Content-Type: application/json' \
              -d '{
                "name": "New Name Only"
              }'
components:
  securitySchemes:
    apikey:
      type: apiKey
      in: header
      name: x-api-key
      description: Service API key for team authentication

````