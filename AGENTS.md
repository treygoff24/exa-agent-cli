# exa-agent — agent guide

You are probably an AI agent setting this up for a human, or using it yourself. This file is the complete contract. The README is for humans; everything you need is here.

Unofficial project; not affiliated with, endorsed by, or sponsored by Exa.

## What this tool does

`exa-agent` is a single self-contained binary that exposes the full Exa API — search, contents, answer, code context, agent runs, monitors, the whole Websets tree (including exports), and team/key administration — as 73 non-interactive commands. Every call returns a stable exit code, and every structured (non-`raw`) success prints exactly one JSON envelope — `--ndjson` emits one envelope per line by design, `raw` prints decoded upstream body bytes except signed payment replaces exact submitted payment credential echoes with `<redacted>`, and streaming and human-format output differ by design. It can describe its own surface offline, with no key and no network call.

## Install

```sh
brew install treygoff24/tap/exa-agent
# or
cargo install exa-agent-cli
# or
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/treygoff24/exa-agent-cli/releases/latest/download/exa-agent-cli-installer.sh | sh
```

Verify: `exa-agent --version`.

The shell installer picks the right archive for the host. On Linux it prefers the glibc build and
falls back to a fully static musl build when glibc is older than the release's recorded minimum or absent, so it works inside
Alpine, distroless, and scratch images. Supported targets: `aarch64-apple-darwin`,
`x86_64-apple-darwin`, `aarch64-unknown-linux-gnu`, `aarch64-unknown-linux-musl`,
`x86_64-unknown-linux-gnu`, `x86_64-unknown-linux-musl`. Windows is a documented non-goal.

Credentials never live in an OS keyring on any artifact. The `keyring` cargo feature is inert — it
gates no code — so `auth login` writes a plaintext `0600` file on every platform. Prefer
`EXA_API_KEY`.

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

Raw output preserves HTTP content-decoded body bytes, including gzip decoding;
it is not a wire-level compressed-byte capture. Signed-payment echo redaction
still applies.

Success envelope (`exa.cli.response.v1`, stdout): `data` carries the command's result, shaped per-command; async-create and paginated commands also carry `nextActions` (paste-ready follow-up commands), `count`, and `dataHash`. Live `contents`/`fetch` and `answer`/`ask` result envelopes carry text-aware `outcome` (`full`, `partial`, or `no_content`) independently of exit classification. They also carry `contentDiagnostics[]`: contents entries expose exact upstream `crawl_status`, `error_tag`, and `http_status` when present plus honestly inferred `content_type`, `content_status`, `usable`, and `pdf_unextracted`; answer currently emits `[]` because Exa provides no per-citation diagnostics. Empty/binary/PDF/crawl failures always add a warning and fallback action. `request.correlationId` echoes `--correlation-id`/`EXA_CORRELATION_ID` if you set one.

Error envelope (`exa.cli.error.v1`, stderr): `error.code` (from the published dictionary below), `error.message`, and often `suggestedCommand`. Stdout normally stays empty on error. If `--output` fails after a successful operation, the complete result stays on stdout with an `output_write_failed` warning and a nonzero exit: save that result rather than repeating a potentially billable create. Interrupted NDJSON streams can also leave partial events on stdout.

Output format is automatic — JSON when stdout is piped, human-readable in a TTY. Always pass `--json` (alias for `--format json`) when you are the consumer, so behavior doesn't depend on how you were invoked. `--raw` emits decoded upstream body bytes with no CLI envelope except signed-payment output replaces exact submitted payment credential echoes with `<redacted>`. `-o/--output FILE` writes the complete selected output to `FILE` (same signed-payment redaction rule for `--raw`); stdout carries only a small confirmation envelope with `dataPath`, and an explicit output path supersedes state-dir auto-spill.

## Exit codes

| Code | Name | Meaning |
| ---: | --- | --- |
| 0 | ok | success |
| 1 | usage | bad invocation, parse error, or local validation failure (missing required body field, unknown field, out-of-range value, malformed `--body`/`--set`) |
| 2 | auth | missing, invalid, or wrong-scope credential |
| 3 | config | malformed config or unknown profile |
| 4 | network | connection/timeout failure reaching Exa |
| 5 | upstream | HTTP failure or an unusable upstream response |
| 6 | rate_limit | 429; budget or concurrency exhausted |
| 7 | not_found | resource does not exist |
| 8 | conflict | duplicate/externalId conflict |
| 9 | safety | destructive op refused without confirmation (pass `--yes`) |
| 10 | partial | batch partially succeeded (per-item statuses) |
| 11 | no_input | required stdin/@file input absent, or a TTY would block |
| 12 | interrupted | SIGINT / stream interrupted |
| 13 | billing | 402; the Exa account is out of credits (key is valid, command was fine) |

`error.code` is the finer-grained signal: 36 codes map onto these 14 exit categories. For exit `2`, `not_authenticated` means set a key, `reauth_required` means Exa rejected the credential, and `feature_not_enabled` means request access from Exa rather than rotating the key. The full dictionary is in `capabilities --json`; trust its generated values if this file disagrees.

