> ## Documentation Index
> Fetch the complete documentation index at: https://exa.ai/docs/llms.txt
> Use this file to discover all available pages before exploring further.

# Context (Exa Code)

> Get relevant code snippets and examples from open source libraries and repositories. Search through code repositories to find contextual examples that help developers understand how specific libraries, frameworks, or programming concepts are implemented in practice.

<Note>
  **New to Exa?** Try the [Coding Agent Quickstart](https://dashboard.exa.ai/onboarding)
  to get started in under a minute.
</Note>

<Card title="Get your Exa API key" icon="key" horizontal href="https://dashboard.exa.ai/api-keys" />

## Overview

The Context API (also called **Exa Code**) is a powerful tool for coding agents that need fast, efficient web context. It searches over billions of GitHub repos, docs pages, Stack Overflow posts, and more to find the perfect, token-efficient context that agents need to code correctly.

This endpoint helps eliminate hallucinations in coding agents by providing real, working code examples from the open source community.

## Example Use Cases

The Context API excels at finding practical code examples for:

* **Framework usage**: "use Exa search in python and request `livecrawl=\"preferred\"` with a 12s `livecrawlTimeout`"
* **API syntax**: "use correct syntax for vercel ai sdk to call gpt-5 nano asking it how are you"
* **Development setup**: "how to set up a reproducible Nix Rust development environment"
* **Library implementation**: "React hooks for state management examples"
* **Best practices**: "authentication patterns in NextJS applications"

**Basic Code Search**

```bash theme={null}
curl -X POST 'https://api.exa.ai/context' \
  -H "x-api-key: $EXA_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{
    "query": "how to use React hooks for state management",
    "tokensNum": 5000
  }'
```

**Example Response:**

````json theme={null}
{
  "requestId": "81c4198a1d6794503b52134fd77159e2",
  "query": "how to use React hooks for state management",
  "response": "## State Management with useState Hook in React\n\nhttps://www.geeksforgeeks.org/reactjs/state-management-with-usestate-hook-in-react/\n\n```\nimport React, {\n  useState\n} from 'react';\n\nfunction InputField() {\n  const [name, setName] = useState('');\n\n  const handleChange = (event) => {\n    setName(event.target.value);\n  }\n\n  return (\n    <div>\n      Name:\n      <input onChange={handleChange} />\n      Entered name: {name}\n    </div>\n  );\n}\n\nexport default InputField;\n```\n\n## Basic useState Example\n\n```\nimport { useState } from 'react';\n\nfunction Example() {\n  const [count, setCount] = useState(0);\n\n  return (\n    <div>\n      <p>You clicked {count} times</p>\n      <button onClick={() => setCount(count + 1)}>\n        Click me\n      </button>\n    </div>\n  );\n}\n```\n\n## Custom Hook for Counter State Management\n\n```\nimport { useState } from \"react\";\n\nconst useCounter = () => {\n  const [count, setCount] = useState(0);\n\n  const increment = () => {\n    setCount((prevCount) => prevCount + 1);\n  };\n\n  const decrement = () => {\n    setCount((prevCount) => prevCount - 1);\n  };\n\n  return { count, increment, decrement };\n};\n\nexport default useCounter;\n```\n\n...(response continues with more code examples)",
  "resultsCount": 502,
  "costDollars": {"total": 1, "search": {"neural": 1}},
  "searchTime": 3112.290825000033,
  "outputTokens": 4805
}
````

**Library Usage Examples**

```bash theme={null}
curl -X POST 'https://api.exa.ai/context' \
  -H "x-api-key: $EXA_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{
    "query": "pandas dataframe filtering and groupby operations",
    "tokensNum": "dynamic"
  }'
```

**Framework Setup and Configuration**

```bash theme={null}
curl -X POST 'https://api.exa.ai/context' \
  -H "x-api-key: $EXA_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{
    "query": "Next.js 14 app router with TypeScript configuration",
    "tokensNum": "dynamic"
  }'
```

## Response Format

The API returns a JSON response with the following structure:

```json theme={null}
{
  "requestId": "req_12345",
  "query": "how to use React hooks for state management",
  "response": "// Formatted code snippets and contextual examples\n...",
  "resultsCount": 15,
  "costDollars": {"total": 1, "search": {"neural": 1}},
  "searchTime": 1.234,
  "outputTokens": 1247
}
```

## Parameters

### `query` (required)

* **Type**: `string`
* **Description**: Search query to find relevant code snippets
* **Example**: `"how to use React hooks for state management"`
* **Min Length**: 1 character
* **Max Length**: 2000 characters

### `tokensNum` (optional)

* **Type**: `string | integer`
* **Default**: `"dynamic"`
* **Description**: Token limit for the response
* **Options**:
  * `"dynamic"`: Automatically determine optimal response length
  * `50-100000`: Specific number of tokens to return (5000 is good default for most queries, and use 10000 when 5k doesn't provide enough context)

**Token Management**

* Use `"dynamic"` for most queries to get optimal, token-efficient responses
* Specify exact token counts when you need precise output length control
* Higher token counts return more comprehensive examples but cost more

## Integration Examples

**Using with Python**

```python theme={null}
import os
import requests

def get_code_context(query, tokens="dynamic"):
    response = requests.post(
        "https://api.exa.ai/context",
        headers={
            "Content-Type": "application/json",
            "x-api-key": os.environ["EXA_API_KEY"]
        },
        json={
            "query": query,
            "tokensNum": tokens
        }
    )
    
    result = response.json()
    return result["response"]

# Example usage
context = get_code_context("Express.js middleware for authentication")
print(context)
```

**Using with JavaScript/Node.js**

```javascript theme={null}
async function getCodeContext(query, tokensNum = "dynamic") {
  const response = await fetch("https://api.exa.ai/context", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "x-api-key": process.env.EXA_API_KEY
    },
    body: JSON.stringify({
      query,
      tokensNum
    })
  });
  
  const result = await response.json();
  return result.response;
}

// Example usage
const context = await getCodeContext("Svelte component lifecycle methods");
console.log(context);
```

## About Exa Code

Vibe coding should never have a bad vibe. `exa-code` is a huge step towards coding agents that never hallucinate.

When your coding agent makes a search query, `exa-code` searches over billions of GitHub repos, docs pages, Stack Overflow posts, and more, to find the perfect, token-efficient context that the agent needs to code correctly. It's powered by the Exa search engine.

## Use with MCP

You can also use `exa-code` through the [Exa MCP server](/reference/exa-mcp) for seamless integration with AI coding assistants like Claude, Cursor, and other MCP-compatible clients.

The MCP integration provides the same powerful code context search capabilities directly within your development environment without needing to make direct API calls.
