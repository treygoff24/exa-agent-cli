# Why a CLI when the Exa MCP exists?

The Exa MCP is good at what it does. For a Claude Desktop or Cursor user who wants web search in their assistant, it is the right level of simplicity. We built `exa-agent` because we kept wanting things from the Exa API that an MCP server, by its nature, is not positioned to give an autonomous coding agent. 

## 1. Surface coverage

The MCP exposes 4 tools: `web_search_exa`, `web_fetch_exa`, `web_search_advanced_exa`, and `agent_run`. That covers `/search`, `/contents`, and the Agent API.

The Exa Public API (spec 2.0.0) also has: `/answer`, `/findSimilar`, `/context` (exa-code), the research endpoints, search Monitors (create/list/get/update/delete/trigger, plus runs), the entire Websets tree (websets, items, searches, enrichments, imports, monitors, webhooks, webhook attempts, events), and the Team Management API for key administration. None of that is reachable through the MCP today.

`exa-agent` wraps all of it: 68 generated API commands across ~20 namespaces, generated at build time from the committed OpenAPI spec, plus a `raw` passthrough so anything Exa ships tomorrow is usable before we model it. When an agent's job is "stand up a Webset, attach an enrichment, wire a webhook, monitor it weekly," the MCP has no verbs for any of those sentences.

## 2. Context-window economics

An MCP tool call has exactly one place to put its result: the model's context window. There is no cap and no overflow valve. A fat crawl or a 50-result search lands in context whole, and every tool's schema sits in context for the life of the session whether it gets used or not.

A CLI has a filesystem, and `exa-agent`'s defaults are built around that. Output over 48 KiB spills to a pretty-printed file automatically, and the envelope in context carries the path, a hash, and diagnostics instead of the payload (`--max-output-bytes` tunes it, `-o FILE` redirects it entirely). Search results default to query-aware highlights rather than full text; bare `--text` is capped, and the caps are flags, not surprises. Nothing about the tool is in context until the agent invokes it.

We measured this before building: early sessions with uncapped output were burning 24–44 KB of context per call on payloads the model skimmed once and never needed again. For a human reading one search in a chat window that cost is invisible. For an agent doing forty calls across a long task, it is the difference between finishing and compacting.

## 3. An agent-grade contract

The MCP returns what the server returns. `exa-agent` commits to a contract an autonomous caller can build on:

- One JSON envelope schema on every command (`exa.cli.response.v1`), with a published error-code dictionary and stable exit codes.
- Offline self-description: `capabilities --json`, `schema`, and `robot-docs` let an agent learn the full surface with zero network calls and zero tokens of preloaded schema.
- Mutation safety: create-POSTs are never auto-retried without an idempotency key; an ambiguous create leaves a pending-run record and a recovery command instead of a maybe-duplicate.
- Destructive-operation gates: deletes and key admin require explicit confirmation flags, and `capabilities` labels every command's blast radius so an agent can know before it acts.
- Cost visibility: every envelope carries `costDollars`, so an agent (or its supervisor) can meter spend per call.

This is what any of us would want from a tool our unattended agents drive hundreds of times a day. It is a different set of requirements than "give my chat assistant web search," and it pulls toward a different shape.

## Same API, different users

The MCP's primary user is a person with an assistant; `exa-agent`'s primary user is a program. The MCP optimizes for install friction and works beautifully there. The CLI optimizes for full surface, bounded context, and contractual behavior under automation, and pays for it with a `brew install` and an API key.

