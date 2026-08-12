# Changelog

All notable changes to this project are documented here.

## Unreleased

## 0.6.0 — 2026-08-11

### Added

- Raw Search/Contents payment pass-through: `--payment-discovery`, `--x402-payment-stdin`, and
  `--mpp-payment-stdin` work only with exact nonstreaming `raw POST /search` or `/contents` on
  the default Exa host. Payment values are stdin-only, generic payment headers are refused, dry-run
  previews redact placeholders, and wallet/signing/custody remain out of scope.
- `agent runs create --max-cost-dollars DOLLARS` maps to `budget.maxCostDollars`; `effort max`
  is exposed behind explicit `--beta agent-max-effort-2026-07-27` and requires an explicit budget
  cap. `stopReason: budget_reached` now emits a machine-visible `budget_reached` warning.
- `search --stream` now maps to the upstream Search SSE field, is advertised by help and
  capabilities, and emits `stream_ignored` when upstream will fall back to normal JSON because
  the final request has no non-null `outputSchema`. Canonical Search SSE events expose
  `text-delta` as NDJSON `delta` records and reconstruct terminal results, output, timing, and
  cost metadata. Search stream error events and streams missing a terminal `done` event now
  fail with structured upstream errors instead of returning partial data as success; malformed
  or non-terminal `done` events are rejected as upstream contract violations.

### Changed

- Error envelopes now expose `error.details.omittedFlags` when argv-derived recovery commands
  drop unsafe or conflicting flags, and argv-derived `suggestedCommand` values are sanitized so
  recovery hints do not echo secrets or invalid combinations.
- Successful signed raw payment responses in JSON-envelope mode now add top-level `payment`
  receipt metadata. Signed payment `--raw` remains envelope-free; all non-payment raw bytes stay
  exact, while exact submitted payment credential echoes are redacted as `<redacted>`.
- HTTP 402 classification now distinguishes billing exhaustion from payment challenges:
  `NO_MORE_CREDITS` / bare 402 stays `insufficient_credits` (exit 13), while a 402 with safe
  payment challenge metadata is `payment_required` (exit 2).
- Typed `agent runs create --data-source` values are validated case-insensitively against the
  current provider enum (`fiber`, `financial_datasets`, `similarweb`, `baselayer`, `affiliate`,
  `particle`, `jinko`) and sent canonically. Legacy aliases and explicit `--body`/`--set`
  pass-through behavior are unchanged.

## 0.5.0 — 2026-08-04

Restores typed parity with the current Exa API under D16/D40: the surface now matches the live spec (2.0.0 as served 2026-08-03) and the documented docs-only endpoints. OpenAI-compatible routes remain raw-only; MPP/x402 remain unsupported (`raw` covers them; see decisions.md D40).

### Breaking

- **Research commands are now a local retirement stub.** Upstream retired `/research/v1` (HTTP 410 `RESEARCH_RETIRED`). `research create|get|list` no longer call the network; each exits 1 with error code `research_retired` and a copy-pasteable replacement — `research create "<query>"` interpolates your query into `exa-agent search "<query>" --type deep-reasoning`. The three operations left `capabilities`, and the registry count is now 67.
- **`websets imports create --csv/--url` removed.** They were advertised but returned `not_implemented`. The documented flow is create → `PUT` your file to the returned `uploadUrl`; the create envelope now carries a ready-to-paste `nextActions` curl template for that second step. A one-shot convenience returns only alongside a resumable-upload design (D40d).

### Added

- **Websets exports**: `websets exports create <webset> --format csv|json` and `websets exports get <webset> <export-id>` (docs-only endpoints, overlay-defined like `/context`; `create` is idempotency-sensitive under D7 and its envelope points at the matching `exports get`).
- **`websets get --expand items`** — a real query-string parameter; `--set expand=items` is also lifted into the query for this command instead of landing in a GET body.
- **Named flags**: `search --output-schema`, `search --system-prompt`, `agent runs create --system-prompt`, `contents --highlights [QUERY]`.
- **Category `publication`** accepted on `search`/`similar` (the canonical spelling upstream renamed from `research paper`). Legacy spellings on typed flags — `research paper`, `fiber_ai`, `particle_news` — are coerced to canonical and flagged with a structured `legacy_value_coerced` warning; `--body`/`--set` values pass through untouched.
- **`-o`/`--output FILE`** writes the full response envelope to a file; stdout receives a small confirmation envelope instead, independent of the automatic spill-on-size behavior. Same-path collisions with `--secret-output` are refused before any request is sent.
- Live `contents`/`fetch` and `answer`/`ask` envelopes carry the `outcome` field (`full`/`partial`/`no_content`) plus per-item `contentDiagnostics[]` (crawl status, error tag, HTTP status, inferred content type); answer/ask emit an empty diagnostics array because the upstream response exposes no per-citation crawl data.
- New exit code `13` (`billing`) and error code `insufficient_credits` for HTTP 402. An
  out-of-credits account previously surfaced as `invalid_value` / exit `1` — a *usage* error —
  so callers read it as "my flags were wrong" and retried with different arguments against an
  account that could not pay for any of them. The 402 path now says the account is out of
  credits, names the top-up URL, is marked non-retryable, and is skipped by the retry policy.
  Credit exhaustion is also detected from a `NO_MORE_CREDITS` body on any 4xx, since the tag has
  been observed outside 402.
- `auth test` and `doctor --online` distinguish "credential valid but account out of credits"
  from both acceptance and rejection. The Exa API publishes no balance endpoint, so this
  billing-free probe is the only credit preflight available.

### Changed

- Re-vendored both OpenAPI snapshots (drift absorbed: crawl-date fields now marked deprecated upstream, `evaluate` on webset import scoping, `scopeId`, integer `limit`/`employees` types, entity research fields, publication `abstract`/`doi`, 402 responses on search/contents).

### Fixed

- Rejected enum flag values now name the accepted set. `similar --category github` reported only
  `invalid value 'github' for '--category <CATEGORY>'`; it now lists the six valid categories.
  (`search --category` already did this.)
- `contents` and `fetch` rows whose upstream crawl failed with an empty `error: {}` now carry
  `error_reason: "upstream_reason_unavailable"` in `contentDiagnostics[]` instead of a bare
  `crawl_status: "error"` with no reason at all. The matching per-URL warning label no longer
  depends on a fallback command being constructible. This complements the 0.4.0 `outcome` field
  (`full`/`partial`/`no_content`) on the same result rows.
- `probe_inconclusive` and `invalid_field_type` were emitted but missing from the published
  `errorCodes` dictionary; both are now declared.

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
