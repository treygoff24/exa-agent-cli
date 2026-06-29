> ## Documentation Index
> Fetch the complete documentation index at: https://exa.ai/docs/llms.txt
> Use this file to discover all available pages before exploring further.

# Get API Key Usage

> Retrieve usage analytics and billing data for a specific API key.

<Card title="Get your Exa API key" icon="key" horizontal href="https://dashboard.exa.ai/api-keys" />

## Overview

The Get API Key Usage endpoint allows you to retrieve detailed billing and usage analytics for a specific API key over a given time period. This endpoint returns cost data from Exa's billing system, providing an authoritative view of what you're being billed for that API key.

## Path Parameters

* **id**: The unique identifier of the API key to retrieve usage for

## Query Parameters

* **start\_date** (optional): Start date for the usage period in ISO 8601 format (e.g., `2025-01-01T00:00:00Z` or `2025-01-01`). Defaults to 30 days ago. Must be within the last 6 months (180 days).
* **end\_date** (optional): End date for the usage period in ISO 8601 format. Defaults to the current time.
* **group\_by** (optional): Time granularity for grouping results (`hour`, `day`, or `month`). Currently reserved for future enhancements and does not change the response shape. Defaults to `day`.

## Response

Returns detailed usage and billing information including:

* **id**: Unique identifier of the API key
* **api\_key\_id**: Unique identifier of the API key
* **api\_key\_name**: Descriptive name of the API key (if set)
* **team\_id**: Team ID this key belongs to
* **period**: Object containing the start and end dates of the usage period
* **total\_cost\_usd**: Total cost in USD for the specified period
* **cost\_breakdown**: Array of cost breakdowns by price type, each containing:
  * **price\_id**: Unique identifier for the price
  * **price\_name**: Name of the price (e.g., "Neural Search", "Content Retrieval")
  * **quantity**: Total quantity consumed
  * **amount\_usd**: Cost in USD for this price type
* **metadata**: Object containing report generation timestamp

## Important Notes

* **6-Month Lookback Limit**: The billing system has a 6-month (180-day) lookback limit. Requests with `start_date` older than 180 days will return a 400 error.
* **Zero Usage**: If the API key has no usage in the requested period, `total_cost_usd` will be 0 and `cost_breakdown` may be empty.
* **Team Ownership**: The service API key used for authentication must belong to the same team as the requested API key. Cross-team access is not permitted.
* **Date Formats**: Dates can be provided in ISO 8601 format with or without time components (e.g., `2025-01-01` or `2025-01-01T00:00:00Z`).

## Use Cases

This endpoint is useful for:

* Building API-key-level billing dashboards
* Monitoring usage and costs for specific API keys
* Creating automated alerts based on usage thresholds
* Generating usage reports for internal cost allocation
* Debugging billing questions for specific API keys


## OpenAPI

