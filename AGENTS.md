# exa-agent — agent guide

You are probably an AI agent setting this up for a human, or using it yourself. This file is the complete contract. The README is for humans; everything you need is here.

Unofficial project; not affiliated with, endorsed by, or sponsored by Exa.

## What this tool does

`exa-agent` is a single static binary that exposes the full Exa API — search, contents, answer, code context, agent runs, research, monitors, the whole Websets tree, and team/key administration — as 68 non-interactive commands. Every call prints exactly one JSON envelope and returns a stable exit code. It can describe its own surface offline, with no key and no network call.

## Install

```sh
brew install treygoff24/tap/exa-agent
# or
cargo install exa-agent-cli
# or
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/treygoff24/exa-agent-cli/releases/latest/download/exa-agent-cli-installer.sh | sh
```

Verify: `exa-agent --version`.

## Setup for your human

One secret is required for ordinary use, a second only for admin commands. Do not guess them; ask your human to provide them or set them in the environment/secret manager you have access to:

1. `EXA_API_KEY` — primary credential, from https://exa.ai
2. `EXA_SERVICE_KEY` — only needed for `admin keys …` (Team Management API); separate from `EXA_API_KEY`, never interchangeable

Then self-verify without spending:

```sh
exa-agent doctor --json          # offline: config parse, key presence, base URL, embedded spec hash, binary provenance
```

And with a live credential probe (a billing-free `POST /search` with an empty body) once a key is set:

```sh
exa-agent doctor --online --json
```

`doctor` uses its own exit dictionary (`0` healthy, `1` findings, `4` refused-unsafe) — not the general exit-code table below — so a `doctor` exit can never be confused with a real command failure. To confirm a credential actually works for a specific call, use `exa-agent auth test`.

## Canonical invocations

```sh
exa-agent search "rust async runtimes" --num-results 5
exa-agent answer "what changed in the EU AI Act in 2025?"
exa-agent contents https://exa.ai --text
exa-agent context "how to stream SSE in Rust with ureq"
exa-agent websets create --query "AI startups in SF" --count 25
exa-agent monitor create --query "AI policy news" --webhook-url https://example.com/hook
exa-agent raw POST /search --body '{"query":"test"}'
```

Before running any mutation for real, preview the exact upstream request it would send — without sending it:

```sh
exa-agent websets create --query "AI startups in SF" --count 25 --dry-run --print-request
```

## Reading the output

Success envelope (`exa.cli.response.v1`, stdout): `data` carries the command's result, shaped per-command; async-create and paginated commands also carry `nextActions` (paste-ready follow-up commands), `count`, and `dataHash`. `request.correlationId` echoes `--correlation-id`/`EXA_CORRELATION_ID` if you set one.

Error envelope (`exa.cli.error.v1`, stderr): `error.code` (from the published dictionary below), `error.message`, and often `suggestedCommand`. Stdout stays empty on error.

Output format is automatic — JSON when stdout is piped, human-readable in a TTY. Always pass `--json` (alias for `--format json`) when you are the consumer, so behavior doesn't depend on how you were invoked. `--raw` emits the exact upstream bytes with no CLI envelope.

## Exit codes

| Code | Name | Meaning |
| ---: | --- | --- |
| 0 | ok | success |
| 1 | usage | bad invocation, parse error, or local validation failure |
| 2 | auth | missing, invalid, or wrong-scope credential |
| 3 | config | malformed config or unknown profile |
| 4 | network | connection/timeout failure reaching Exa |
| 5 | upstream | Exa returned a non-2xx the CLI maps to a server error |
| 6 | rate_limit | 429; budget or concurrency exhausted |
| 7 | not_found | resource does not exist |
| 8 | conflict | duplicate/externalId conflict |
| 9 | safety | destructive op refused without confirmation (pass `--yes`) |
| 10 | partial | batch partially succeeded (per-item statuses) |
| 11 | no_input | required stdin/@file input absent, or a TTY would block |
| 12 | interrupted | SIGINT / stream interrupted |

`error.code` is the finer-grained signal — 27 codes map onto these 13 exit categories (e.g. `not_authenticated` and `reauth_required` both map to exit `2`, so you can branch "set a key" vs "rotate the key"). The full `error.code` dictionary is in `capabilities --json`; if this file and `capabilities` disagree, trust `capabilities` — it is generated from the code.

## Safety model

- Destructive operations (deletes, cancels) refuse to run without `--yes` and exit `9` otherwise.
- Create-POSTs never auto-retry without `--idempotency-key` — retrying a create on a post-send timeout can double-bill. An ambiguous create failure writes a local pending-run record and the error names the exact recovery command.
- `--dry-run --print-request` works on every mutation: it builds and prints the exact request body without sending it.
- `--header` cannot override managed auth headers (`Authorization` or other secret headers) — refused at exit `1`.

## Machine self-description

These run with no credential and no network call:

```sh
exa-agent capabilities --json    # all 68 commands: method, path, read-only/destructive/idempotency-sensitive, full exit-code + error-code dictionaries, embedded spec hash
exa-agent robot-docs guide        # short paste-ready playbook for agents
exa-agent schema --help           # embedded API/CLI schema
exa-agent doctor                  # read-only health checks (add --online for a live probe)
```

If anything in this file disagrees with `capabilities` output, trust `capabilities`.

## Maintainers

Release process and CI are driven by cargo-dist (`dist-workspace.toml`); the design record lives under `docs/v2/`, starting with `docs/v2/decisions.md`.
