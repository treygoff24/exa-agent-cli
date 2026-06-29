> ## Documentation Index
> Fetch the complete documentation index at: https://exa.ai/docs/llms.txt
> Use this file to discover all available pages before exploring further.

# Web Search MCP

> Complete setup guide for Exa MCP Server. Connect Claude Desktop, Cursor, VS Code, and 10+ AI assistants to Exa's web search and code search tools.

Exa MCP connects AI assistants to Exa's search capabilities, including web search, code search, and the async [Exa Agent](/reference/agent-api-guide). It is open-source and available on [GitHub](https://github.com/exa-labs/exa-mcp-server).

<br />

# Installation

Exa's Search MCP can be installed in any MCP client with the server URL: `https://mcp.exa.ai/mcp`

<CardGroup cols={2}>
  <Card
    title="Install in Cursor"
    icon={
  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 466.73 532.09">
    <path
      d="M457.43 125.94 244.42 2.96a22.127 22.127 0 0 0-22.12 0L9.3 125.94C3.55 129.26 0 135.4 0 142.05v247.99c0 6.65 3.55 12.79 9.3 16.11l213.01 122.98a22.127 22.127 0 0 0 22.12 0l213.01-122.98c5.75-3.32 9.3-9.46 9.3-16.11V142.05c0-6.65-3.55-12.79-9.3-16.11h-.01Zm-13.38 26.05L238.42 508.15c-1.39 2.4-5.06 1.42-5.06-1.36V273.58c0-4.66-2.49-8.97-6.53-11.31L24.87 145.67c-2.4-1.39-1.42-5.06 1.36-5.06h411.26c5.84 0 9.49 6.33 6.57 11.39h-.01Z"
      fill="#0765D9"
    />
  </svg>
}
    href="https://cursor.com/marketplace/exa"
  />

  <Card
    title="Install in VS Code"
    icon={
  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
    <path
      d="M70.912 99.572a6.193 6.193 0 0 0 4.96-.191l20.588-9.958a6.285 6.285 0 0 0 3.54-5.661V16.239a6.286 6.286 0 0 0-3.54-5.662L75.873.62a6.2 6.2 0 0 0-7.104 1.216L29.355 37.98l-17.168-13.1a4.146 4.146 0 0 0-5.318.238l-5.506 5.035a4.205 4.205 0 0 0-.004 6.194L16.247 50 1.36 63.654a4.205 4.205 0 0 0 .004 6.194l5.506 5.034a4.145 4.145 0 0 0 5.318.238l17.168-13.1L68.77 98.166a6.205 6.205 0 0 0 2.143 1.407Zm4.103-72.39L45.11 50 75.015 72.82V27.18Z"
      fillRule="evenodd"
      fill="#0765D9"
    />
  </svg>
}
    href="https://vscode.dev/redirect/mcp/install?name=exa&config=%7B%22type%22%3A%22http%22%2C%22url%22%3A%22https%3A%2F%2Fmcp.exa.ai%2Fmcp%22%7D"
  />
</CardGroup>

