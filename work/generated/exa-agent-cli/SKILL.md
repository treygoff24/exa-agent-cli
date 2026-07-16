---
name: exa-agent-cli
description: >-
  Become an instant expert at driving the `exa-agent` CLI — the agent-first
  command-line interface over the full Exa API (search, contents, answer, code
  context, agent runs, research, monitors, the whole Websets tree, and team/key
  admin). Use this WHENEVER you are about to call `exa-agent` or any Exa endpoint
  from the command line, or the user asks to search the web, fetch page contents,
  get a cited answer, pull code/docs context, build a Webset, run a monitor, or
  administer Exa API keys via the CLI. Load it BEFORE running exa-agent so you use
  its self-description instead of guessing flags. Trigger phrases include
  "exa-agent", "exa cli", "exa search", "exa answer", "exa contents", "exa
  context", "exa websets", "exa monitor", "Exa API from the command line".
---

<!-- Generated from `exa-agent robot-docs guide|commands|errors|examples`; staged for Wave 4b installation. -->

# exa-agent CLI

`exa-agent` is an agent-first CLI over the full Exa API. You are its primary
user. The single most important thing to know: **it describes itself, so you
never explore blind.** Every command, every flag, every field, every exit code,
and every error is emitted by the binary itself, offline, with no key and no
network. Guessing flags and running commands to "see what happens" is the wrong
mode — ask the binary and it tells you exactly.

The binary is `exa-agent`. It is pre-1.0; the surface is stable but versioned.

## The reflex: ask the binary, don't guess

Four offline commands replace all trial-and-error. Run them, don't improvise:

```sh
exa-agent capabilities --compact        # full inventory: every command + its metadata + the exit/error dictionaries
exa-agent schema show <command>         # exact flags for one command: flag → JSON bodyPath, which are required
exa-agent robot-docs errors             # the full exit-code + error-code dictionary
exa-agent <command> --help              # clap help; typo'd flags come back with "did you mean"
```

Also useful when orienting: `schema list` (all operations), `robot-docs guide`
/ `robot-docs examples` / `robot-docs commands` (built-in agent playbook),
`doctor` (offline health) and `doctor --online` (adds a live credential probe).

`capabilities` returns 73 commands (~35 KiB, ~9k tokens). Each command object
tells you what you need to act safely: `method`, `apiPath`, `readOnly`,
`destructive`, `requiresConfirm`, `idempotencySensitive`, `streaming`,
`deprecated`, and `pagination`. Read that metadata before you run anything —
it's how you know a command mutates, needs `--yes`, streams, or is deprecated,
without finding out the hard way. Pass a command path to get just that entry
instead of the full dump — `exa-agent capabilities search` returns one command
(~7 KiB) rather than all 73.

## Build any request without guessing

`schema show <command>` maps each flag to the JSON body path it fills and marks
required fields. For example `search` exposes `query` (required, → `query`),
`num-results` (→ `numResults`), `text` (→ `contents.text`), and so on.

Two ways to set body fields:
- **Named flags** — the ergonomic path: `exa-agent search "rust async" --num-results 5`.
- **`--set <bodyPath>=<value>`** — set any field by its JSON path, including
  ones without a dedicated flag: `--set contents.text=true --set numResults=3`.
  The `bodyPath` values come straight from `schema show`.

Then **prove the request before sending it**. `--dry-run --print-request` builds
the exact upstream body and prints it in the envelope's `data.request.body`
*without making the call* (`data.dryRun: true`, cost `0`). Do this before every
mutation and whenever you're unsure the body is shaped right:

```sh
exa-agent websets create --query "AI startups in SF" --count 25 --dry-run --print-request --compact
```

## The output contract (parse this, don't scrape)

**One JSON envelope per call.** Success is `exa.cli.response.v1`; errors are
`exa.cli.error.v1`. **stdout is data, stderr is diagnostics** — the parseable
result is always on stdout, trace/errors on stderr.

**Output format is TTY-dependent — this is the one thing that bites agents.**
In a pipe it emits JSON; in an interactive terminal it prints human-readable
text. When you run it programmatically and need guaranteed JSON, **pass `--json`**
(or `--ndjson` for streams, or `--compact` for single-line JSON). Don't assume
JSON just because you asked for data — force it.

