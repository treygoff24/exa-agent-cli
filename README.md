# exa-agent

An agent-first command-line interface over the full [Exa](https://exa.ai) API.

Unofficial project; not affiliated with, endorsed by, or sponsored by Exa.

`exa-agent` exposes every documented Exa capability — search, contents, answer, code context, agent runs, monitors, the whole Websets tree (including exports), and team/key administration — as a single self-contained Rust binary (the Linux musl artifacts are fully static). It is built for AI agents as the primary user: every command is non-interactive, has a stable exit code, and can describe itself offline. Structured (non-`raw`) commands print one JSON envelope on success (`--ndjson`: one per line); `raw` prints decoded upstream body bytes except signed payment replaces exact submitted payment credential echoes with `<redacted>`, and streaming/human-format output differ by design. A human can drive it too, but the defaults are tuned for a program calling it, not a person typing at a prompt.

The binary is `exa-agent`. The crate is `exa-agent-cli`. It is pre-1.0 and built from a committed copy of the Exa Public API spec (2.0.0) plus the Team Management spec (1.0.0).

## Install

Pick whichever fits your setup:

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

The 0.7.0 release configuration targets six prebuilt archives. These additions
remain Unreleased. Windows is a deliberate non-goal.

| Platform | Target triple | Linkage |
| --- | --- | --- |
| macOS, Apple silicon | `aarch64-apple-darwin` | dynamic |
| macOS, Intel | `x86_64-apple-darwin` | dynamic |
| Linux arm64, glibc | `aarch64-unknown-linux-gnu` | dynamic |
| Linux arm64, static | `aarch64-unknown-linux-musl` | static |
| Linux x86_64, glibc | `x86_64-unknown-linux-gnu` | dynamic |
| Linux x86_64, static | `x86_64-unknown-linux-musl` | static |

The shell installer chooses for you. On Linux it prefers the glibc archive and falls back to the
musl archive when the host's glibc is older than the release's recorded minimum or missing entirely, so Alpine, distroless,
and scratch images get a binary that actually runs. Nothing changes for an existing glibc user:
the same `-gnu` archive is still what they receive.

The musl archives are fully static. CI fails the build unless the binary carries no `PT_INTERP`
segment and no `NEEDED` shared-library entries, and unless it runs on an Alpine image with no
glibc present. Both Linux flavors are built for the architecture baseline, never `-C
target-cpu=native`; CI's static lane fails if the repository's cargo config or the build
environment narrows the CPU baseline.

If you pin an exact archive instead of using the installer, take `-musl` inside containers and
`-gnu` on an ordinary distribution.

Credentials work identically on every artifact, and none of them uses an OS keyring — see
[Authentication](#authentication).


## Build and run

Build from source:

```sh
cargo build --release
./target/release/exa-agent search "rust async runtimes" --num-results 5
```

During development you can run it through cargo:

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

### Local build knobs

The dev profile keeps line tables only (`debug = "line-tables-only"`) and drops debug info for
dependencies. Backtraces through this crate's own frames still carry file and line; dependency
frames keep symbol names but lose line numbers. For a full debugging session, delete the
`[profile.dev]` and `[profile.dev.package."*"]` blocks in `Cargo.toml` rather than working around
them. Release and `dist` builds are unaffected. In the coordinator's Linux
comparison, the debug binary shrank from 82,861,952 to 33,311,480 bytes. Single cold
builds took 11.63s and 11.80s respectively: this supports a size reduction, not a
build-speed claim. Project line tables were checked with `readelf`.

Other things that help, roughly in order of payoff:

- `cargo check` while editing; `cargo build` only when you need to run it.
- A single test filter (`cargo test <name>`) instead of the whole suite.
- A faster linker such as `mold` or `lld`, configured in your own `~/.cargo/config.toml`. The
  repository's `.cargo/config.toml` is deliberately limited to the `xtask` alias so that CI and
  new contributors get the stock toolchain with no surprises.

Do not add `-C target-cpu=native` to anything you intend to ship or hand to someone else. Release
artifacts have to run on any CPU of their architecture, and CI rejects a narrowed baseline.


## Usage

A few real commands (all verified to parse):

```sh
# Search
exa-agent search "rust async runtimes" --num-results 5
# Search returns query-aware highlights capped at 800 chars/result by default; --highlights N
# for a different cap, --no-highlights for metadata only, or --text / --text 1500 / --text full
# for page text.

# Cited answer
exa-agent answer "what changed in the EU AI Act in 2025?"
exa-agent answer "Recent fusion energy milestones" --model exa-pro --system-prompt "Use primary sources" --user-location US

# Share a highlights budget across results (beta)
exa-agent search "fusion energy" --highlights '{"dynamic":true,"verbosity":"medium"}' --beta dynamic-highlights-2026-08-28

# Page contents
exa-agent contents https://exa.ai https://docs.exa.ai --text
# Contents accepts positional URLS or `--ids`. Text accepts bare, full, or N (1..10000).

# Code/docs context for a coding agent
exa-agent context "how to stream SSE in Rust with ureq"

# Create a Webset (async structured list-building)
exa-agent websets create --query "AI startups in SF" --count 25

# Create a recurring search monitor
exa-agent monitor create --query "AI policy news" --webhook-url https://example.com/hook

# Call any Exa endpoint directly, with the same auth/output/error contracts
exa-agent raw POST /search --body '{"query":"test"}'
```

Before running any mutation for real, preview the exact upstream request it will send — without sending it — by appending `--dry-run --print-request`:

```sh
exa-agent websets create --query "AI startups in SF" --count 25 --dry-run --print-request
```

Previews show the method, path, query, body, and explicit or feature-specific headers.
Credentials and HTTP-stack defaults such as `Content-Type`, `Host`, and `User-Agent`
are omitted. Only non-secret values belong in `--header`; custom headers are not
copied into automatic follow-up commands.

`--dry-run --print-request` still performs the same local body validation as a live call. If the body is invalid (unknown field, out-of-range value, missing required field, or malformed `--body`/`--set`), the command exits `1` without printing a request; when the body is valid it prints the exact request body and exits `0` without sending it.

### Discovering the surface (offline, no key, no network)

The CLI describes itself, which is the point of the agent-first design. These run with no credential and no network call:

```sh
exa-agent capabilities          # machine-readable inventory of all commands + exit/error codes
exa-agent robot-docs guide      # a short, paste-ready playbook for agents
exa-agent schema --help         # embedded API/CLI schema
exa-agent doctor                # offline health checks (add --online for a live probe)
```

`capabilities` lists all 73 commands with each one's HTTP method, path, and metadata (read-only vs. destructive, pagination style, streaming, deprecation, idempotency sensitivity), alongside the full exit-code and error-code dictionaries. Pass a command path (e.g. `exa-agent capabilities search`) to get just that command's entry instead of the full dump.

For a hard local-only boundary, set `EXA_AGENT_NO_NETWORK` to any value (including empty).
Its presence enables the guard; unsetting it is the only off state. Live typed, raw,
streaming, `auth test`, and `doctor --online` paths then return a structured
`usage_error` on stderr with exit 1 before credential resolution or transport;
`auth status` and `schema refresh --check` are also refused before credential
resolution or network access; dry-run request previews and self-description
commands still work.

### Command surface

- **Core retrieval** — `search`, `contents`, `answer`, `context`, and `similar` (deprecated upstream).
- **Agent runs** — `agent runs create|get|list|events|cancel|stop|delete`; `create` streams and supports metered `--max-cost-dollars` caps for `auto`/beta `max` effort.
- **Batches** — `batches create|list|get|cancel|delete` (alias `batch`) runs `/search` and `/agent/runs` requests asynchronously. Typed batch commands add the required beta token. Access depends on your team's Batch API entitlement.
- **Research (retired)** — the upstream `/research/v1` API was retired (HTTP 410); `research …` remains as a local stub that exits with `research_retired` and points at `search --type deep-reasoning`.
- **Monitors** — `monitor …`, the top-level recurring search monitors.
- **Websets** — the full tree: websets, searches, items, enrichments, exports, monitors and their runs, imports, webhooks and their delivery attempts, and events.
- **Team and admin** — `team` (bare, or `team info`) calls Exa's `/websets/v0/teams/me` endpoint for quota/concurrency; `admin keys create|list|get|update|delete|usage` against the Team Management API, gated behind a separate `EXA_SERVICE_KEY` and admin host. Whether a call succeeds still depends on your team's own access to that endpoint. To confirm a credential works, use `auth test`.
- **Escape hatch** — `raw METHOD PATH` calls any Exa endpoint, including ones not yet modeled, while keeping auth, retry, output, and error handling. For payment-annotated Search/Contents calls, raw also supports stdin-only signed payment pass-through (`--x402-payment-stdin`, `--mpp-payment-stdin`) and `--payment-discovery`; wallet custody/signing is intentionally out of scope.
- **Offline self-description** — `capabilities`, `schema`, `robot-docs`, `doctor`, `auth`, `config`, `preset`, and `macro`.

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

`agent runs stop ID --yes` completes a max-effort run early with the results it
has gathered and still incurs accrued usage. `agent runs cancel ID --yes`
terminates the run and discards those results, so both are gated the same way.
The stop command adds the required beta token automatically, and neither command
repeats a token you already supplied through `--beta` or `--header Exa-Beta:`.

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

Preset values are defaults: explicit flags, `--body`, and `--set` win. Preset bodies are validated against the vendored OpenAPI request schema, so only documented body properties are allowed; unknown keys are rejected before the preset is merged with flags, `--body`, or `--set`. `macro show` exposes the canonical expansion for the built-in `ask` and `fetch` macros.

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

The contract is what makes this usable from code. Highlights:

- **One JSON envelope per call.** Success is `exa.cli.response.v1`; errors are `exa.cli.error.v1` carrying a stable `error.code` and a category.
- **stdout is data, stderr is diagnostics.** Errors and trace output go to stderr; the parseable result goes to stdout.
- **Output format is automatic:** JSON when stdout is piped, human-readable in a TTY. Override with `--json`, `--ndjson`, `--format`, `--compact`/`--pretty`, or `--raw` to emit decoded upstream body bytes except signed payment responses replace exact submitted payment credential echoes with `<redacted>`.
- **Contents coverage is explicit.** Live `contents` and `fetch` result envelopes carry `outcome: "full"`, `"partial"`, or `"no_content"`, independent of the exit code.
- **Exit codes are stable and meaningful** — `0` ok, `1` usage (bad invocation or local body validation failure), `2` auth, `4` network, `5` upstream, `6` rate_limit, `7` not_found, `9` safety (a destructive op refused without confirmation), among others. The full table is in `capabilities`.
- **`--dry-run --print-request` works on every mutation.** It builds and prints the exact request body without sending it, but invalid bodies still exit `1` before any request is printed.
- **Destructive operations refuse to run without `--yes`** (deletes and cancels exit `9` otherwise).
- **No surprise double-billing.** `--idempotency-key` is forwarded upstream, and the CLI never auto-retries a non-idempotent create-POST.
- **Billing vs payment is explicit.** `insufficient_credits` remains exit `13`; only a challenge-evidenced raw payment 402 is `payment_required` / exit `2`.

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

Relative or empty `XDG_CONFIG_HOME` and `XDG_STATE_HOME` values are ignored in
favor of `$HOME/.config` and `$HOME/.local/state`. Explicit `EXA_AGENT_*` file
paths may remain relative. New managed files/directories use 0600/0700. Existing
paths are not silently chmod-ed: `doctor --check permissions.state` reports
accessible managed files and group/world-writable state, spill, or credential
directories, without following child-directory symlinks. A 0755 directory alone
is not a finding. Inspection is bounded to 1,024 state entries, one level deep.

Config transactions and `doctor --fix`/`--undo` share a lock; lock failure refuses
the operation. `doctor --fix --dry-run` creates no directories, locks, or backups;
`doctor --undo --dry-run` plans restoration without changing files or the marker.
Explicit `--trace FILE` appends under a sibling `.<filename>.lock`; a new trace
file is 0600, while existing trace files retain their modes.

## Authentication

Authentication is environment-first. Set the key in the environment for ordinary use:

```sh
export EXA_API_KEY=...        # primary credential for the Exa API
export EXA_SERVICE_KEY=...    # required only for `admin keys …` (Team Management)
```

Alternatively, `exa-agent auth login` reads a key from stdin and writes it to a credentials file at `~/.config/exa-agent-cli/credentials.json` (mode `0600`). That file is plaintext on disk — it is not an OS keyring — so prefer the environment variable where you can, and protect the file otherwise. `exa-agent auth status` shows which source resolved the active credential, and `exa-agent auth logout` clears the stored key.

This is true on every platform and every release artifact. The crate carries a `keyring` cargo
feature, on by default, but it is currently inert: no code is compiled conditionally on it, so
it does not enable an OS keyring. Earlier design notes under `docs/v2/`
describe a keyring-backed credential store and a keyring-free musl build (D11/D15); that split is
planned, not implemented. Treat `auth login` as plaintext-file storage until this README says
otherwise.

## Design docs

The full design set for the Rust build lives under `docs/v2/`: the locked decisions and their rationale (`decisions.md`), the agent-facing wire/output spec (`contracts.md`), the complete command tree (`commands.md`), the crate architecture (`architecture.md`), and the phased implementation plan (`implementation-plan.md`). The domain glossary is in `CONTEXT.md`. Earlier, language-agnostic v1 notes remain under `docs/` and `work/research/` for traceability.

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at your option.
