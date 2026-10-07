# Why a CLI when SDKs and MCP exist?

`exa-agent` makes Exa usable from shell workflows and unattended agents. An SDK
fits application code; an MCP connects an assistant to a tool server. A CLI gives
the caller files, pipes, and process exit codes without requiring either integration.

## Choose the interface for the job

| Interface | Useful when | What the caller manages |
| --- | --- | --- |
| Exa SDK | Exa requests are part of an application | Application code, SDK dependencies, and result handling |
| Exa MCP | An assistant needs Exa tools | Tool-server setup and the assistant's handling of results |
| `exa-agent` | A script or coding agent needs inspectable requests and saved output | One binary, an API key, and command invocations |

The CLI has 71 API commands across search, contents, answer, agent runs, batches,
monitors, Websets, and key administration. `capabilities --json` describes the
installed surface offline. `raw METHOD PATH` supports requests that the typed
commands do not model. SDK-only beta Agent Monitors are deliberately omitted from
the typed surface, and the retained `/context` route is undocumented upstream.

```sh
exa-agent capabilities websets create --json
exa-agent websets create --query "AI startups in SF" --count 25 --dry-run --print-request
```

## Keep result size under the caller's control

Search defaults to query-aware highlights rather than full page text. Inline
`data` over 48 KiB spills automatically to a JSON file; the envelope carries
`dataPath`, a hash, and diagnostics. `--max-output-bytes` changes the threshold.
An explicit `--output FILE` saves the complete selected output instead.

```sh
exa-agent search "fusion energy demonstration projects" --num-results 10 --json
exa-agent contents 'https://exa.ai' --text 1500 --output page.json --json
```

This lets an agent read only the result fields it needs while retaining the full
response on disk. Tool-result size in an MCP depends on the server and host;
there is no need to assume every MCP loads an unlimited response into context.

## Handle failures and mutations predictably

Structured JSON success uses `exa.cli.response.v1`; errors use
`exa.cli.error.v1` on stderr with stable exit codes and `error.code` values.
Streaming, raw, human, and chunked output have their own shapes. Signed-payment
raw output redacts exact submitted payment credential echoes.

Create-POSTs are never automatically retried without an idempotency key. Batch
creates are never retried even with a key because Exa does not document Batch
deduplication. An ambiguous create retains recovery information. Deletes and
cancels require confirmation, and `capabilities` describes command safety before
execution. Upstream cost information is exposed when available; the caller can
also cap typed Ultra runs explicitly.

```sh
exa-agent agent run "Map fusion demonstration projects" --effort ultra --max-cost-dollars 5 --max-duration-seconds 600 --dry-run --print-request
```

The CLI adds these automation contracts to Exa's API. It still depends on Exa
credentials, account entitlements, and upstream availability. See the
[README](../README.md) for installation, output details, and troubleshooting.