**Out of credits is exit `13` / `insufficient_credits`, never exit `1`.** Challenge-evidenced raw payment 402 is checked first and is `payment_required` / exit `2`; otherwise a bare 402, or any 4xx body carrying `NO_MORE_CREDITS`, means the credential is valid and the invocation was well-formed — the account just cannot pay. Retrying and re-guessing flags is wasted effort; top up at https://dashboard.exa.ai or move the task to another research lane.

Dispatch-level body validation runs before credential resolution and network I/O. Body-level mistakes (unknown fields, out-of-range values, missing required fields, or a malformed `--body`/`--set`) exit `1` as a local `usage` error rather than being sent upstream and returning `5`. `--dry-run --print-request` still performs this validation and exits `1` without printing a request when the body is invalid; when the body is valid it prints the exact request body and exits `0` without sending it.

## Safety model

- Destructive operations (deletes, cancels) refuse to run without `--yes` and exit `9` otherwise.
- Create-POSTs never auto-retry without `--idempotency-key` — retrying a create on a post-send timeout can double-bill. An ambiguous create failure writes a local pending-run record and the error names the exact recovery command.
- Batch creation never auto-retries, even with a key: the beta API has no documented deduplication guarantee. Ambiguous batch failures retain recovery information with or without a key.
- `--dry-run --print-request` works on every mutation: it builds and prints the exact request body without sending it.
- Preview headers show explicit and feature-specific values, not credentials or HTTP-stack defaults. Custom caller headers are not copied into automatic follow-ups; `followup_context_required` means preserve the original context manually.
- `--header` cannot override managed auth or payment headers (`Authorization`, payment namespaces, or other secret headers) — refused at exit `1`.
- Raw payment modes are pass-through only: `--payment-discovery`, `--x402-payment-stdin`, and `--mpp-payment-stdin` are limited to exact nonstreaming `raw POST /search` or `/contents` on the default host; payment values are stdin-only and never combined with API/service credentials. Successful signed raw payment responses redact exact submitted payment credential echoes before any output. JSON-envelope mode adds top-level `payment: { kind: "receipt", headers: [...] }` receipt metadata after `dataTruncated`; under `--raw`, no envelope or `payment` metadata is added and output preserves decoded upstream body bytes except those echoes are replaced with `<redacted>`.

## Receive, chunk, and state limits

- `--max-response-bytes N` / config `max_response_bytes`: 64 MiB default, positive
  decoded-byte cap, including gzip and the entire SSE stream/JSON fallback. Bad
  values fail before sending (exit 1); an exceeded cap is `response_too_large`,
  nonretryable Upstream/exit 5. Preserve partial output and use create recovery.
  This is separate from the inline `--max-output-bytes` spill threshold.
- `--connect-timeout DURATION` / config `connect_timeout` bounds connection setup
  without changing the whole-request `--timeout`; unset means no extra connect cap.
- `contents --chunk-size N --jobs J`: 1-16 workers, default 1, input-order output.
  Explicit `--jobs` requires `--chunk-size`. A failed round admits no further work, but drains every already-started result.
  `--output` holds the complete sequence of successful chunk renderings, with one
  final confirmation. JSON is a sequence of envelopes; NDJSON retains records and
  summaries. Completed data survives later failure, with stdout fallback for failed
  writes and a staging path if rename fails. File modes and symlinks are preserved.
- Empty/relative XDG config/state roots are ignored; explicit `EXA_AGENT_*` paths
  retain relative-path support. `permissions.state` reports accessible managed files
  and writable state/spill/credential directories; 0755 alone is not a finding.
  It checks at most 1,024 state entries, one level deep, without following child
  directory symlinks. Existing paths are report-only, not silently chmod-ed.
- Doctor fix/undo refuse config-lock failure; fix dry-run has no filesystem writes.
  Explicit trace paths use a sibling `.<filename>.lock`; new traces are 0600.

## Machine self-description

These run with no credential and no network call:

