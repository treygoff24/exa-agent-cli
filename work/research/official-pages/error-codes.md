> ## Documentation Index
> Fetch the complete documentation index at: https://exa.ai/docs/llms.txt
> Use this file to discover all available pages before exploring further.

# Error Codes

> Reference for common error codes used by the Exa API

## API errors

| Code                        | Overview                                                                                                                                                                                                                                                                                                         |
| --------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 400 - Bad Request           | **Cause:** Invalid request parameters, malformed JSON, missing required fields<br />**Solution:** Check request body format, validate parameters, ensure API key is correctly formatted                                                                                                                          |
| 401 - Unauthorized          | **Cause:** Missing or invalid API key<br />**Solution:** Verify your API key is correct and active, ensure proper authentication headers                                                                                                                                                                         |
| 402 - Payment Required      | **Cause:** Account credits exhausted or API key spending budget exceeded<br />**Solution:** Top up credits at [dashboard.exa.ai](https://dashboard.exa.ai) or contact your team administrator to increase the API key budget                                                                                     |
| 403 - Forbidden             | **Cause:** Valid API key but insufficient permissions, feature disabled for your plan, or content blocked by policy<br />**Solution:** Check feature access permissions for your plan, verify the content is not blocked by robots.txt or content moderation                                                     |
| 404 - Not Found             | **Cause:** Resource not found (e.g., Webset, task, or URL doesn't exist)<br />**Solution:** Verify the resource identifier exists and is accessible                                                                                                                                                              |
| 409 - Conflict              | **Cause:** Resource already exists (e.g., Webset with same externalId)<br />**Solution:** Use a different identifier or update the existing resource                                                                                                                                                             |
| 422 - Unprocessable Entity  | **Cause:** Request was well-formed but could not be processed — e.g., a URL that failed to fetch (`/contents`), or a query that could not be decomposed into a valid entity/criteria pair (`/websets`)<br />**Solution:** Check the error message for details; verify URLs are accessible or rephrase your query |
| 429 - Too Many Requests     | **Cause:** Rate limit exceeded<br />**Solution:** Implement exponential backoff and reduce request rate                                                                                                                                                                                                          |
| 500 - Internal Server Error | **Cause:** Issue on our servers<br />**Solution:** Retry your request after a brief wait and contact us if the issue persists                                                                                                                                                                                    |
| 501 - Not Implemented       | **Cause:** `/answer` only — the model was unable to generate a response for the given query with the available information<br />**Solution:** Try rephrasing your query or adjusting parameters                                                                                                                  |
| 502 - Bad Gateway           | **Cause:** Upstream server issue<br />**Solution:** Retry the request after a brief delay                                                                                                                                                                                                                        |
| 503 - Service Unavailable   | **Cause:** Service temporarily down<br />**Solution:** Retry after delay, check for maintenance announcements                                                                                                                                                                                                    |

## Error Response Structure

All error responses include a `requestId` field, `error` message, and an error `tag`:

```json theme={null}
{
  "requestId": "67207943fab9832d162b5317f4cca830",
  "error": "Invalid request body | Validation error: Invalid enum value. Expected 'auto' | 'fast' | 'instant' | 'deep-lite' | 'deep' | 'deep-reasoning', received 'slow' at \"type\"",
  "tag": "INVALID_REQUEST_BODY"
}
```

<Note>
  Include the `requestId` when contacting support for faster troubleshooting. The `tag` field identifies the specific error type programmatically.
</Note>

Rate limit errors (429) use a simpler response format with only an `error` field:

```json theme={null}
{
  "error": "You've exceeded your Exa rate limit of 10 requests per second. If you want this increased, please email hello@exa.ai :)"
}
```

## API Error Tags

Error tags provide programmatic identification of the specific error. Use the `tag` field in the response to handle errors in your code.

### Authentication & Authorization

| Tag                       | HTTP Code | Description                                                                                            |
| ------------------------- | --------- | ------------------------------------------------------------------------------------------------------ |
| `INVALID_API_KEY`         | `401`     | API key is missing, empty, or invalid                                                                  |
| `NO_MORE_CREDITS`         | `402`     | Account credits are exhausted — top up at [dashboard.exa.ai](https://dashboard.exa.ai)                 |
| `API_KEY_BUDGET_EXCEEDED` | `402`     | API key has exceeded its spending budget — contact your team administrator                             |
| `TEAM_BUDGET_EXCEEDED`    | `402`     | Team has exceeded its spending budget for the current billing period — contact your team administrator |
| `ACCESS_DENIED`           | `403`     | `/search` only — feature requires a specific flag or permission you don't have                         |
| `FEATURE_DISABLED`        | `403`     | Feature is disabled for your plan type                                                                 |
| `ROBOTS_FILTER_FAILED`    | `403`     | `/contents` only — all requested URLs were blocked by robots.txt                                       |
| `PROHIBITED_CONTENT`      | `403`     | `/answer` only — request blocked by content safety moderation                                          |
| `CONTENT_FILTER_ERROR`    | `403`     | `/search` only — content was filtered due to safety policy                                             |

### Request Validation

| Tag                    | HTTP Code | Description                                                                               |
| ---------------------- | --------- | ----------------------------------------------------------------------------------------- |
| `INVALID_REQUEST_BODY` | `400`     | Request body failed validation (malformed JSON, missing fields, invalid parameter values) |
| `INVALID_REQUEST`      | `400`     | Conflicting parameters (e.g., setting both `livecrawl` and `maxAgeHours`)                 |
| `INVALID_URLS`         | `400`     | One or more URLs/IDs are in an invalid format                                             |
| `INVALID_NUM_RESULTS`  | `400`     | `/search` only — `numResults` must be ≤ 100 when using highlights                         |
| `INVALID_FLAGS`        | `400`     | Unrecognized flags in request                                                             |
| `INVALID_JSON_SCHEMA`  | `400`     | Provided JSON schema is invalid (used by `/answer`)                                       |
| `NUM_RESULTS_EXCEEDED` | `400`     | Requested number of results exceeds your plan's limit                                     |
| `NO_CONTENT_FOUND`     | `400`     | No contents could be found for the given URLs                                             |

### Processing Errors

| Tag                           | HTTP Code | Description                                                                   |
| ----------------------------- | --------- | ----------------------------------------------------------------------------- |
| `FETCH_DOCUMENT_ERROR`        | `422`     | A specific URL could not be processed                                         |
| `UNABLE_TO_GENERATE_RESPONSE` | `501`     | `/answer` only — unable to generate a response with the available information |
| `DEFAULT_ERROR`               | `500`     | Unexpected server error — retry after a brief wait                            |
| `INTERNAL_ERROR`              | `500`     | Unclassified internal error — retry after a brief wait                        |

## Content Fetch Status Tags

When using the `/contents` endpoint, per-URL errors are returned in the `statuses` field rather than as HTTP error codes. This allows for granular error handling when fetching multiple URLs. These status tags are specific to the `/contents` endpoint and are not returned by `/search`.

```json theme={null}
{
  "results": [...],
  "statuses": [
    {
      "id": "https://example.com",
      "status": "error",
      "error": {
        "tag": "CRAWL_NOT_FOUND",
        "httpStatusCode": 404
      }
    }
  ]
}
```

| Tag                       | HTTP Code | Description                                                             | How to Handle                                                                                                             |
| ------------------------- | --------- | ----------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| `CRAWL_NOT_FOUND`         | `404`     | Content not found at the specified URL                                  | Verify the URL is correct and accessible                                                                                  |
| `CRAWL_TIMEOUT`           | `504`     | The crawl timed out while fetching content from the target page         | Retry the request or try again later                                                                                      |
| `CRAWL_LIVECRAWL_TIMEOUT` | `504`     | Content could not be retrieved within your requested `livecrawlTimeout` | Increase `livecrawlTimeout`, omit `maxAgeHours` for default fallback behavior, or set `maxAgeHours: -1` to use cache only |
| `SOURCE_NOT_AVAILABLE`    | `403`     | Access forbidden or source unavailable                                  | Check if the source requires authentication or is behind a paywall                                                        |
| `UNSUPPORTED_URL`         | —         | URL scheme is not supported for content fetching                        | Use a standard HTTP/HTTPS URL                                                                                             |
| `CRAWL_UNKNOWN_ERROR`     | `500+`    | Other crawling errors                                                   | Retry the request; contact support if persistent                                                                          |

## Getting Help

If you encounter persistent errors or need clarification on error codes:

* Check the [Rate Limits](/reference/rate-limits) page for current limits
* Review the [API Reference](/reference/search) for parameter requirements
* Contact support at [hello@exa.ai](mailto:hello@exa.ai) with error details and request IDs
