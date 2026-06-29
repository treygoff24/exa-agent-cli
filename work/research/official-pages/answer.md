> ## Documentation Index
> Fetch the complete documentation index at: https://exa.ai/docs/llms.txt
> Use this file to discover all available pages before exploring further.

# Answer

> Get an LLM answer to a question informed by Exa search results. `/answer` performs an Exa search and uses an LLM to generate either:
1. A direct answer for specific queries. (i.e. "What is the capital of France?" would return "Paris")
2. A detailed summary with citations for open-ended queries (i.e. "What is the state of ai in healthcare?" would return a summary with citations to relevant sources)

The response includes both the generated answer and the sources used to create it. The endpoint also supports streaming (as `stream=True`), which will return tokens as they are generated.

Alternatively, you can use the OpenAI compatible [chat completions interface](/reference/openai-sdk#answer).


<Card title="Get your Exa API key" icon="key" horizontal href="https://dashboard.exa.ai/api-keys" />

<Info>
  `/answer` supports structured output via the `outputSchema` parameter. Pass a [JSON Schema](https://json-schema.org/draft-07) object and the answer will be returned as structured JSON matching your schema instead of a plain string.
</Info>


## OpenAPI

````yaml post /answer
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
  /answer:
    post:
      summary: Answer
      description: >-
        Performs a search based on the query and generates either a direct
        answer or a detailed summary with citations, depending on the query
        type.
      operationId: answer
      requestBody:
        required: true
        content:
          application/json:
            schema:
              $ref: '#/components/schemas/AnswerRequest'
      responses:
        '200':
          description: OK
          content:
            application/json:
              example:
                answer: $350 billion.
                citations:
                  - id: >-
                      https://www.theguardian.com/science/2024/dec/11/spacex-valued-at-350bn-as-company-agrees-to-buy-shares-from-employees
                    url: >-
                      https://www.theguardian.com/science/2024/dec/11/spacex-valued-at-350bn-as-company-agrees-to-buy-shares-from-employees
                    title: >-
                      SpaceX valued at $350bn as company agrees to buy shares
                      from ...
                    author: Dan Milmon
                    publishedDate: '2023-11-16T01:36:32.547Z'
                    text: >-
                      SpaceX valued at $350bn as company agrees to buy shares
                      from ...
                    image: >-
                      https://i.guim.co.uk/img/media/7cfee7e84b24b73c97a079c402642a333ad31e77/0_380_6176_3706/master/6176.jpg?width=1200&height=630&quality=85&auto=format&fit=crop&overlay-align=bottom%2Cleft&overlay-width=100p&overlay-base64=L2ltZy9zdGF0aWMvb3ZlcmxheXMvdGctZGVmYXVsdC5wbmc&enable=upscale&s=71ebb2fbf458c185229d02d380c01530
                    favicon: >-
                      https://assets.guim.co.uk/static/frontend/icons/homescreen/apple-touch-icon.svg
                costDollars:
                  total: 0.007
                  search:
                    neural: 0.007
              schema:
                $ref: '#/components/schemas/AnswerResponse'
            text/event-stream:
              schema:
                $ref: '#/components/schemas/AnswerStreamChunk'
      x-codeSamples:
        - lang: bash
          label: Simple answer
          source: |-
            curl -X POST 'https://api.exa.ai/answer' \
              -H 'x-api-key: YOUR-EXA-API-KEY' \
              -H 'Content-Type: application/json' \
              -d '{
                "query": "What is the latest valuation of SpaceX?",
                "text": true
              }'
        - lang: python
          label: Simple answer
          source: |-
            # pip install exa-py
            from exa_py import Exa
            exa = Exa(api_key='YOUR_EXA_API_KEY')

            result = exa.answer(
                "What is the latest valuation of SpaceX?",
                text=True
            )

            print(result)
        - lang: javascript
          label: Simple answer
          source: |-
            // npm install exa-js
            import Exa from 'exa-js';
            const exa = new Exa('YOUR_EXA_API_KEY');

            const result = await exa.answer(
                'What is the latest valuation of SpaceX?',
                { text: true }
            );

            console.log(result);
components:
  schemas:
    AnswerRequest:
      type: object
      properties:
        query:
          type: string
          minLength: 1
          description: Natural-language question or instructions for the request.
          example: What is the latest valuation of SpaceX?
        stream:
          type: boolean
          description: >-
            If true, the response is returned as a server-sent events (SSE)
            stream.
          default: false
        text:
          type: boolean
          title: Simple text retrieval
          description: >-
            If true, returns full page text with default settings. If false,
            disables text return.
          default: false
        outputSchema:
          type: object
          properties:
            type:
              type: string
              description: The root schema type (typically "object").
              example: object
            properties:
              type: object
              propertyNames:
                type: string
              additionalProperties:
                $ref: '#/components/schemas/JsonValue'
              description: >-
                An object where each key is a property name and each value is a
                JSON Schema describing that property (with `type`,
                `description`, etc).
            required:
              type: array
              items:
                type: string
              description: List of required property names.
            description:
              type: string
              description: A description of the schema.
            additionalProperties:
              type: boolean
              description: Whether to allow properties not listed in `properties`.
              default: false
          additionalProperties:
            $ref: '#/components/schemas/JsonValue'
          description: >-
            A [JSON Schema Draft 7](https://json-schema.org/draft-07)
            specification for the desired answer structure. When provided, the
            answer is returned as a structured object matching the schema
            instead of a plain string.
      required:
        - query
    AnswerResponse:
      type: object
      properties:
        requestId:
          type: string
          description: Unique identifier for the request.
          example: b5947044c4b78efa9552a7c89b306d95
        answer:
          description: >-
            The generated answer based on search results. Returns a string by
            default, or a structured object matching the provided outputSchema.
          example: $350 billion.
          oneOf:
            - type: string
            - type: object
              propertyNames:
                type: string
              additionalProperties:
                $ref: '#/components/schemas/JsonValue'
        citations:
          description: Search results used to generate the answer.
          type: array
          items:
            type: object
            properties:
              title:
                type: string
                description: The title of the search result.
                example: >-
                  SpaceX valued at $350bn as company agrees to buy shares from
                  ...
              url:
                type: string
                description: The URL of the search result.
                example: >-
                  https://www.theguardian.com/science/2024/dec/11/spacex-valued-at-350bn-as-company-agrees-to-buy-shares-from-employees
                format: uri
              publishedDate:
                description: >-
                  An estimate of the creation date, from parsing HTML content.
                  Format is YYYY-MM-DD.
                example: '2023-11-16T01:36:32.547Z'
                format: date-time
                type: string
              author:
                description: If available, the author of the content.
                example: Humza Naveed
                anyOf:
                  - type: string
                  - type: 'null'
              id:
                description: >-
                  The temporary ID for the document. Useful for the /contents
                  endpoint.
                example: https://arxiv.org/abs/2307.06435
                type: string
              image:
                description: >-
                  The URL of an image associated with the search result, if
                  available.
                example: https://arxiv.org/pdf/2307.06435.pdf/page_1.png
                format: uri
                type: string
              favicon:
                description: The URL of the favicon for the search result's domain.
                example: https://arxiv.org/favicon.ico
                format: uri
                type: string
              text:
                description: >-
                  The full text content of each source. Only present when text
                  contents are requested.
                example: >-
                  SpaceX valued at $350bn as company agrees to buy shares from
                  ...
                type: string
            required:
              - title
              - url
            additionalProperties: false
        costDollars:
          $ref: '#/components/schemas/CostDollarsOutput'
      required:
        - answer
      additionalProperties: false
    AnswerStreamChunk:
      description: >-
        Schema for each JSON payload emitted in an `/answer` server-sent event
        stream. Each event is emitted as `data: <json>`.
      oneOf:
        - type: object
          properties:
            choices:
              type: array
              items:
                type: object
                properties:
                  index:
                    type: integer
                    minimum: 0
                    description: Index of this streamed choice.
                  delta:
                    type: object
                    properties:
                      role:
                        type: string
                        const: assistant
                      content:
                        type: string
                      refusal:
                        anyOf:
                          - type: string
                          - type: 'null'
                    additionalProperties:
                      $ref: '#/components/schemas/JsonValue'
                    description: Incremental answer content emitted by the model.
                  finish_reason:
                    description: Reason this streamed choice finished, when present.
                    oneOf:
                      - type: string
                      - type: 'null'
                required:
                  - index
                  - delta
                additionalProperties:
                  $ref: '#/components/schemas/JsonValue'
              description: >-
                OpenAI-compatible streamed completion choices with internal
                provider fields removed.
          required:
            - choices
          additionalProperties:
            $ref: '#/components/schemas/JsonValue'
        - type: object
          properties:
            citations:
              type: array
              items:
                type: object
                properties:
                  title:
                    type: string
                    description: The title of the search result.
                    example: >-
                      SpaceX valued at $350bn as company agrees to buy shares
                      from ...
                  url:
                    type: string
                    description: The URL of the search result.
                    example: >-
                      https://www.theguardian.com/science/2024/dec/11/spacex-valued-at-350bn-as-company-agrees-to-buy-shares-from-employees
                    format: uri
                  publishedDate:
                    description: >-
                      An estimate of the creation date, from parsing HTML
                      content. Format is YYYY-MM-DD.
                    example: '2023-11-16T01:36:32.547Z'
                    format: date-time
                    type: string
                  author:
                    description: If available, the author of the content.
                    example: Humza Naveed
                    anyOf:
                      - type: string
                      - type: 'null'
                  id:
                    description: >-
                      The temporary ID for the document. Useful for the
                      /contents endpoint.
                    example: https://arxiv.org/abs/2307.06435
                    type: string
                  image:
                    description: >-
                      The URL of an image associated with the search result, if
                      available.
                    example: https://arxiv.org/pdf/2307.06435.pdf/page_1.png
                    format: uri
                    type: string
                  favicon:
                    description: The URL of the favicon for the search result's domain.
                    example: https://arxiv.org/favicon.ico
                    format: uri
                    type: string
                  text:
                    description: >-
                      The full text content of each source. Only present when
                      text contents are requested.
                    example: >-
                      SpaceX valued at $350bn as company agrees to buy shares
                      from ...
                    type: string
                required:
                  - title
                  - url
                additionalProperties: false
              description: Search results cited by the final streamed answer.
          required:
            - citations
          additionalProperties: false
        - type: object
          properties:
            costDollars:
              $ref: '#/components/schemas/CostDollarsOutput'
            requestId:
              type: string
              description: Unique identifier for the request.
              example: b5947044c4b78efa9552a7c89b306d95
          required:
            - costDollars
          additionalProperties: false
        - type: object
          properties:
            tag:
              type: string
              const: ERROR
            payload:
              type: object
              properties:
                error:
                  type: object
                  properties:
                    code:
                      type: integer
                    message:
                      type: string
                  required:
                    - code
                    - message
                  additionalProperties: false
                requestId:
                  type: string
                  description: Unique identifier for the request.
                  example: b5947044c4b78efa9552a7c89b306d95
              required:
                - error
              additionalProperties: false
          required:
            - tag
            - payload
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
    CostDollarsOutput:
      type: object
      properties:
        total:
          description: >-
            Estimated total dollar cost for the completed request. This response
            value is not an invoice record.
          example: 0.007
          format: float
          type: number
        search:
          description: >-
            Endpoint-dependent estimated search cost breakdown by retrieval
            mode. Instant, fast, and auto search responses may include neural
            search cost. Deep search modes may be reflected only in total.
          type: object
          properties:
            neural:
              description: Cost of neural search operations.
              example: 0.007
              format: float
              type: number
          additionalProperties: false
      additionalProperties: false
      description: >-
        Endpoint-dependent estimated dollar cost breakdown for the completed
        request. Billing is computed from usage counters rather than this
        response object.
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