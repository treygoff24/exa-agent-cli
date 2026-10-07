# exa-agent

Search the web, retrieve pages, and run Exa workflows from scripts and AI agents.

Unofficial project; not affiliated with, endorsed by, or sponsored by Exa.

`exa-agent` is one self-contained Rust binary with offline command discovery, stable
exit codes, and JSON output. It covers search, contents, answers, agent runs,
monitors, Websets, and team/key administration. The binary is `exa-agent`; the
crate is `exa-agent-cli`.

```sh
brew install treygoff24/tap/exa-agent
```

The source and locally verified candidate are **0.8.0**. GitHub releases and
crates.io publication are pending; package-manager and latest-release installers
below retrieve published artifacts, which may have an earlier version. Check
`exa-agent --version` before using the new flags.

## Quick example

Set `EXA_API_KEY` through your environment or secret manager, then preview a
request before sending it:

```sh
exa-agent search "fusion energy" --objective "Find US demonstration projects" --user-location US --num-results 5 --dry-run --print-request
exa-agent search "fusion energy" --num-results 5 --json
exa-agent contents 'https://exa.ai' --text 1500 --json
exa-agent answer "Recent fusion energy milestones" --json
exa-agent websets create --query "AI startups in SF" --count 25 --dry-run --print-request
exa-agent capabilities search --json
```

Search defaults to query-aware highlights capped at 800 characters per result.
Use `--text N` for page text or `--no-highlights` for metadata only. API calls can
incur usage charges; dry-run and self-description commands use no key or network.

| Need | CLI behavior |
| --- | --- |
| Discover commands without loading every schema | `capabilities search --json` describes one command offline |
| Keep large results out of an agent's context | `--output FILE` saves the full result; oversized inline data spills automatically |
| Inspect a mutation before spending | `--dry-run --print-request` prints the resolved request |
| Branch on a failed call | Stable exit codes and `error.code` identify the cause |

## Install

```sh
# Homebrew (macOS/Linux)
brew install treygoff24/tap/exa-agent

# cargo
cargo install exa-agent-cli

# shell installer (from the GitHub release)
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/treygoff24/exa-agent-cli/releases/latest/download/exa-agent-cli-installer.sh | sh
```

All three install the `exa-agent` binary. Verify with `exa-agent --version`.

### Platforms and release artifacts

The release configuration targets six prebuilt archives. Windows is a deliberate non-goal.

| Platform | Target triple | Linkage |
| --- | --- | --- |
| macOS, Apple silicon | `aarch64-apple-darwin` | dynamic |
| macOS, Intel | `x86_64-apple-darwin` | dynamic |
| Linux arm64, glibc | `aarch64-unknown-linux-gnu` | dynamic |
| Linux arm64, static | `aarch64-unknown-linux-musl` | static |
| Linux x86_64, glibc | `x86_64-unknown-linux-gnu` | dynamic |
| Linux x86_64, static | `x86_64-unknown-linux-musl` | static |

The shell installer chooses the archive. On Linux it prefers the glibc archive and falls back to the
musl archive when the host's glibc is older than the release's recorded minimum or missing entirely, so Alpine, distroless,
and scratch images get a binary that actually runs. Ordinary glibc hosts receive the `-gnu` archive.

The musl archives are fully static. CI fails the build unless the binary carries no `PT_INTERP`
segment and no `NEEDED` shared-library entries, and unless it runs on an Alpine image with no
glibc present. Both Linux flavors are built for the architecture baseline, never `-C
target-cpu=native`; CI's static lane fails if the repository's cargo config or the build
environment narrows the CPU baseline.

If you pin an exact archive instead of using the installer, take `-musl` inside containers and
`-gnu` on an ordinary distribution.

