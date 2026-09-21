# Changelog

All notable changes to this project are documented here.

## Unreleased

### Added

- Typed content-freshness flags on `search`, `contents`, and `similar`:
  `--max-age-hours N` (-1..=720), `--fresh` (`maxAgeHours: 0`, always crawl),
  `--cache-only` (`maxAgeHours: -1`), and `--livecrawl-timeout MS`. The default stays
  cache-first and the request body is unchanged when none of the flags are given. A merged
  body carrying the deprecated `livecrawl` field warns, and `livecrawl` with `maxAgeHours`
  is rejected locally. The `contents` recovery hints now emit a `--fresh` that actually
  parses; every emitted `nextActions` and `suggestedCommand` is parsed through the real
  command tree in tests, including warning-derived ones.
- Exa Snapshot support: `--snapshot-as-of DATE_OR_DATETIME` on `search`, `contents`, and
  `similar` (nested under `contents` for search/similar, top-level for contents). Snapshot
  conflicts are rejected on the merged body: `maxAgeHours`, `livecrawl`, `livecrawlTimeout`,
  `subpages`, and for `search` any `category` or a `type` other than `auto`/`fast`/`instant`.
  Snapshot plan, contract, and trial error tags classify as `feature_not_enabled` on 402/403;
  `SNAPSHOT_RATE_LIMIT_EXCEEDED` classifies as a rate limit. Snapshot access remains subject
  to Exa plan entitlements.
- `websets preview --search <true|false>`, sent as the URL query parameter the corrected
  upstream spec now declares. It defaults to `true` when the body carries `search.count`,
  which preserves the previous behaviour. `capabilities` reports request placement (`in`)
  for query parameters.
- Vendored specs re-verified 2026-09-21: the public spec kept 64 operations and gained
  `snapshotAsOf`, explicit 503 `SERVICE_OVERLOADED` responses on the four core endpoints,
  and a factored stream/output schema set; the admin spec changed in prose only.
  `xtask vendor-spec` now normalizes the admin YAML in Rust (`serde_yaml_ng`, dev-tool only)
  instead of shelling out to Ruby, so the re-vendor runs on hosts without Ruby. The shipped
  binary still carries no YAML parser.
- `websets exports create|get` and `context` warn (`undocumented_upstream`) that their routes
  are no longer documented by Exa as of 2026-09-21 and are absent from the official SDKs.
  The exports route returned a route-level 404 on a live probe and may have been retired;
  `/context` still works. Both stay available pending an upstream statement.
- Batch create, list, get, cancel, and delete commands, plus early Agent run stop.
  Batch wrappers are validated locally over the merged body (`--requests`, `--body`,
  `--set`, or a preset may supply `requests`), completed-list filters survive
  pagination, and commands add their required beta tokens. Batch and stop access
  remains subject to Exa account entitlements.
- Answer model, system prompt, and country flags; the Polymarket Agent data source;
  monitor domain filters; Dynamic Highlights JSON and file input with beta and
  option-conflict checks.
- Follow-up commands for created resources and remaining pages. Continuations retain
  filters and explicit profile/beta settings without copying API keys or output paths.
- Linux static release artifacts for `x86_64-unknown-linux-musl` and
  `aarch64-unknown-linux-musl`, alongside the existing glibc builds. The shell installer
  prefers the glibc archive and falls back to musl when the host's glibc is older than the release's recorded minimum
  or absent, so containers without glibc now get a working binary. Existing glibc users
  receive the same `-gnu` archive as before.
- CI gains a Linux static lane that builds the musl target, fails unless the binary has no
  `PT_INTERP` segment and no `NEEDED` entries, and runs the result on an Alpine image; a
  guard rejecting `-C target-cpu`/`-C target-feature` in the build environment or the
  repository's cargo config; an offline `cargo xtask vendor-spec --check` gate over the
  vendored specs, overlay, and `openapi/provenance.toml`; and a check that the generated
  `release.yml` still matches `dist-workspace.toml`.

- `--max-response-bytes` and config `max_response_bytes`: a 64 MiB default ceiling
  on decoded success bodies, gzip, and entire SSE streams/JSON fallbacks. Oversize
  responses return nonretryable `response_too_large` (exit 5), preserving HTTP error
  classification and ambiguous-create recovery. Invalid/zero caps remain exit 1.
- `contents --jobs` supports 1-16 workers (default 1) for `--chunk-size` requests.
  Output stays in input order and retains every admitted result after a failure.
  One `--output` destination holds all successful chunk renderings, preserving
  modes, symlinks, and completed data on later failure.
- `permissions.state` reports accessible state files and group/world-writable
  managed directories, including the credential directory, without changing modes
  or following child-directory symlinks.

### Fixed