<Tabs>
  <Tab title="Codex">
    Run in terminal:

    ```bash theme={null}
    codex mcp add exa --url https://mcp.exa.ai/mcp
    ```
  </Tab>

  <Tab title="Claude Code">
    Run in terminal:

    ```bash theme={null}
    claude mcp add --transport http exa https://mcp.exa.ai/mcp
    ```
  </Tab>

  <Tab title="Claude Desktop">
    Exa is available as a **native Claude Connector** — no config files or terminal commands needed.

    1. Open Claude Desktop and click **+** (or **Add connectors**)
    2. Go to the **Connectors** tab
    3. Search for **Exa**
    4. Click **+** to add it

    That's it! Claude will now have access to Exa's search tools.
  </Tab>

  <Tab title="OpenCode">
    Add to your `opencode.json`:

    ```json theme={null}
    {
      "mcp": {
        "exa": {
          "type": "remote",
          "url": "https://mcp.exa.ai/mcp",
          "enabled": true
        }
      }
    }
    ```
  </Tab>

  <Tab title="Kiro">
    Add to `~/.kiro/settings/mcp.json`:

    ```json theme={null}
    {
      "mcpServers": {
        "exa": {
          "url": "https://mcp.exa.ai/mcp"
        }
      }
    }
    ```
  </Tab>

  <Tab title="Other">
    Exa MCP works with most other MCP clients — point them at `https://mcp.exa.ai/mcp`. The config key for the URL varies by client:

    | Client             | Where to add it                                                         | URL key                |
    | ------------------ | ----------------------------------------------------------------------- | ---------------------- |
    | Windsurf           | `~/.codeium/windsurf/mcp_config.json` (under `mcpServers`)              | `serverUrl`            |
    | Google Antigravity | Agent panel → Manage MCP Servers → View Raw config (under `mcpServers`) | `serverUrl`            |
    | Zed                | Zed `settings.json` (under `context_servers`)                           | `url`                  |
    | Gemini CLI         | `~/.gemini/settings.json` (under `mcpServers`)                          | `httpUrl`              |
    | Warp               | Settings → MCP Servers → Add MCP Server (top-level `exa`)               | `url`                  |
    | v0 by Vercel       | Prompt Tools → Add MCP                                                  | paste the URL directly |

    Most other clients use the standard `mcpServers` shape:

    ```json theme={null}
    {
      "mcpServers": {
        "exa": {
          "url": "https://mcp.exa.ai/mcp"
        }
      }
    }
    ```

    If your client doesn't support remote MCP servers directly, use the `mcp-remote` bridge:

    ```json theme={null}
    {
      "mcpServers": {
        "exa": {
          "command": "npx",
          "args": ["-y", "mcp-remote", "https://mcp.exa.ai/mcp"]
        }
      }
    }
    ```

    Or run the local [npm package](https://www.npmjs.com/package/exa-mcp-server) with your [Exa API key](https://dashboard.exa.ai/api-keys):

    ```json theme={null}
    {
      "mcpServers": {
        "exa": {
          "command": "npx",
          "args": ["-y", "exa-mcp-server"],
          "env": {
            "EXA_API_KEY": "your_api_key"
          }
        }
      }
    }
    ```
  </Tab>
</Tabs>

# API Key

Exa MCP has a generous free plan. To overcome free plan rate limits and enable production use, add your own API key:

```json theme={null}
{
  "exa": {
    "url": "https://mcp.exa.ai/mcp",
    "headers": {
      "x-api-key": "YOUR_EXA_API_KEY"
    }
  }
}
```

[Get your Exa API key](https://dashboard.exa.ai/api-keys)

# Available Tools

**Enabled by default:**

| Tool             | Description                                                           |
| ---------------- | --------------------------------------------------------------------- |
| `web_search_exa` | Search the web for any topic and get clean, ready-to-use content      |
| `web_fetch_exa`  | Read a webpage's full content as clean markdown from one or more URLs |

**Optional** (enable via `tools` parameter):

| Tool                      | Description                                                                                                                                |
| ------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| `web_search_advanced_exa` | Advanced web search with full control over category filters, domain restrictions, date ranges, highlights, summaries, and subpage crawling |
| `agent_tools`             | Alias that enables the [Exa Agent](#exa-agent) tools for async research, list-building, and enrichment                                     |

Enable specific tools:

```
https://mcp.exa.ai/mcp?tools=web_fetch_exa
```

Enable all search tools (see [Exa Agent](#exa-agent) below to add the Agent tools):

```json theme={null}
{
  "exa": {
    "url": "https://mcp.exa.ai/mcp?tools=web_search_exa,web_fetch_exa,web_search_advanced_exa",
    "headers": {
      "x-api-key": "YOUR_EXA_API_KEY"
    }
  }
}
```

<br />

# Exa Agent

[Exa Agent](/reference/agent-api-guide) is Exa's async, high-compute endpoint for multi-step research, list-building, enrichment, and structured output — work that needs more than a single search call. Its tools run on the same server and are off by default.

Agent runs are usage-based, so the Agent tools require authentication — connect with OAuth or pass your own [Exa API key](https://dashboard.exa.ai/api-keys).

Enable the Agent tools with the `agent_tools` alias:

```
https://mcp.exa.ai/mcp?tools=agent_tools
```

Or alongside the default search tools:

```json theme={null}
{
  "exa": {
    "url": "https://mcp.exa.ai/mcp?tools=web_search_exa,web_fetch_exa,agent_tools",
    "headers": {
      "x-api-key": "YOUR_EXA_API_KEY"
    }
  }
}
```

## Agent tools

| Tool                   | Description                                                                                                                                  |
| ---------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| `agent_create_run`     | Start an async Agent run for multi-step research, list-building, enrichment, or structured output. Returns an `agent_run_...` ID immediately |
| `agent_wait_for_run`   | Poll a run until it reaches a terminal status (`completed`, `failed`, or `cancelled`) or times out                                           |
| `agent_get_run_output` | Retrieve completed output: text, structured JSON, grounding citations, usage, and cost                                                       |
| `agent_cancel_run`     | Cancel a queued or running run                                                                                                               |

## How it works

Agent runs are async and run-ID based. A typical loop:

1. **Create the run** with `agent_create_run`. Include an `outputSchema` whenever you need repeatable, structured results. Returns an `agent_run_...` ID.
2. **Wait for completion** with `agent_wait_for_run`, which polls until the run reaches a terminal status or times out. Call it again if the run is still going.
3. **Read the output** with `agent_get_run_output` once the run is `completed`. You get `output.text`, `output.structured` (when a schema was provided), `output.grounding` citations, plus usage and `costDollars`.
4. **Continue or cancel.** Pass `previousRunId` to `agent_create_run` to refine or extend a completed run, or use `agent_cancel_run` to stop a run that is clearly wrong.

`agent_create_run` accepts a natural-language `query` plus optional `outputSchema`, `systemPrompt`, `input` (`data` to enrich, `exclusion` to avoid), `dataSources` ([Exa Connect](/reference/agent-api/connect/overview) providers, up to 5), `previousRunId`, and `effort` (`low`, `medium`, `high`, `xhigh`, or `auto`).

See the [Exa Agent guide](/reference/agent-api-guide) for schema patterns, effort modes, Exa Connect data sources, and pricing.

<br />

# Resources

* [**GitHub**](https://github.com/exa-labs/exa-mcp-server) - View Exa MCP source code
* [**npm**](https://www.npmjs.com/package/exa-mcp-server) - Install Exa MCP npm package

<Accordion title="Usage Examples" icon="magnifying-glass">
  **Web Search**

  ```
  Search for recent developments in AI agents and summarize the key trends.
  ```

  **Code Search**

  ```
  Find Python examples for implementing OAuth 2.0 authentication.
  ```

  **Read a Page**

  ```
  Fetch the full content of https://exa.ai and summarize what the company does.
  ```
</Accordion>

<Accordion title="Troubleshooting" icon="wrench">
  **Rate limit error (429)**

  You've hit the free plan rate limit. Add your own API key to continue:

  ```json theme={null}
  {
    "exa": {
      "url": "https://mcp.exa.ai/mcp",
      "headers": {
        "x-api-key": "YOUR_EXA_API_KEY"
      }
    }
  }
  ```

  [Get your API key](https://dashboard.exa.ai/api-keys)

  **Tools not appearing**

  Restart your MCP client after updating the config file. Some clients require a full restart to detect new MCP servers.

  **Claude Desktop not connecting**

  Use the built-in Connector: click **+** (or **Add connectors**) → **Connectors** tab → search for **Exa** → click **+**.

  **Config file not found**

  Common config locations:

  * Cursor: `~/.cursor/mcp.json`
  * VS Code: `.vscode/mcp.json` (in project root)
  * Claude Desktop (macOS): `~/Library/Application Support/Claude/claude_desktop_config.json`
  * Claude Desktop (Windows): `%APPDATA%\Claude\claude_desktop_config.json`
</Accordion>