**Never pipe exa-agent through `head`/`tail` to limit output** — the envelope is
one line of JSON, so line-based truncation silently does nothing. Size the
*request* instead (default highlights, `--text 1500`, `--num-results`), or rely
on `--max-output-bytes` (default 48 KiB) spilling oversized payloads to a
pretty-printed file at `dataPath`.

Success envelope, top-level keys worth knowing:
- `ok` (bool), `command`, `operation` (`method`/`path`/`operationId`).
- `data` — the payload. `count` when it's a collection.
- `costDollars.total` — what the call cost. Watch this when looping.
- `nextActions` — suggested follow-up commands for async/multi-step flows (e.g.
  poll a Webset). Follow them instead of inventing the next call.
- `dataTruncated` + `dataPath` — if a payload exceeded `--max-output-bytes`, the
  full body was spilled to `dataPath`. Read that file rather than treating the
  truncated inline data as complete.
- `warnings`, `diagnostics` (`durationMs`, `retries`), `pagination`, `dataHash`.
  Always-null/empty fields (e.g. `pagination`, `dataPath`) are omitted rather
  than emitted as `null` — don't assume a key's presence means it applies.
- `contents` and `fetch` add `outcome`: `no_content` (no successful results),
  `partial` (one or more URL failures), or `full` (successful results with no
  URL failures). Existing `ok`, warning, and exit behavior is unchanged: total
  URL failure still emits `ok: true` with exit `10` and `all_urls_failed`.

Error envelope carries a **stable, actionable** `error`:
- `error.code` (stable string, e.g. `not_found`, `rate_limited`, `not_authenticated`),
  `error.category`, `error.exitCode`, `error.retryable` (bool).
- `error.suggestedCommand` / `error.details.didYouMean` — the CLI tells you the
  fix. On a failure, **read the envelope and apply its suggestion** rather than
  thrashing. Retry only when `retryable: true`.

**Exit codes are stable and meaningful — branch on them, don't parse prose.**
`0` ok · `1` usage · `2` auth · `3` config · `4` network · `5` upstream ·
`6` rate_limit · `7` not_found · `8` conflict · `9` safety (destructive op
refused without `--yes`) · `10` partial (batch, check per-item statuses) ·
`11` no_input (required stdin/@file missing) · `12` interrupted (SIGINT/stream).
The full dictionary is `robot-docs errors`.

## Safety and money (non-negotiables)

- **Preview every mutation** with `--dry-run --print-request` before running it live.
- **Destructive ops (deletes, cancels) refuse without `--yes`** and exit `9`.
  Adding `--yes` is a deliberate act — do it only when you mean it.
- **No surprise double-billing.** The CLI never auto-retries a non-idempotent
  create-POST. For safe create retries, pass `--idempotency-key <key>`; it's
  forwarded upstream.
- **Never pass managed auth headers** via `--header`. Auth comes from the
  environment or `auth login` (below); hand-setting auth headers is refused.

## Auth

Environment-first:

```sh
export EXA_API_KEY=...       # primary Exa API credential
export EXA_SERVICE_KEY=...   # ONLY for `admin keys …` (Team Management API, separate host)
```

Or `exa-agent auth login` (reads a key from stdin into a `0600`
`~/.config/exa-agent-cli/credentials.json` — plaintext on disk, prefer the env
var). Verify with `exa-agent auth status` (shows which source resolved) and
`exa-agent auth test` (live probe, no billing). `auth logout` clears the stored
key. If a key is set in *both* the env and the credentials file, `auth status`
reports which one actually won — trust it over your assumptions.

## The escape hatch

Any endpoint — including ones not yet modeled as a typed command — is reachable
with the same auth/retry/output/error contracts:

```sh
exa-agent raw POST /search --body '{"query":"test","numResults":1}'
```

Use `raw` when a new Exa endpoint isn't in `capabilities` yet, or when you need
byte-exact control over the body.

## Live quirks worth knowing up front

These are real behaviors you'd otherwise waste calls discovering:

- **`team` with no subcommand runs `team info`** — it's a parent with exactly
  one child, so the bare form works. If your team plan lacks API access
  you'll get a clean `reauth_required` error explaining that, not a broken
  command.
- **`context "query"` works bare.** `--tokens` defaults to `dynamic`; pass
  `--tokens <50..100000>` only to override the budget.