````yaml get /api-keys/{id}/usage
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
  /api-keys/{id}/usage:
    get:
      tags:
        - Team Management
      summary: Get API Key Usage
      description: >-
        Retrieves usage analytics and billing data for a specific API key over a
        given time period. Returns cost breakdown by price type from the billing
        system.
      operationId: get-api-key-usage
      parameters:
        - name: id
          in: path
          required: true
          schema:
            type: string
          description: The unique identifier of the API key.
        - name: start_date
          in: query
          required: false
          schema:
            type: string
            format: date-time
          description: >-
            Start date for the usage period (ISO 8601 format). Defaults to 30
            days ago. Must be within the last 6 months (180 days).
          example: '2025-01-01T00:00:00Z'
        - name: end_date
          in: query
          required: false
          schema:
            type: string
            format: date-time
          description: >-
            End date for the usage period (ISO 8601 format). Defaults to current
            time.
          example: '2025-01-31T23:59:59Z'
        - name: group_by
          in: query
          required: false
          schema:
            type: string
            enum:
              - hour
              - day
              - month
          description: >-
            Time granularity for grouping results. Currently reserved for future
            enhancements and does not change the response shape. Defaults to
            'day'.
          example: day
      responses:
        '200':
          description: Usage data retrieved successfully
          content:
            application/json:
              schema:
                type: object
                properties:
                  id:
                    type: string
                    description: The unique identifier of the API key.
                  api_key_id:
                    type: string
                    format: uuid
                    description: The API key ID.
                  api_key_name:
                    type:
                      - string
                      - 'null'
                    description: The name of the API key
                  team_id:
                    type: string
                    format: uuid
                    description: The team ID this key belongs to
                  period:
                    type: object
                    properties:
                      start:
                        type: string
                        format: date-time
                        description: Start of the usage period
                      end:
                        type: string
                        format: date-time
                        description: End of the usage period
                  total_cost_usd:
                    type: number
                    description: Total cost in USD for the period
                    example: 45.67
                  cost_breakdown:
                    type: array
                    description: Breakdown of costs by price type
                    items:
                      type: object
                      properties:
                        price_id:
                          type: string
                          description: Unique identifier for the price
                        price_name:
                          type: string
                          description: >-
                            Name of the price (e.g., "Neural Search", "Content
                            Retrieval")
                        quantity:
                          type: number
                          description: Total quantity consumed
                        amount_usd:
                          type: number
                          description: Cost in USD for this price type
                  metadata:
                    type: object
                    properties:
                      generated_at:
                        type: string
                        format: date-time
                        description: When this report was generated
              example:
                id: key_abc123def456
                api_key_id: 550e8400-e29b-41d4-a716-446655440000
                api_key_name: Production API Key
                team_id: 660e8400-e29b-41d4-a716-446655440000
                period:
                  start: '2025-01-01T00:00:00Z'
                  end: '2025-01-31T23:59:59Z'
                total_cost_usd: 45.67
                cost_breakdown:
                  - price_id: price_neural_search
                    price_name: Neural Search
                    quantity: 1000
                    amount_usd: 30
                  - price_id: price_content_retrieval
                    price_name: Content Retrieval
                    quantity: 500
                    amount_usd: 15.67
                metadata:
                  generated_at: '2025-02-01T10:30:00Z'
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
                      - Invalid API key ID format.
                      - >-
                        Invalid date format. Use ISO 8601 format (YYYY-MM-DD or
                        YYYY-MM-DDTHH:mm:ss)
                      - start_date must be before end_date
                      - >-
                        Date range too far in the past. start_date must be
                        within the last 6 months.
                      - >-
                        Invalid group_by parameter. Must be one of: hour, day,
                        month
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
        '500':
          description: Internal Server Error - Failed to fetch usage data
          content:
            application/json:
              schema:
                type: object
                properties:
                  error:
                    type: string
                    example: Failed to fetch usage data. Please try again later.
      security:
        - apikey: []
      x-codeSamples:
        - lang: bash
          label: Get usage for the last 30 days (default)
          source: >
            curl -X GET
            'https://admin-api.exa.ai/team-management/api-keys/{id}/usage' \
              -H 'x-api-key: YOUR-SERVICE-KEY'
        - lang: bash
          label: Get usage for a specific date range
          source: >
            curl -X GET
            'https://admin-api.exa.ai/team-management/api-keys/{id}/usage?start_date=2025-01-01&end_date=2025-01-31'
            \
              -H 'x-api-key: YOUR-SERVICE-KEY'
        - lang: python
          label: Get usage for a specific date range
          source: |
            import requests
            from datetime import datetime, timedelta

            headers = {
                'x-api-key': 'YOUR-SERVICE-KEY'
            }

            params = {
                'start_date': '2025-01-01T00:00:00Z',
                'end_date': '2025-01-31T23:59:59Z'
            }

            response = requests.get(
                'https://admin-api.exa.ai/team-management/api-keys/{id}/usage',
                headers=headers,
                params=params
            )

            print(response.json())
        - lang: javascript
          label: Get usage for a specific date range
          source: |
            const params = new URLSearchParams({
              start_date: '2025-01-01T00:00:00Z',
              end_date: '2025-01-31T23:59:59Z'
            });

            const response = await fetch(
              `https://admin-api.exa.ai/team-management/api-keys/{id}/usage?${params}`,
              {
                method: 'GET',
                headers: {
                  'x-api-key': 'YOUR-SERVICE-KEY'
                }
              }
            );

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