> ## Documentation Index
> Fetch the complete documentation index at: https://exa.ai/docs/llms.txt
> Use this file to discover all available pages before exploring further.

# HIPAA

> Use HIPAA compliance mode for eligible cached retrieval requests.

<Info>
  HIPAA compliance is available for Enterprise customers after Exa enables it for your team. Contact [sales@exa.ai](mailto:sales@exa.ai) to discuss Enterprise access, BAA requirements, and enablement.
</Info>

HIPAA mode is controlled per request with a top-level `compliance` field:

```json theme={null}
{
  "compliance": "hipaa"
}
```

When this field is present on an eligible team, Exa handles the request with HIPAA compliance controls for that request. If your team is not enabled, the API returns `403 FEATURE_DISABLED`.

HIPAA mode includes Zero Data Retention behavior for those requests: Exa does not persist PHI, and the request follows a compliant processor path that only uses approved subprocessors.

## Supported endpoints

The `compliance` field is recognized on:

* [`/search`](/reference/search)
* [`/contents`](/reference/get-contents)

Other endpoints reject the field. For HIPAA retrieval workflows, use `/contents` for cached, non-generative content extraction.

## Search behavior

HIPAA search requests fail closed if the resolved search path requires live retrieval, keyword/SERP-backed retrieval, summaries, or any other non-HIPAA-safe processor path. In that case, the API returns `400 INVALID_REQUEST_BODY`.

## Contents example

Use HIPAA mode when extracting cached content from known URLs. If Exa does not already have cached content for a URL, the request can return no content for that URL instead of livecrawling it.

```bash theme={null}
curl -X POST "https://api.exa.ai/contents" \
  -H "Content-Type: application/json" \
  -H "x-api-key: $EXA_API_KEY" \
  -d '{
    "urls": ["https://example.com/article"],
    "compliance": "hipaa",
    "text": true,
    "maxAgeHours": -1
  }'
```

## Compatible parameters

HIPAA mode is designed for cached, non-generative retrieval. For the supported `/contents` path:

* Request `text` or `highlights`
* Omit freshness fields, or set `maxAgeHours: -1` for explicit cache-only retrieval on `/contents`

The API returns `400 INVALID_REQUEST_BODY` for incompatible parameters, including:

* `summary` on `/contents`
* `contents.summary` on `/search`
* Livecrawl or freshness settings that require fetching a fresh page, such as `maxAgeHours: 0` or a positive `maxAgeHours`
* Search requests whose resolved path requires non-HIPAA-safe retrieval, including `auto`, `deep-lite`, `deep`, and `deep-reasoning`

HIPAA mode is currently intended for:

* Cached content retrieval only
* Non-generative retrieval workflows without summaries

## Access

To enable HIPAA mode for a team, contact [sales@exa.ai](mailto:sales@exa.ai). You can also visit the [Trust Center](https://trust.exa.ai) for Exa security documentation.