```sh
exa-agent capabilities --json    # all 73 commands: method, path, read-only/destructive/idempotency-sensitive, full exit-code + error-code dictionaries, embedded spec hash
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

Release process and CI are driven by cargo-dist (`dist-workspace.toml`); the design record lives under `docs/v2/`, starting with `docs/v2/decisions.md`.

`.github/workflows/release.yml` is **generated** — never hand-edit it. Change `dist-workspace.toml`
or `.github/dist-build-setup.yml`, then run `dist generate` with the pinned dist version. CI's
`release-config` job runs `dist generate --check` and fails the pull request if the two drift. The
target list lives only in `dist-workspace.toml`; the workflow computes its build matrix at plan
time, so adding a target does not change `release.yml` at all.

CI jobs: `lint` (formatting, generated skill/registry, offline `vendor-spec --check`) runs once on
Linux because those checks are OS-independent; `test` keeps clippy, the suite, and a release build
on both Linux and macOS; `msrv` pins 1.85; `static-linux` builds the musl target and proves the
binary is static and runs on Alpine; `release-config` checks the generated workflow.

Note for anyone implementing D15: dist has no per-target feature selection, so the musl artifacts
are built with the default feature set like every other target. A real keyring must be gated on
`cfg(target_env = "musl")` (or equivalent), not on the `musl-set` cargo feature, or the shipped
binary will not match the design. Local working docs (audits, reviews, plans, journals, research) belong in `work/`, which is gitignored — keep them out of the repo; `work/generated/` is the tracked exception pinned by tests.

## Issue tracking — beads (house rules)

This project uses **bd (beads)** as the work ledger. `bd prime` for commands; `bd ready` on arrival.

- **Beads is the work graph only** — tasks, bugs, dependencies, close-reasons. **Journal, state/sitrep, and memory files are the narrative and continuity layer and we use them heavily.** Beads never replaces them; a close-reason should point at the journal entry or commit that holds the story.
- Model decisions-needed-from-Trey as blocker beads (human-checkpoint-as-blocker-edge), so dependent work can't be picked up by mistake.
- Create the bead before starting substantial work; close with `--reason`.
- `bd remember` is welcome *alongside* memory files, not instead of them.
- Git behavior comes from this room's own rules (commits ungated, pushes gated — global CLAUDE.md), never from beads tooling.

Do not let `bd` tooling re-inject its managed CLAUDE.md/AGENTS.md block; this section replaces it deliberately.

<!-- BEGIN BEADS INTEGRATION v:1 profile:minimal hash:46cd31e7 -->
## Beads Issue Tracker

This project uses **bd (beads)** for issue tracking. Run `bd prime` to see full workflow context and commands.

### Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --claim  # Claim work
bd close <id>         # Complete work
```

### Rules

- Use `bd` for ALL task tracking — do NOT use TodoWrite, TaskCreate, or markdown TODO lists
- Run `bd prime` for detailed command reference and session close protocol
- Use `bd remember` for persistent knowledge — do NOT use MEMORY.md files

**Architecture in one line:** issues live in a local Dolt DB; sync uses `refs/dolt/data` on your git remote; `.beads/issues.jsonl` is a passive export. See https://github.com/gastownhall/beads/blob/main/docs/core-concepts/sync-concepts.md for details and anti-patterns.

## Agent Context Profiles

The managed Beads block is task-tracking guidance, not permission to override repository, user, or orchestrator instructions.

- **Conservative (default)**: Use `bd` for task tracking. Do not run git commits, git pushes, or Dolt remote sync unless explicitly asked. At handoff, report changed files, validation, and suggested next commands.
- **Minimal**: Keep tool instruction files as pointers to `bd prime`; use the same conservative git policy unless active instructions say otherwise.
- **Team-maintainer**: Only when the repository explicitly opts in, agents may close beads, run quality gates, commit, and push as part of session close. A current "do not commit" or "do not push" instruction still wins.

## Session Completion

This protocol applies when ending a Beads implementation workflow. It is subordinate to explicit user, repository, and orchestrator instructions.

1. **File issues for remaining work** - Create beads for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **Handle git/sync by active profile**:
   ```bash
   # Conservative/minimal/default: report status and proposed commands; wait for approval.
   git status

   # Team-maintainer opt-in only, unless current instructions forbid it:
   git pull --rebase
   bd dolt push
   git push
   git status
   ```
5. **Hand off** - Summarize changes, validation, issue status, and any blocked sync/commit/push step

**Critical rules:**
- Explicit user or orchestrator instructions override this Beads block.
- Do not commit or push without clear authority from the active profile or the current user request.
- If a required sync or push is blocked, stop and report the exact command and error.
<!-- END BEADS INTEGRATION -->

<!-- BEGIN BEADS CODEX SETUP: generated by bd setup codex -->
## Beads Issue Tracker

Use Beads (`bd`) for durable task tracking in repositories that include it. Use the `beads` skill at `.agents/skills/beads/SKILL.md` (project install) or `~/.agents/skills/beads/SKILL.md` (global install) for Beads workflow guidance, then use the `bd` CLI for issue operations.

### Quick Reference

```bash
bd ready                # Find available work
bd show <id>            # View issue details
bd update <id> --claim  # Claim work
bd close <id>           # Complete work
bd prime                # Refresh Beads context
```

### Rules

- Use `bd` for all task tracking; do not create markdown TODO lists.
- Run `bd prime` when Beads context is missing or stale. Codex 0.129.0+ can load Beads context automatically through native hooks; use `/hooks` to inspect or toggle them.
- Keep persistent project memory in Beads via `bd remember`; do not create ad hoc memory files.

**Architecture in one line:** issues live in a local Dolt DB; sync uses `refs/dolt/data` on your git remote; `.beads/issues.jsonl` is a passive export. See https://github.com/gastownhall/beads/blob/main/docs/core-concepts/sync-concepts.md for details and anti-patterns.
<!-- END BEADS CODEX SETUP -->
