> ## Documentation Index
> Fetch the complete documentation index at: https://exa.ai/docs/llms.txt
> Use this file to discover all available pages before exploring further.

# OpenAI SDK Compatibility

> Use Exa's endpoints as a drop-in replacement for OpenAI - supporting both chat completions and responses APIs.

<Note>
  **New to Exa?** Try the [Coding Agent Quickstart](https://dashboard.exa.ai/onboarding)
  to get started in under a minute.
</Note>

***

## Overview

Exa provides OpenAI-compatible endpoints that work seamlessly with the OpenAI SDK:

| Endpoint            | OpenAI Interface     | Models Available | Use Case                                              |
| ------------------- | -------------------- | ---------------- | ----------------------------------------------------- |
| `/chat/completions` | Chat Completions API | `exa`            | Traditional chat interface                            |
| `/responses`        | Responses API        | `exa-agent`      | Agent API (async research, enrichment, list-building) |

<Info>
  {" "}

  `/chat/completions` routes to [`/answer`](/reference/answer). `/responses` routes to the [Agent API](/reference/agent-api/overview) — see [Agent via Responses API](#agent-via-responses-api) below.
</Info>

## Answer

To use Exa's `/answer` endpoint via the chat completions interface:

1. Replace base URL with `https://api.exa.ai`
2. Replace API key with your Exa API key
3. Replace model name with `exa`.

<Info>
  {" "}

  See the full `/answer` endpoint reference [here](/reference/answer).{" "}
</Info>

<Info>
  {" "}

  Need custom behavior when routing through `/answer`? Contact us at [hello@exa.ai](mailto:hello@exa.ai) and we can help tailor the integration.{" "}
</Info>

<CodeGroup>
  ```python Python theme={null}
  import os
  from openai import OpenAI

  client = OpenAI(
    base_url="https://api.exa.ai", # use exa as the base url
    api_key=os.environ["EXA_API_KEY"],
  )

  completion = client.chat.completions.create(
    model="exa",
    messages = [
    {"role": "system", "content": "You are a helpful assistant."},
    {"role": "user", "content": "What are the latest developments in quantum computing?"}
  ],

  # use extra_body to pass extra parameters to the /answer endpoint
    extra_body={
      "text": True # include full text from sources
    }
  )

  print(completion.choices[0].message.content)  # print the response content
  print(completion.choices[0].message.citations)  # print the citations
  ```

  ```javascript JavaScript theme={null}
  import OpenAI from "openai";

  const openai = new OpenAI({
    baseURL: "https://api.exa.ai", // use exa as the base url
    apiKey: process.env.EXA_API_KEY,
  });

  async function main() {
    const completion = await openai.chat.completions.create({
      model: "exa",
      messages: [
        { role: "system", content: "You are a helpful assistant." },
        {
          role: "user",
          content: "What are the latest developments in quantum computing?",
        },
      ],
      store: true,
      stream: true,
      extra_body: {
        text: true, // include full text from sources
      },
    });

    for await (const chunk of completion) {
      console.log(chunk.choices[0].delta.content);
    }
  }

  main();
  ```

  ```bash Curl theme={null}
  curl https://api.exa.ai/chat/completions \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer $EXA_API_KEY" \
    -d '{
      "model": "exa",
      "messages": [
        {
          "role": "system",
          "content": "You are a helpful assistant."
        },
        {
          "role": "user",
          "content": "What are the latest developments in quantum computing?"
        }
      ],
      "text": true
    }'
  ```
</CodeGroup>

## Agent via Responses API

Exa's [`/responses`](https://api.exa.ai/responses) endpoint provides OpenAI Responses API compatibility for the [Agent API](/reference/agent-api/overview). Runs are asynchronous — set `background: true` and poll `GET /responses/{id}` for the result.

<Info>
  {" "}

  The `/responses` surface maps directly to the [Agent API](/reference/agent-api/overview). For the full Agent API reference and examples, see the [Agent guide](/reference/agent-api-guide).
</Info>

<CodeGroup>
  ```python Python theme={null}
  import os
  import time
  from openai import OpenAI

  client = OpenAI(
      base_url="https://api.exa.ai",
      api_key=os.environ["EXA_API_KEY"],
  )

  response = client.responses.create(
      model="exa-agent",
      input="Find the top 5 AI startups founded in 2025 with their funding amounts",
      background=True,
  )

  # Poll until complete
  while response.status in ("queued", "in_progress"):
      time.sleep(5)
      response = client.responses.retrieve(response.id)

  print(response.output_text)
  ```

  ```javascript JavaScript theme={null}
  import OpenAI from "openai";

  const openai = new OpenAI({
    baseURL: "https://api.exa.ai",
    apiKey: process.env.EXA_API_KEY,
  });

  async function main() {
    let response = await openai.responses.create({
      model: "exa-agent",
      input: "Find the top 5 AI startups founded in 2025 with their funding amounts",
      background: true,
    });

    // Poll until complete
    while (response.status === "queued" || response.status === "in_progress") {
      await new Promise((r) => setTimeout(r, 5000));
      response = await openai.responses.retrieve(response.id);
    }

    console.log(response.output_text);
  }

  main();
  ```

  ```bash cURL theme={null}
  # Create a background run
  curl -X POST 'https://api.exa.ai/responses' \
    -H "x-api-key: $EXA_API_KEY" \
    -H 'Content-Type: application/json' \
    -d '{
      "model": "exa-agent",
      "input": "Find the top 5 AI startups founded in 2025 with their funding amounts",
      "background": true
    }'

  # Poll with the returned response ID
  curl 'https://api.exa.ai/responses/resp_agent_run_...' \
    -H "x-api-key: $EXA_API_KEY"
  ```
</CodeGroup>

## Chat Wrapper

Exa provides a Python wrapper that automatically enhances any OpenAI chat completion with RAG capabilities. With one line of code, you can turn any OpenAI chat completion into an Exa-powered RAG system that handles search, chunking, and prompting automatically.

<CodeGroup>
  ```python Python theme={null}
  from openai import OpenAI
  from exa_py import Exa

  # Initialize clients
  openai = OpenAI(api_key='OPENAI_API_KEY')
  exa = Exa(api_key='EXA_API_KEY')

  # Wrap the OpenAI client
  exa_openai = exa.wrap(openai)

  # Use exactly like the normal OpenAI client
  completion = exa_openai.chat.completions.create(
      model="gpt-4o",
      messages=[{"role": "user", "content": "What is the latest climate tech news?"}]
  )

  print(completion.choices[0].message.content)
  ```
</CodeGroup>

The wrapped client works exactly like the native OpenAI client, except it automatically improves your completions with relevant search results when needed.

The wrapper supports any parameters from the `exa.search()` function.

```python theme={null}
completion = exa_openai.chat.completions.create(
    model="gpt-4o",
    messages=messages,
    use_exa="auto",              # "auto", "required", or "none"
    num_results=5,               # defaults to 3
    result_max_len=1024,         # defaults to 2048 characters
    include_domains=["arxiv.org"],
    category="research paper",
    start_published_date="2019-01-01"
)
```