Every artifact uses the same credential storage, described under
[Authentication](#authentication). Credentials are not stored in an OS keyring.


## Build and run

```sh
cargo build --release
./target/release/exa-agent search "rust async runtimes" --num-results 5
```

```sh
cargo run --bin exa-agent -- search "rust async runtimes" --num-results 5
```

The minimum supported Rust version is 1.85. Run the local MSRV gate before
opening a Rust change:

```sh
cargo +1.85 clippy --all-features --all-targets
```

After installing Rust 1.85, CI confirms the stricter
`cargo clippy --locked --all-features --all-targets -- -D warnings` variant.

### Development builds

The dev profile keeps line tables for this crate and omits dependency debug info.
For full debugging, override those profile settings locally. Use `cargo check`
while editing and targeted tests for changed behavior. Release builds keep the
architecture baseline: do not add `-C target-cpu=native` to shipped artifacts.

## Usage

Examples below send requests unless they include `--dry-run --print-request`:

```sh
# Search
exa-agent search "rust async runtimes" --num-results 5
# Search returns query-aware highlights capped at 800 chars/result by default; --highlights N
# for a different cap, --no-highlights for metadata only, or --text / --text 1500 / --text full
# for page text.
# Search, contents, and similar are cache-first by default. Use --fresh for latest/current tasks,
# --cache-only, --max-age-hours N, or --livecrawl-timeout MS for explicit freshness behavior.
exa-agent search "current AI policy" --fresh --text 1500
exa-agent search "AI policy" --snapshot-as-of 2026-09-01

# Cited answer
exa-agent answer "what changed in the EU AI Act in 2025?"
exa-agent answer "Recent fusion energy milestones" --model exa-pro --system-prompt "Use primary sources" --user-location US

# Give search a broader objective and a paired location hint
exa-agent search "fusion energy" --objective "Find recent US demonstration projects" --user-location US --latitude 37.77 --longitude -122.42

# Share a highlights budget across results (beta)
exa-agent search "fusion energy" --highlights '{"dynamic":true,"verbosity":"medium"}' --beta dynamic-highlights-2026-08-28

# Page contents
exa-agent contents 'https://exa.ai' 'https://exa.ai/docs' --text
# Contents accepts positional URLS or `--ids`. Text accepts bare, full, or N (1..1000000).
# --snapshot-as-of accepts an RFC 3339 date or date-time and cannot be combined with freshness,
# live-crawl, or subpage options.

# Legacy code/docs context (undocumented upstream; may change or disappear)
exa-agent context "how to stream SSE in Rust with ureq"

# Create a Webset (async structured list-building)
exa-agent websets create --query "AI startups in SF" --count 25
# Preview its decomposition and search for sample items (`--search` defaults to true when search.count is set)
exa-agent websets preview --query "AI startups in SF" --count 3 --search true

# Create a recurring search monitor
exa-agent monitor create --query "AI policy news" --webhook-url https://example.com/hook

# Call any Exa endpoint directly, with the same auth/output/error contracts
exa-agent raw POST /search --body '{"query":"test"}'
```

Before sending a mutation, inspect its resolved request by appending `--dry-run --print-request`:

```sh
exa-agent websets create --query "AI startups in SF" --count 25 --dry-run --print-request
```

Previews show the method, path, query, body, and explicit or feature-specific headers.
Credentials and HTTP-stack defaults such as `Content-Type`, `Host`, and `User-Agent`
are omitted. Only non-secret values belong in `--header`; custom headers are not
copied into automatic follow-up commands.

`--dry-run --print-request` still performs the same local body validation as a live call. If a modeled field is invalid (wrong type, out-of-range value, missing required field, or malformed `--body`/`--set`), the command exits `1` without printing a request; when the body is valid it prints the exact request body and exits `0` without sending it. Ordinary typed calls forward unknown body fields for upstream compatibility; `schema validate-input` and stored presets reject unknown fields where the
request structure is modeled. `schema validate-input` reports validation
in `valid`; a successful report can exit `0` with `valid: false`. `valid: null`
means structural validation is unsupported for that operation, not that input passed.

### Discovering the surface (offline, no key, no network)

These run with no credential and no network call:

```sh
exa-agent capabilities --json   # machine-readable inventory of all commands + exit/error codes
exa-agent robot-docs guide      # a short, paste-ready playbook for agents
exa-agent schema --help         # embedded API/CLI schema
exa-agent doctor                # offline health checks (add --online for a live probe)
```

`capabilities` lists all 71 commands with each one's HTTP method, path, and metadata (read-only vs. destructive, pagination style, streaming, deprecation, idempotency sensitivity), alongside the full exit-code and error-code dictionaries. Pass a command path (e.g. `exa-agent capabilities search`) to get just that command's entry instead of the full dump.

### Agent skill

The generated agent skill lives at `skills/exa-agent-cli/SKILL.md`. Copy the
`skills/exa-agent-cli/` directory into your agent's skills folder, such as
`.claude/skills/` or `~/.codex/skills/`. Maintainers regenerate it with
`cargo xtask generate-skill` from the offline `robot-docs` guide.

For a hard local-only boundary, set `EXA_AGENT_NO_NETWORK` to any value (including empty).
Its presence enables the guard; unsetting it is the only off state. Live typed, raw,
streaming, `auth test`, and `doctor --online` paths then return a structured
`usage_error` on stderr with exit 1 before credential resolution or transport;
`auth status` and `schema refresh --check` are also refused before credential
resolution or network access; dry-run request previews and self-description
commands still work.

### Command surface

| Family | Commands and scope |
| --- | --- |
| Core retrieval | `search`, `contents`, `answer`, `similar` (deprecated), and `context` (undocumented compatibility) |
| Agent runs | `agent runs create|get|list|events|cancel|stop|delete`; `agent run` aliases create |
| Batches | `batches create|list|get|cancel|delete` (alias `batch`); requires the Batch beta token and account access |
| Search monitors | `monitor` manages recurring searches |
| Websets | Websets, searches, items, enrichments, imports, monitors, webhooks, delivery attempts, and events |
| Team and admin | `team info` reports quota/concurrency; `admin keys` uses a separate service key and host |
| Raw requests | `raw METHOD PATH` calls endpoints outside the typed surface with shared auth/output/error handling |
| Offline tools | `capabilities`, embedded `schema`, `robot-docs`, default `doctor`, `config`, `preset`, and `macro` |

The retired `research` family is a local stub: it returns `research_retired` and
suggests `search --type deep-reasoning`. `auth test`, `auth status`, and
`schema refresh --check` can use the network. The undocumented `/context` route
may change or disappear; offline examples do not establish live support.

Raw payment discovery and signed stdin-only payment pass-through work only for
nonstreaming `raw POST /search` or `/contents` on the default host. Wallet custody
and signing are outside the CLI.

Search accepts `--objective TEXT` (up to 4,096 characters) independently of the
query. Search and answer accept `--user-location COUNTRY|JSON` and paired `--latitude` /
`--longitude` coordinates; latitude is -90..90 and longitude is -180..180.

Typed Ultra runs require an explicit `--max-cost-dollars` cap of $1..$100 as CLI safety
policy for typed commands; `raw` remains the pass-through escape hatch.
`--max-duration-seconds` optionally sets a soft Ultra wall-clock limit of
300..10,800 seconds; upstream stops starting work as the limit approaches.
Exa reports `time_limit_reached` when that limit stops a run. Legacy `--effort max`
fails locally with an Ultra migration command instead of changing effort silently.
The migration preview preserves the final body; restore any explicit profile, base URL, beta, and custom
headers manually before replay.

```sh
exa-agent agent run "Map fusion demonstration projects" --effort ultra --max-cost-dollars 5 --max-duration-seconds 600 --data-source macrobond --dry-run --print-request
```

Supported Websets mutations limit metadata keys to 250 Unicode characters,
including nested enrichment metadata on Webset creation. This applies to Webset
create/update, search create, enrichment create/update, import create, and webhook
create/update. Agent/Batch metadata is outside this Websets rule. Nested
`search.metadata` is not defined by the upstream Webset creation schema.

The SDK-only beta `/agent/monitors` family is intentionally absent from the typed
CLI because it has no public documented spec or contract. Use the documented
`monitor` or `websets monitors` commands for recurring work; `raw` remains available
when you have an explicit endpoint contract.

### Asynchronous batches

```sh
exa-agent batches create --requests '[{"customId":"news","method":"POST","url":"/search","body":{"query":"fusion energy"}}]' --dry-run --print-request
exa-agent batches list --status completed --limit 100 --all
exa-agent batches get batch_abc123
exa-agent batches cancel batch_abc123 --yes
```

`--requests` and `--metadata` accept inline JSON or `@file`; `--body` and `--set`
override them. Each request needs a unique `customId` and an object body, with
streaming disabled. `batches get` returns a fresh, short-lived `resultsUrl` and
a credential-free download command in `nextActions`. Treat that URL as a bearer
credential; fetch it directly without sending your Exa API key.

Batch creation is never automatically retried, even with `--idempotency-key`:
Exa does not document deduplication for this beta. An ambiguous failure records
the request and points to a scoped batch listing instead of risking a duplicate.

`agent runs stop ID --yes` completes an Ultra run early with the results it
has gathered and still incurs accrued usage. `agent runs cancel ID --yes`
terminates the run and discards those results, so both are gated the same way.
Ultra creation and early stop require no beta header. Batch commands still add
their required beta token.

### Presets and macros

Request presets live in `~/.config/exa-agent/presets.toml`. A repo can override them with
`.exa-agent/presets.toml` at its Git root. The local definition wins when both files define the
same name.

```toml
[presets.news-fresh]
command = "search"

[presets.news-fresh.body]
category = "news"
numResults = 10
```

```sh
exa-agent preset list
exa-agent preset show news-fresh
exa-agent search "AI policy" --preset news-fresh --dry-run --print-request
exa-agent macro show ask
exa-agent macro show fetch
```

Preset values are defaults: explicit flags, `--body`, and `--set` win. For modeled request structures, preset bodies allow documented properties and
reject unknown keys before merging with flags, `--body`, or `--set`. Required
fields and cross-field constraints are checked on the final merged request. `macro show` exposes the canonical expansion for the built-in `ask` and `fetch` macros.

### Doctor repair and undo

Bare `doctor` remains read-only and offline. `doctor --fix` is an explicit, opt-in mutation that
repairs only canonical TOML formatting and config-file permission bits (0600). It creates one
wall-clock-timestamped, byte-preserving config backup and a `*-latest` marker before writing the
config. `doctor --undo` restores only the latest marker backup (single-slot, pre-last-fix state
only). `--fix --dry-run` plans the same actions and exits `0` when every planned finding would be
fixed. It reports every planned, fixed, skipped, or refused action in the existing
`exa.cli.doctor.v1` envelope.

```sh
exa-agent doctor --fix --dry-run       # plan only, exits 0 if only planned actions
exa-agent doctor --fix                 # safe config repairs
exa-agent doctor --fix --allow-auth    # may secure credential-file permissions
exa-agent doctor --fix --allow-delete  # may delete spill files older than seven days
exa-agent doctor --undo                # restore the latest config backup
```

Auth-file changes require `--allow-auth` because they touch credential storage. Stale spill cleanup
requires `--allow-delete` because it deletes local data. `doctor --undo` is config-only: it does not
reverse credential-file permission changes or spill deletions. Network checks still require `--online`.

## Output contract

Structured JSON success uses `exa.cli.response.v1`; errors use
`exa.cli.error.v1` with `error.code` on stderr. Stdout carries results. Raw,
streaming, human, and chunked output have their own shapes.

| Setting or result | Behavior |
| --- | --- |
| Default format | JSON when piped, human-readable in a TTY; pass `--json` for automation |
| `--ndjson` | One record per line for list or stream output, with summaries as applicable |
| `--raw` | Decoded upstream body bytes without an envelope, subject to signed-payment echo redaction |
| Live contents/fetch/answer/ask | Text-aware `outcome`: `full`, `partial`, or `no_content`, independent of exit category |
| `--output FILE` | Complete selected output in a file; stdout carries a confirmation with `dataPath` |
| Oversized inline data | Automatic spill above 48 KiB; `data` becomes `null`, and `dataPath` points to the former `data` object |

Read `.results` from an automatic spill file; read `.data.results` from a complete
JSON envelope written with `--output`. `--max-output-bytes 0` disables auto-spill.

Exit codes distinguish usage (`1`), auth (`2`), network (`4`), upstream (`5`),
rate limits (`6`), missing resources (`7`), confirmation required (`9`), and billing
(`13`), among others. `capabilities --json` publishes the complete dictionary.
`insufficient_credits` is exit `13`; a challenge-evidenced raw payment 402 is
`payment_required` / exit `2`.

Most destructive operations require `--yes`; admin key deletion uses
`--confirm ID`, and live monitor batch deletion requires both `--yes` and
`--confirm delete`.
Create-POSTs are never auto-retried without `--idempotency-key`, which is forwarded
upstream. Batch creates are never auto-retried even with a key.

## Transport, bulk output, and local state

Raw output preserves HTTP content-decoded body bytes, including gzip decoding;
it is not a wire-level compressed-byte capture. Signed-payment echo redaction
still applies.

`--max-response-bytes N` limits decoded response bytes, including gzip inflation
and the entire successful SSE stream or JSON fallback. The default is 64 MiB
(67,108,864 bytes); config key `max_response_bytes` supplies the same setting.
Zero and malformed values fail before sending (exit 1). Exceeding the cap returns
`response_too_large`, nonretryable exit 5. The response is incomplete; a create's
outcome may be unknown, so use its recovery suggestion rather than repeat it.
HTTP errors use a separate bounded diagnostic read, preserving status and payment
challenge classification. This receive cap is separate from `--max-output-bytes`,
which controls inline output and spilling.

`--connect-timeout DURATION` (config key `connect_timeout`) limits connection
setup for normal, nonredirecting, and streaming requests. It does not replace
`--timeout`, the whole-request budget. Without it there is no additional connect
limit. Flag values override config values.

`contents URL... --chunk-size N --jobs J` fetches independent chunks with 1-16
workers, default 1. Explicit `--jobs` requires `--chunk-size`; ordinary contents
without `--jobs` is unchanged. Results remain in input order. After a request, per-item, or
output failure, no new round starts; all already-started results are drained.
`--output FILE` keeps every successful chunk in one file and emits one final
confirmation. JSON files contain a sequence of chunk envelopes; NDJSON files
contain each chunk's records and summary, and human output concatenates the chunk
renderings. Existing file modes and symlinks are preserved. Completed chunks
survive later failures; an unwritable chunk falls back to stdout. A failed final
rename identifies the staging path. Multi-chunk `--raw` is not supported.

New managed files and directories use 0600/0700. Existing permissions are
report-only unless an explicit doctor repair applies. Empty or relative XDG roots
fall back to `$HOME`; explicit `EXA_AGENT_*` paths may be relative. Config and
doctor mutations use a shared lock; dry-run repairs create no files or locks.
See the [contracts](docs/v2/contracts.md) for local-state, trace, and write-failure details.

## Authentication

Authentication is environment-first. Set the key in the environment for ordinary use:

```sh
export EXA_API_KEY=...        # primary credential for the Exa API
export EXA_SERVICE_KEY=...    # required only for `admin keys …` (Team Management)
```

Alternatively, `exa-agent auth login` reads a key from stdin and writes it to a credentials file at `~/.config/exa-agent-cli/credentials.json` (mode `0600`). That file is plaintext on disk. Prefer the environment variable and protect stored credentials. `exa-agent auth status` shows which source resolved the active credential, and `exa-agent auth logout` clears the stored key.

This is true on every platform and every release artifact. The crate carries a `keyring` cargo
feature, on by default, but it is currently inert: no code is compiled conditionally on it, so
it does not enable an OS keyring. Earlier design notes under `docs/v2/`
describe a keyring-backed credential store and a keyring-free musl build (D11/D15); that split is
planned, not implemented. Treat `auth login` as plaintext-file storage until this README says
otherwise.

## Troubleshooting and limitations

| Failure | Next step |
| --- | --- |
| Exit 1, usage | Read `error.message` and `suggestedCommand`; preview the corrected request |
| Exit 2, missing or rejected credential | Set `EXA_API_KEY`; admin key commands need `EXA_SERVICE_KEY` |
| Exit 13, `insufficient_credits` | Top up the Exa account; changing flags or retrying will not help |
| Partial or empty page content | Inspect `contentDiagnostics` and follow `nextActions`; government PDF recovery uses `firecrawl scrape 'URL' --max-age 0` |
| Output write fails after a successful call | Save the complete result left on stdout; repeating a create may duplicate it |

Firecrawl is a separate tool and must be installed and configured separately.
Exa does not provide trustworthy raw PDF bytes. Account entitlements can limit
Batch, Snapshot, and other APIs. Windows is unsupported. The retained `/context`
route is undocumented upstream, so offline examples do not establish live support.

### Common questions

**Why a CLI when an SDK or MCP exists?** Use an SDK inside an application, an MCP
for assistant integration, and the CLI for shell workflows with output files and
exit codes. See [Why a CLI](docs/why-a-cli.md).

**Can I inspect a request without an API key?** Yes. Add
`--dry-run --print-request`, or use `capabilities --json`.

**Does login use an OS keyring?** No. `auth login` writes a plaintext 0600 file;
prefer the environment variable.

## Design and maintenance

The [design index](docs/v2/README.md) preserves decisions, contracts, command
plans, and reviews. Historical plans can differ from the running binary,
particularly credential storage and flags. [Monthly maintenance](docs/monthly-update.md)
covers checks and release preparation; [spec provenance](openapi/PROVENANCE.md)
identifies the committed API inputs. `work/` holds ignored local evidence and is
not distributed.

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at your option.