- **`fetch`/`contents` distinguish partial from total failure.** If every
  requested URL fails, exit code is `10` with an `all_urls_failed` warning —
  don't read `ok: true, count: 0` as "the page is empty." If only some URLs
  fail, you get exit `0` with a per-URL warning; check `warnings` either way.
- **Empty contents reasons are stable.** When an upstream status has no usable
  reason, the warning uses `upstream_reason_unavailable` and suggests a direct
  fetch; the CLI never invents a reason.
- **Upstream error bodies are parsed, not dumped.** `error.message` is a clean
  sentence; the raw upstream JSON (capped at 4096 bytes) or an HTML preview
  lands in `error.details.upstream`/`bodyPreview` if you need it.
- **Unknown/missing subcommands list valid ones.** The error's
  `error.details.subcommands` enumerates what's actually callable, plus
  `error.suggestedCommand` — follow it instead of guessing.
- **`similar` is deprecated upstream** (Exa's `/findSimilar`). Prefer `search`.
- **Search is not cursor-paginated.** Use `--num-results`; if an invocation is
  rejected, follow `error.suggestedCommand` rather than reaching for a cursor.
- **`admin …` needs `EXA_SERVICE_KEY`** and hits a separate admin host. Without
  it you'll get an auth/scope error (exit `2`), by design.
- **Local-only mode.** `EXA_AGENT_NO_NETWORK=1` refuses live typed, raw,
  streaming, `auth test`, `auth status`, `schema refresh --check`, and
  `doctor --online` commands with a structured `usage_error` on stderr and exit
  `1`, before credential resolution or transport creation. Dry-run and
  self-description commands remain offline.

## Recipes

```sh
# Web search (force JSON for programmatic use). Default already returns
# query-aware highlight snippets per result — usually all you need for triage.
exa-agent search "rust async runtimes" --num-results 5 --json

# Parse search results with jq (the response path is `.data.results[]`).
exa-agent search "rust async runtimes" --num-results 1 --json | jq '.data.results[] | {title,url}'

# Restrict search to one domain; quote domains and URLs in shell recipes.
exa-agent search "AI infrastructure" --include-domain "exa.ai" --num-results 5 --json

# More/less snippet text: --highlights 800 (chars/result), --no-highlights (metadata only),
# --text (capped 1500 chars/result), --text 4000, --text full (whole pages — rarely what you want)

# Cited answer (or the `ask` macro — same thing, shorter to type)
exa-agent answer "what changed in the EU AI Act in 2025?" --json
exa-agent ask "what changed in the EU AI Act in 2025?" --json

# Page contents (or the `fetch` macro — contents + --text + summary query).
# The normal numeric cap is 10000 characters/result; use `full` only deliberately.
exa-agent contents "https://exa.ai" --text 10000 --json
exa-agent fetch "https://exa.ai" --json

# Code/docs context for a coding task — bare call works, --tokens is optional
exa-agent context "how to stream SSE in Rust with ureq" --json

# Async list-building: create a Webset, then follow nextActions to poll it
exa-agent websets create --query "AI startups in SF" --count 25 --json
exa-agent websets get <id> --json          # or run what nextActions told you

# Recurring search monitor
exa-agent monitor create --query "AI policy news" --webhook-url https://example.com/hook --json

# Any endpoint directly, same contracts
exa-agent raw POST /search --body '{"query":"test"}' --json
```

## Working loop (internalize this)

1. `capabilities --compact` to find the narrowest command and read its metadata.
2. `schema show <command>` to learn the exact flags/body paths and required fields.
3. Build the call; if it mutates or you're unsure, `--dry-run --print-request` first.
4. Run it with `--json`. Branch on the **exit code**; read the **envelope**.
5. On error, apply `error.suggestedCommand`/`didYouMean`; retry only if `retryable`.
6. For multi-step/async work, follow `nextActions` instead of inventing the next call.

Everything here is authoritative because the binary emits it. When in doubt about
a specific command or field, `schema show <command>` and `<command> --help` are
the source of truth — always current, because they're generated from the same
embedded spec the CLI runs on.

## Verification

The jq path in the search recipe was verified with one live, credential-backed
search using `--num-results 1`; only the shape check was retained, not the
response or any credential material. Other examples are offline parse/build
recipes generated from the canonical `robot-docs` surface.
