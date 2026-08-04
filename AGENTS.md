# exa-agent — agent guide

You are probably an AI agent setting this up for a human, or using it yourself. This file is the complete contract. The README is for humans; everything you need is here.

Unofficial project; not affiliated with, endorsed by, or sponsored by Exa.

## What this tool does

`exa-agent` is a single static binary that exposes the full Exa API — search, contents, answer, code context, agent runs, monitors, the whole Websets tree (including exports), and team/key administration — as 67 non-interactive commands. Every call returns a stable exit code, and every structured (non-`raw`) success prints exactly one JSON envelope — `raw` prints the upstream bytes as-is, and streaming and human-format output differ by design. It can describe its own surface offline, with no key and no network call.

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

**Repo-work rule:** every local `exa-agent` invocation used for generated docs, examples, or
probing MUST export `EXA_AGENT_NO_NETWORK` (any value, including empty) to prevent unintended
billed live calls; unset it only for an intentionally live test. The guard refuses live typed, raw, streaming, `auth test`/`status`,
`schema refresh --check`, and `doctor --online` before credential resolution; dry-run and
self-description commands still work.

## Reading the output

Success envelope (`exa.cli.response.v1`, stdout): `data` carries the command's result, shaped per-command; async-create and paginated commands also carry `nextActions` (paste-ready follow-up commands), `count`, and `dataHash`. Live `contents`/`fetch` and `answer`/`ask` result envelopes carry text-aware `outcome` (`full`, `partial`, or `no_content`) independently of exit classification. They also carry `contentDiagnostics[]`: contents entries expose exact upstream `crawl_status`, `error_tag`, and `http_status` when present plus honestly inferred `content_type`, `content_status`, `usable`, and `pdf_unextracted`; answer currently emits `[]` because Exa provides no per-citation diagnostics. Empty/binary/PDF/crawl failures always add a warning and fallback action. `request.correlationId` echoes `--correlation-id`/`EXA_CORRELATION_ID` if you set one.

Error envelope (`exa.cli.error.v1`, stderr): `error.code` (from the published dictionary below), `error.message`, and often `suggestedCommand`. Stdout stays empty on error.

Output format is automatic — JSON when stdout is piped, human-readable in a TTY. Always pass `--json` (alias for `--format json`) when you are the consumer, so behavior doesn't depend on how you were invoked. `--raw` emits the exact upstream bytes with no CLI envelope. `-o/--output FILE` writes the complete selected output (exact bytes for `--raw`) to `FILE`; stdout carries only a small confirmation envelope with `dataPath`, and an explicit output path supersedes state-dir auto-spill.

## Exit codes

| Code | Name | Meaning |
| ---: | --- | --- |
| 0 | ok | success |
| 1 | usage | bad invocation, parse error, or local validation failure (missing required body field, unknown field, out-of-range value, malformed `--body`/`--set`) |
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
| 13 | billing | 402; the Exa account is out of credits (key is valid, command was fine) |

`error.code` is the finer-grained signal — 33 codes map onto these 14 exit categories (e.g. `not_authenticated` and `reauth_required` both map to exit `2`, so you can branch "set a key" vs "rotate the key"). The full `error.code` dictionary is in `capabilities --json`; if this file and `capabilities` disagree, trust `capabilities` — it is generated from the code.

**Out of credits is exit `13` / `insufficient_credits`, never exit `1`.** A 402 means the credential is valid and the invocation was well-formed — the account just cannot pay. Retrying and re-guessing flags is wasted effort; top up at https://dashboard.exa.ai or move the task to another research lane. `exa-agent auth test` and `doctor --online` report this state without spending anything, and are the only credit preflight available: the Exa API publishes no balance endpoint, so exhaustion is observable only as a 402 on the billing-free probe.

Dispatch-level body validation runs before credential resolution and network I/O. Body-level mistakes (unknown fields, out-of-range values, missing required fields, or a malformed `--body`/`--set`) exit `1` as a local `usage` error rather than being sent upstream and returning `5`. `--dry-run --print-request` still performs this validation and exits `1` without printing a request when the body is invalid; when the body is valid it prints the exact request body and exits `0` without sending it.

## Safety model

- Destructive operations (deletes, cancels) refuse to run without `--yes` and exit `9` otherwise.
- Create-POSTs never auto-retry without `--idempotency-key` — retrying a create on a post-send timeout can double-bill. An ambiguous create failure writes a local pending-run record and the error names the exact recovery command.
- `--dry-run --print-request` works on every mutation: it builds and prints the exact request body without sending it.
- `--header` cannot override managed auth headers (`Authorization` or other secret headers) — refused at exit `1`.

## Machine self-description

These run with no credential and no network call:

```sh
exa-agent capabilities --json    # all 67 commands: method, path, read-only/destructive/idempotency-sensitive, full exit-code + error-code dictionaries, embedded spec hash
exa-agent robot-docs guide        # short paste-ready playbook for agents
exa-agent schema --help           # embedded API/CLI schema
exa-agent doctor                  # read-only health checks (add --online for a live probe)
```

`doctor --fix` is an explicit, opt-in mutation: it repairs only canonical TOML formatting and
config-file permission bits (0600) after creating one wall-clock-timestamped config backup plus a
`*-latest` marker. `--fix --allow-auth` may also secure the credential-file permissions;
`--fix --allow-delete` may delete spill files older than seven days. `--undo` restores only the
latest marker backup (single-slot, pre-last-fix state only) and is config-only: it does not reverse
credential-file permission changes or spill deletions. `--fix --dry-run` plans the same actions and
exits `0` when only planned actions remain.

If anything in this file disagrees with `capabilities` output, trust `capabilities`.

## Maintainers

Release process and CI are driven by cargo-dist (`dist-workspace.toml`); the design record lives under `docs/v2/`, starting with `docs/v2/decisions.md`. Local working docs (audits, reviews, plans, journals, research) belong in `work/`, which is gitignored — keep them out of the repo; `work/generated/` is the tracked exception pinned by tests.
