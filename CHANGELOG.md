# Changelog

All notable changes to this project are documented here.

## 0.4.0 — 2026-07-16

### Changed

- `--text` character caps now accept only bare `--text`, `--text full`, or an
  integer from 1 through 10000. The legacy `--text 0`, `--text true`, and
  `--text false` forms from 0.2-era recipes are intentionally removed; use
  bare `--text` for the command default or `--text full` for uncapped text.
- Live `contents` and `fetch` result envelopes add a required `outcome` field:
  `full`, `partial`, or `no_content`. This is additive and independent of the
  command's exit classification.

### Fixed

- Wave 5 contract hardening: contents metadata distinguishes positional URLs,
  contents/fetch outcomes require one result row per requested item, and repo
  probes can forbid network.
- The documented `.data.results[]` jq path was verified with one budgeted live
  search; no response or credential material was retained.

## 0.3.0 — 2026-07-07

Fix pass driven by a cold-start dogfood audit: a fresh agent using only `--help`
and error messages hit a first-call failure on `context`, two entirely dead command trees
(`websets`, `team`), and error messages that destroyed the one piece of information an agent
needed to recover. All four are fixed.

### Fixed

- `context "query"` now works on the first call: `--tokens` defaults to `dynamic` instead of
  sending no token budget and failing upstream with a 400. `--tokens dynamic` and `--tokens N`
  both reach the request body correctly; `--help` now documents the range and the default.
- `websets` and `team` were calling the wrong URL prefix (`/v0/...`) and 404ing on every
  invocation with an HTML body. The runtime path is now `/websets/v0/...`, matching Exa's
  deployed Websets base. `team` (bare, no subcommand) now runs `team info` directly instead of
  requiring the one child command by name.
- Upstream error bodies are parsed instead of dumped raw. A JSON error body yields a clean
  `message` plus `details.upstream` (capped at 4096 bytes, with `details.upstreamPreview` and
  `upstreamTruncated` when it's cut); an HTML error page yields
  `"upstream returned non-JSON error page (HTTP N)"` plus `bodyPreview` instead of the literal
  `<!DOCTYPE html>` as the error message.
- The `ask` macro no longer expands to `answer QUESTION --text`. `/answer`'s `text` field is
  boolean-only (no character cap), so that flag pulled in full uncapped citation text — a
  44.8 KB response for a question `answer` alone answers in 5 KB. `ask` now expands to plain
  `answer QUESTION`.

### Changed

- `contents`/`fetch` no longer report total failure as success. When every requested URL
  fails, the command now emits an `all_urls_failed` warning and exits `10` instead of `ok: true`
  with an empty result set; partial failures emit a per-URL `url_failed` warning and still exit
  `0`.
- Search's default highlights are now capped at 800 characters per result (previously
  uncapped server-default length); `--highlights N` still overrides the cap and
  `--no-highlights` still turns highlights off entirely.
- `--highlights` and `--no-highlights` are no longer hidden from `search --help` — they were
  functional but undocumented.
- The envelope now omits always-null optional fields (`pagination`, `bytes`, `dataPath`,
  `upstreamRequestId`, `correlationId`) instead of emitting them as literal `null`; an empty
  `resolvedSearchType` is omitted rather than serialized as `""`. `warnings`/`nextActions`
  still serialize as `[]` when empty.
- `--ndjson` on list-shaped data now emits one line per result plus a final summary envelope,
  instead of a single envelope line indistinguishable from `--compact`.
- `--format human` now renders a real terse text format for `search`, `contents`, and `answer`
  (title/url/citation lines instead of indented JSON); other commands still fall back to
  pretty-printed JSON, with a one-time note on stderr when stdout is a TTY.
- Global flags now all carry help text and are grouped under a "Global options" heading in
  `--help`, separate from each subcommand's own flags.

### Added

- `capabilities <command-path>` filters the inventory to a single command's entry, instead of
  requiring the full ~9k-token dump to find one command.
- `buildDate` (in `capabilities` and `doctor`) is now a real date: `SOURCE_DATE_EPOCH` if set,
  else the git HEAD commit date, else `"unknown"` — previously always `"unknown"` on
  cargo-install builds.
- `missing_subcommand` and `unknown_subcommand` errors now carry `details.subcommands` (the
  valid children) and a `suggestedCommand`, instead of surfacing the parent command's own
  `about` string as the error message.

## 0.2.0 — 2026-07-06

Token-safe retrieval defaults: search results are now sized for agent context windows out of the box.

- Breaking-ish pre-1.0: `search` now requests query-aware highlights by default at Exa's server default length; use `--highlights N` to cap highlight characters or `--no-highlights` for metadata-only results. Bare `search --text` and `similar --text` now request `text.maxCharacters=1500`; use `--text full` or `--text 0` for uncapped text. Bare `contents --text` remains uncapped.
- Breaking-ish pre-1.0: default `--max-output-bytes` drops from 1 MiB to 48 KiB for agent context safety. Spill files are now pretty-printed JSON.

## 0.1.0 — 2026-07-06

Initial public release.

- Full Exa API surface as a single static binary: 68 commands covering core retrieval (`search`, `contents`, `answer`, `context`, `similar`), agent runs, research, monitors, the complete Websets tree (searches, items, enrichments, imports, webhooks, events), and team/key administration.
- Agent-first output contract: one JSON envelope per call (`exa.cli.response.v1` success / `exa.cli.error.v1` error), auto-JSON-when-piped / human-in-a-TTY defaults, stable exit codes (0–12), and a published `error.code` dictionary.
- Safety model: destructive operations refuse to run without `--yes`; creates never auto-retry without `--idempotency-key`; every mutation supports `--dry-run --print-request` to preview the exact upstream request without sending it.
- Offline self-description: `capabilities`, `schema`, `robot-docs`, and a read-only `doctor` (with `--online` for a live credential probe) run with no key and no network call.
- `raw METHOD PATH` escape hatch calls any Exa endpoint, including ones not yet modeled, while keeping the same auth/retry/output/error contracts.
- Environment-first authentication (`EXA_API_KEY`, `EXA_SERVICE_KEY` for admin), with an optional local credentials file as a fallback.
- 334 tests: unit, golden (insta), property, and transport-contract suites.
- Distribution: crates.io (`cargo install exa-agent-cli`), a Homebrew tap (`brew install treygoff24/tap/exa-agent`), and a checksummed shell installer via GitHub Releases.
