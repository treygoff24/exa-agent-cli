# exa-agent

An agent-first command-line interface over the full [Exa](https://exa.ai) API.

Unofficial project; not affiliated with, endorsed by, or sponsored by Exa.

`exa-agent` exposes every documented Exa capability — search, contents, answer, code context, agent runs, research, monitors, the whole Websets tree, and team/key administration — as a single static Rust binary. It is built for AI agents as the primary user: every command is non-interactive, prints one JSON envelope, has a stable exit code, and can describe itself offline. A human can drive it too, but the defaults are tuned for a program calling it, not a person typing at a prompt.

The binary is `exa-agent`. The crate is `exa-agent-cli`. It is pre-1.0 (version `0.3.0`) and built from a committed copy of the Exa Public API spec (2.0.0) plus the Team Management spec (1.0.0).

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

After a push, CI confirms the stricter
`cargo +1.85 clippy --locked --all-features --all-targets -- -D warnings`
variant.

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

### Discovering the surface (offline, no key, no network)

The CLI describes itself, which is the point of the agent-first design. These run with no credential and no network call:

```sh
exa-agent capabilities          # machine-readable inventory of all commands + exit/error codes
exa-agent robot-docs guide      # a short, paste-ready playbook for agents
exa-agent schema --help         # embedded API/CLI schema
exa-agent doctor                # read-only health checks (add --online for a live probe)
```

`capabilities` lists all 68 commands with each one's HTTP method, path, and metadata (read-only vs. destructive, pagination style, streaming, deprecation, idempotency sensitivity), alongside the full exit-code and error-code dictionaries. Pass a command path (e.g. `exa-agent capabilities search`) to get just that command's entry instead of the full dump.

For a hard local-only boundary, set `EXA_AGENT_NO_NETWORK` to any value (including empty).
Its presence enables the guard; unsetting it is the only off state. Live typed, raw,
streaming, `auth test`, and `doctor --online` paths then return a structured
`usage_error` on stderr with exit 1 before credential resolution or transport;
`auth status` and `schema refresh --check` are also refused before credential
resolution or network access; dry-run request previews and self-description
commands still work.

### Command surface

- **Core retrieval** — `search`, `contents`, `answer`, `context`, and `similar` (deprecated upstream).
- **Agent runs** — `agent runs create|get|list|events|cancel|delete`; `create` streams.
- **Research** — `research create|get|list` (the `/research/v1` API).
- **Monitors** — `monitor …`, the top-level recurring search monitors.
- **Websets** — the full tree: websets, searches, items, enrichments, monitors and their runs, imports, webhooks and their delivery attempts, and events.
- **Team and admin** — `team` (bare, or `team info`) calls Exa's `/websets/v0/teams/me` endpoint for quota/concurrency; `admin keys create|list|get|update|delete|usage` against the Team Management API, gated behind a separate `EXA_SERVICE_KEY` and admin host. Whether a call succeeds still depends on your team's own access to that endpoint. To confirm a credential works, use `auth test`.
- **Escape hatch** — `raw METHOD PATH` calls any Exa endpoint, including ones not yet modeled, while keeping auth, retry, output, and error handling.
- **Offline self-description** — `capabilities`, `schema`, `robot-docs`, `doctor`, plus `auth` and `config`, and the `ask`/`fetch` convenience macros.

## Output contract

The contract is what makes this usable from code. Highlights:

- **One JSON envelope per call.** Success is `exa.cli.response.v1`; errors are `exa.cli.error.v1` carrying a stable `error.code` and a category.
- **stdout is data, stderr is diagnostics.** Errors and trace output go to stderr; the parseable result goes to stdout.
- **Output format is automatic:** JSON when stdout is piped, human-readable in a TTY. Override with `--json`, `--ndjson`, `--format`, `--compact`/`--pretty`, or `--raw` to pass the upstream JSON through untouched.
- **Contents coverage is explicit.** Live `contents` and `fetch` result envelopes carry `outcome: "full"`, `"partial"`, or `"no_content"`, independent of the exit code.
- **Exit codes are stable and meaningful** — `0` ok, `2` auth, `4` network, `5` upstream, `6` rate_limit, `7` not_found, `9` safety (a destructive op refused without confirmation), among others. The full table is in `capabilities`.
- **`--dry-run --print-request` works on every mutation.** It builds and prints the exact request body without sending it.
- **Destructive operations refuse to run without `--yes`** (deletes and cancels exit `9` otherwise).
- **No surprise double-billing.** `--idempotency-key` is forwarded upstream, and the CLI never auto-retries a non-idempotent create-POST.

## Authentication

Authentication is environment-first. Set the key in the environment for ordinary use:

```sh
export EXA_API_KEY=...        # primary credential for the Exa API
export EXA_SERVICE_KEY=...    # required only for `admin keys …` (Team Management)
```

Alternatively, `exa-agent auth login` reads a key from stdin and writes it to a credentials file at `~/.config/exa-agent-cli/credentials.json` (mode `0600`). That file is plaintext on disk — it is not an OS keyring — so prefer the environment variable where you can, and protect the file otherwise. `exa-agent auth status` shows which source resolved the active credential, and `exa-agent auth logout` clears the stored key.

## Design docs

The full design set for the Rust build lives under `docs/v2/`: the locked decisions and their rationale (`decisions.md`), the agent-facing wire/output spec (`contracts.md`), the complete command tree (`commands.md`), the crate architecture (`architecture.md`), and the phased implementation plan (`implementation-plan.md`). The domain glossary is in `CONTEXT.md`. Earlier, language-agnostic v1 notes remain under `docs/` and `work/research/` for traceability.

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at your option.