- The generated agent skill now explains the auto-spill envelope: when output crosses
  `--max-output-bytes`, `data` is `null` and the file at `dataPath` holds the former `data`
  object, so results live at `.results` there (or pass `--output FILE` / `--max-output-bytes 0`
  for the inline shape). Extractors that read `.data.results` from the spill file returned
  nothing.
- Healthy doctor test fixtures explicitly create private managed directories;
  writable-directory diagnostics remain report-only.
- Chunked contents rejects `stream:true` before sending or touching output, and
  explicit `--jobs` requires `--chunk-size`. Failed writes roll back partial
  records on regular files, including targets created through dangling symlinks.

- Account feature-access failures now return `feature_not_enabled`, distinct from
  a rejected credential. This follows a live Batch API entitlement refusal.
- Schema refresh decides drift on canonical JSON (`embeddedCanonicalSha256` and
  `liveCanonicalSha256`) rather than on formatting-dependent byte hashes;
  `embeddedSpecSha256` stays as provenance for the vendored file.
- `contents --text URL` and interleaved URL lists now treat `--text` as bare text
  retrieval, while retaining `--text full`, numeric caps, and explicit `--text=...`.
- Recovery and pagination actions retain safe request scope, withhold private headers
  and filters, and stop offering a cursor after the pagination loop rejects it.
  Batch creation never auto-retries without a documented deduplication guarantee.
- Auto-paginated NDJSON now honors `--output`, retains completed pages on later
  failures, and avoids collecting the entire result set in memory. Pages are staged
  in a sibling temp file, so an existing output file is never truncated before the
  first page is written; the file keeps its permissions and a symlinked path keeps
  its link. Intermediate pages no longer offer redundant continuation commands.
- Completed Agent streams now offer resource follow-ups without copying their
  output again. Follow-up generation no longer clones whole retrieval responses.
- Request previews now share header assembly with live requests, including custom
  headers, SSE Accept, idempotency keys, and the documented `Exa-Beta` header.
  Beta tokens supplied through `--beta`, `--header 'Exa-Beta: …'`, or automatic
  injection are folded into one `Exa-Beta` header with each token sent once, and
  every beta gate accepts a token supplied by either flag.
- Doctor config backups are always written `0600`, whatever the config's own mode
  was; `--undo` restores the original mode recorded alongside the backup.
- Output documentation now explains how to recover the complete stdout result after
  an output-file write failure without repeating a successful create.

- Connect timeout now reaches all three HTTP agents via `--connect-timeout` or
  config `connect_timeout`, independently of the whole-request timeout.
- Doctor fix/undo honor lock acquisition failure and refresh config under lock;
  `--fix --dry-run` performs no filesystem mutation. Temporary files are cleaned
  after failed sync/rename, and supported directory-sync errors are reported.
- Empty/relative XDG roots fall back to HOME; explicit Exa path overrides remain
  relative-capable. Explicit trace paths use locking sidecars and new-file 0600.

### Changed

- A leading `@` in `contents --highlights` now selects a file. For a literal query
  such as `@openai roadmap`, use `--highlights '{"query":"@openai roadmap"}'`.
- `agent runs cancel` now requires `--yes` for live calls, like `stop` and `delete`:
  it discards the results the run has gathered.
- Every cursor-paginated `list` rejects `--limit 0` locally as `invalid_value`
  instead of forwarding it upstream.
- `answer --user-location` is sent as the string upstream expects, and
  `answer --text` is a boolean; `capabilities` now reports both kinds.
- HTTP responses may now be gzip-encoded: `ureq` gains its `gzip` feature, which pulls in
  `flate2` on its default pure-Rust backend, so musl static linking stays free of a zlib C
  shim. `default-features` on `ureq` stays off. Raw output preserves the decoded
  response body, not compressed wire bytes; payment redaction is unchanged.
- `rustix` (unix only, `fs` feature) is now a direct dependency for the state layer's file
  handling. It was already present transitively, so the lockfile gained no new versions
  beyond the gzip chain and nothing was upgraded.
- The source package now includes `openapi/provenance.toml`, which the OpenAPI parity test
  and `vendor-spec --check` read.
- CI no longer repeats the formatting and generated-artifact checks on both operating
  systems; they run once in a dedicated `lint` job. Per-OS clippy, tests, and release
  builds are unchanged. Every job now has a timeout, and superseded pull-request runs are
  cancelled while pushes to `main` are not.
- The dev profile now builds with `debug = "line-tables-only"` and no debug info for
  dependencies. A Linux comparison reduced the binary from 82,861,952 to 33,311,480
  bytes while retaining project line tables. Single cold builds were 11.63s and
  11.80s, so no build-speed gain is claimed. Release and `dist` builds are unaffected.
- Documentation correction: the `keyring` cargo feature is inert and gates no code, so no
  release artifact stores credentials in an OS keyring. `auth login` writes a plaintext
  `0600` file on every platform. The D11/D15 keyring design in `docs/v2/` is planned, not
  implemented.

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
