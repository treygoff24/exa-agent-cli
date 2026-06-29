> ## Documentation Index
> Fetch the complete documentation index at: https://exa.ai/docs/llms.txt
> Use this file to discover all available pages before exploring further.

# Delete API Key

> Permanently delete an API key from your team.

<Card title="Get your Exa API key" icon="key" horizontal href="https://dashboard.exa.ai/api-keys" />

## Overview

The Delete API Key endpoint permanently removes an API key from your team.

## Path Parameters

* **id**: The unique identifier of the API key to delete.


## OpenAPI

````yaml delete /api-keys/{id}
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
    delete:
      tags:
        - Team Management
      summary: Delete API Key
      description: >-
        Deletes an API key. Only API keys belonging to the authenticated team
        can be deleted.
      operationId: delete-api-key
      parameters:
        - name: id
          in: path
          required: true
          schema:
            type: string
          description: The unique identifier of the API key to delete.
      responses:
        '200':
          description: API key deleted successfully
          content:
            application/json:
              schema:
                type: object
                properties:
                  success:
                    type: boolean
                    example: true
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
          label: Delete an API key
          source: >
            curl -X DELETE
            'https://admin-api.exa.ai/team-management/api-keys/{id}' \
              -H 'x-api-key: YOUR-SERVICE-KEY'
        - lang: python
          label: Delete an API key
          source: |
            import requests

            headers = {
                'x-api-key': 'YOUR-SERVICE-KEY',
                'Content-Type': 'application/json'
            }

            response = requests.delete(
                'https://admin-api.exa.ai/team-management/api-keys/{id}',
                headers=headers
            )

            print(response.json())
        - lang: javascript
          label: Delete an API key
          source: >
            const response = await
            fetch('https://admin-api.exa.ai/team-management/api-keys/{id}', {
              method: 'DELETE',
              headers: {
                'x-api-key': 'YOUR-SERVICE-KEY'
              }
            });


            const result = await response.json();

            console.log(result);
components:
  securitySchemes:
    apikey:
      type: apiKey
      in: header
      name: x-api-key
      description: Service API key for team authentication

````