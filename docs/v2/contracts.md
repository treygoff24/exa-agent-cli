# v2 Contracts (the agent-facing spec)

Date: 2026-06-29
Status: canonical. The schema ids, field names, exit codes, and rules here are the source of truth. `architecture.md`, `commands.md`, and the in-tool `robot-docs guide` must match this exactly. If you find a divergence, fix it here first, then propagate.

This is what an agent actually contracts against: how output is shaped, how failures are signalled, when to retry, how streaming/pagination/batch behave. It is deliberately separable from the command list so `robot-docs` can be generated from it.

---

## 1. stdout / stderr discipline (non-negotiable)

- **stdout is data only.** Human table/markdown, the JSON envelope, NDJSON event stream, or raw upstream bytes. Nothing else.
- **stderr is diagnostics only.** Progress, warnings, deprecations, retry notices, doctor explanations, suggested commands.
- `exa-agent <cmd> --json | jq` must work with **no** `grep -v` of log lines.
- No ANSI on stdout ever in non-TTY. Color/spinners go to stderr and auto-disable on non-TTY, `NO_COLOR`, `CI`, `TERM=dumb`, `--no-color`, or any non-human `--format`.
- Never prompt on stdin unless the command is explicitly interactive. Non-TTY confirmations require `--yes` / `--confirm`.

## 2. Output model

Default format is **auto** (D3): JSON envelope when stdout is not a TTY, human when it is.

| Control | Meaning |
|---|---|
| `--format human\|json\|ndjson` | Canonical format selector. |
| `--json` | Alias for `--format json`. |
| `--ndjson` | Alias for `--format ndjson` (one envelope per line; for streams/batches/`--all`). |
| `--raw` | Emit exact upstream bytes, **no** CLI envelope. `--raw --stream` = raw SSE. Single spelling (no `--raw-response`, no `--format raw`). |
| `--pretty` / `--compact` | Whitespace only. Default: pretty in TTY, compact when piped. |
| `--max-output-bytes N` | Hard ceiling on inline stdout payload (default conservative; see §9). Over-ceiling spills to a file and returns a handle. |
| `--correlation-id ID` | Agent-supplied id echoed verbatim into `request.correlationId` across stdout/stderr/`--trace` (§4). |
| `EXA_OUTPUT` | Sets default format when no flag is given. |

Precedence: explicit `--format`/`--json`/`--ndjson`/`--raw` > `EXA_OUTPUT` > auto(TTY-detect).

## 3. Schema versioning

All envelopes carry a `schema` string. Versions bump only on breaking change; each is pinned by a golden test (insta) so drift fails the build.

| Schema id | Used for |
|---|---|
| `exa.cli.response.v1` | Success envelope (single response, or accumulated `--all --json`). |
| `exa.cli.error.v1` | Error envelope (stderr; stdout stays empty). |
| `exa.cli.event.v1` | One streamed/paginated event under NDJSON. |
| `exa.cli.capabilities.v1` | `capabilities --json` output. |
| `exa.cli.doctor.v1` | `doctor --json` report (§15). |
| `exa.cli.pending_run.v1` | Ambiguous-create recovery record (§7). |

## 4. Success envelope — `exa.cli.response.v1`

Written to **stdout**. Field order is stable (serialized in this order).

```json
{
  "schema": "exa.cli.response.v1",
  "ok": true,
  "command": "search",
  "operation": {
    "method": "POST",
    "path": "/search",
    "operationId": "search",
    "source": "https://exa.ai/docs/exa-spec.json",
    "sourceVersion": "2.0.0"
  },
  "request": {
    "requestId": "req_local_01J...",
    "profile": "default",
    "redacted": true
  },
  "count": null,
  "data": {},
  "dataHash": null,
  "pagination": {
    "cursor": null,
    "nextCursor": null,
    "hasMore": false,
    "autoPaginated": false,
    "page": 1,
    "pageCount": 1,
    "total": null
  },
  "costDollars": { "total": 0.0 },
  "nextActions": [],
  "warnings": [],
  "diagnostics": { "durationMs": 0, "retries": 0 },
  "dataTruncated": false
}
```

`upstreamRequestId`, `correlationId`, `dataPath`, `bytes`, and `pagination` are shown above only
where populated for this example (`pagination` on a list/paginated command). On an actual call,
any of these that would otherwise be `null` are **omitted from the envelope entirely** rather
than serialized as `null` — an agent should check for key presence, not compare against `null`.
`warnings[]`/`nextActions[]` are the exception: they always serialize, even when empty.

Rules:
- `data` holds the upstream payload **unwrapped from the envelope** — e.g. `data.results` for search, `data.answer` + `data.citations` for answer. `--raw` is the only way to get upstream bytes ungrouped under `data`.
- `count` is the number of primary items in this response (results, items, citations — whichever the command's `data` is a list of), or `null` for single-object responses. It is **always populated even when `data` is spilled** (`dataTruncated: true`, §9) so an agent can size a spilled result without reading the file.
- `dataHash` is a sha256 over the serialized `data` (or `null` when spilled-without-hashing). For the offline/registry-derived surfaces it is deterministic; for live-index results it is a change-fingerprint for cheap dedup/drift detection, **not** a determinism guarantee (§12).
- `nextActions[]` carries paste-ready follow-ups (`{ "description": "...", "command": "exa-agent agent runs events <id> --stream" }`) — populated on async-create and cursor-paginated commands (e.g. after `agent run`, the obvious `runs get`/`runs events`). Empty `[]` when there is no obvious next step. This is the success-path analogue of the error envelope's `suggestedCommand`.
- `costDollars` always present; `{ "total": 0.0 }` when upstream reports none.
- `warnings[]` carries non-fatal notices (deprecated flag used, livecrawl fallback, empty-result broaden hint, etc.) — never on stdout as prose, always here.
- `request.requestId` is a locally-generated id (ULID-style, deterministic-friendly); `upstreamRequestId` is Exa's when present, omitted otherwise. `request.correlationId` echoes a caller-supplied `--correlation-id`/`EXA_CORRELATION_ID` verbatim when set; omitted otherwise, so an orchestrator running many concurrent calls can stamp its own key instead of scraping `requestId`.
- `dataTruncated`/`dataPath`/`bytes` support `--output`, `--max-output-bytes`, and auto-spill (§9). When data is inlined: `dataTruncated: false`, with `dataPath`/`bytes` omitted.
- `pagination` present only on list/paginated commands; omitted otherwise (commands.md specifies per-command). `pagination.total` is the upstream total when the cursor API supplies it, else `null` (pure cursor pagination legitimately can't know it — `count` always can).

## 5. Error envelope — `exa.cli.error.v1`

Written to **stderr**. stdout stays empty (so a failed `| jq` sees nothing, not a half-object).

```json
{
  "schema": "exa.cli.error.v1",
  "ok": false,
  "error": {
    "code": "invalid_flag_combination",
    "category": "usage",
    "message": "--all is not valid for `exa-agent search` because /search is not cursor-paginated.",
    "details": { "flag": "--all", "command": "exa-agent search", "didYouMean": null, "checked": null, "retryAfterMs": null },
    "httpStatus": null,
    "retryable": false,
    "suggestedCommand": "exa-agent search \"latest AI chips\" --num-results 100 --json",
    "see": "exa-agent search --help"
  },
  "operation": { "method": "POST", "path": "/search" },
  "request": { "requestId": "req_local_01J...", "upstreamRequestId": null, "correlationId": null, "redacted": true }
}
```

Every error MUST carry: `code` (stable machine string from the §5.1 dictionary), `category` (maps to an exit code, §6), `message` (one line, names what failed and where), `retryable` (the primary retry signal — see §7), and `suggestedCommand` (copy-pasteable, the exact thing the agent should have run). Optional, populated when relevant: `details.didYouMean` (the corrected token / candidate list for a typo'd flag, subcommand, or enum value), `details.checked` (for auth, the credential-ladder rungs that were tried — see §5.1), `details.retryAfterMs` (server-advised wait on a terminal `rate_limit`/transient failure), and `see` (a pointer into `--help`/`robot-docs`/`capabilities`). `httpStatus`/`upstreamRequestId` are populated when the failure came from upstream. **No secrets** in any field, including `suggestedCommand`.

`retryable` is the contract agents branch on. The exit code is the coarse signal; the envelope is the fine one.

**Parser errors are wrapped, never leaked.** `clap` exits **2** by default for any parse/usage error (unknown flag, bad `ValueEnum` value, missing positional, `conflicts_with`/`ArgGroup`/`range` violation) and prints its own plain text. Since §6 reserves exit 2 for `auth`, the binary catches `clap` via `try_parse` and **remaps every parse error to exit 1 (`usage`)**, rendered as `exa.cli.error.v1` on stderr — clap's native exit 2 and raw text are never surfaced (a leaked exit 2 would read to an agent as an auth failure). clap's nearest-candidate suggestion is mirrored into `error.details.didYouMean` rather than printed raw. `--help`/`--version` still print to **stdout** and exit **0** (clap's `DisplayHelp`/`DisplayVersion` are passed through unchanged). Pinned by `golden_parse_error_envelope`.

### 5.1 Error-code dictionary

`error.code` is the agent's **primary** branch signal (§6), so the vocabulary is published — enumerated here, surfaced in `capabilities --json` as `errorCodes`, and golden-pinned (§14). Each `code` has a fixed `category` (→ exit code) and a default `retryable`; an individual error may carry richer `details`. The set is stable and versioned with the `exa.cli.error.v1` schema; new codes are additive.

| `code` | `category` (exit) | default `retryable` | meaning |
|---|---|---:|---|
| `usage_error` | usage (1) | false | generic parse/usage failure (the remapped clap fallback). |
| `unknown_flag` | usage (1) | false | unrecognized flag; `details.didYouMean` set when a near match exists. |
| `unknown_subcommand` | usage (1) | false | unrecognized command; `details.didYouMean` set. |
| `invalid_value` | usage (1) | false | flag value outside its `ValueEnum`/range; `details.didYouMean` lists valid values. |
| `invalid_flag_combination` | usage (1) | false | mutually-exclusive or unsupported flag combo. |
| `missing_required_argument` | usage (1) | false | a required positional/flag was absent. |
| `placeholder_argument` | usage (1) | false | an argument looks like a placeholder (`<id>`, `$VAR`, `YOUR_*`, `…`); names the discovery step. |
| `broadcast_scope_refused` | usage (1) | false | a broad/destructive scope was refused without an explicit `--all`/opt-in. |
| `not_authenticated` | auth (2) | false | no credential resolved locally; `details.checked` lists the ladder rungs tried. |
| `reauth_required` | auth (2) | false | a credential was sent but upstream rejected it (401/403 — revoked/expired/wrong scope). |
| `key_scope_mismatch` | auth (2) | false | an api key was presented where a service key is required, or vice versa (D4). |
| `config_parse_error` | config (3) | false | config TOML failed to parse. |
| `unknown_profile` | config (3) | false | `--profile`/`EXA_PROFILE` names a profile that doesn't exist. |
| `config_invalid` | config (3) | false | a config value is malformed (e.g. bad base-url). |
| `network_error` | network (4) | true | DNS/connect/TLS/timeout before an upstream response. |
| `upstream_error` | upstream (5) | true | Exa 5xx. |
| `upstream_malformed` | upstream (5) | false | upstream returned an unparseable/contract-violating body. |
| `rate_limited` | rate_limit (6) | true | HTTP 429; `details.retryAfterMs` set when `Retry-After` is present. |
| `concurrency_limit` | rate_limit (6) | true | account concurrency cap hit. |
| `not_found` | not_found (7) | false | resource id does not exist. |
| `conflict` | conflict (8) | false | resource conflict (e.g. `externalId` exists). |
| `idempotency_conflict` | conflict (8) | false | idempotency-key reuse with a different payload. |
| `confirmation_required` | safety (9) | false | destructive op refused for missing `--yes`/`--confirm`. |
| `partial_batch` | partial (10) | false | a batch had mixed success/failure (§11). |
| `no_input` | no_input (11) | false | required stdin/input was empty or a TTY (§1). |
| `interrupted` | interrupted (12) | false | SIGINT or a stream broke after partial output (§8). |

`retryable` here is the **default**; transport may refine it (e.g. an un-keyed create failure is always `retryable: false` regardless of the underlying network class — D7/§7).

## 6. Exit-code dictionary

Exit codes are CLI categories, not raw HTTP codes (HTTP detail lives in `error.httpStatus`). Documented in `capabilities --json`. Empty result sets are success (exit 0 with `data: []`).

| Exit | category | Meaning | Example |
|---:|---|---|---|
| 0 | success | Completed; empty results are success. | `results: []` |
| 1 | usage | Invalid command/flag/JSON body/schema, or local validation. | `--all` on search; `--urls`+`--ids`; bad category filter |
| 2 | auth | Missing/invalid API key or team context (upstream 401/403). | no key; revoked key |
| 3 | config | Config/profile/env problem. | malformed TOML; unknown profile |
| 4 | network | DNS/connect/TLS/timeout *before* an upstream response. | offline; connect timeout |
| 5 | upstream | Exa 5xx or malformed upstream response. | server error |
| 6 | rate_limit | HTTP 429 or concurrency limit. | search QPS; agent concurrency |
| 7 | not_found | Resource does not exist. | run/webset/monitor id unknown |
| 8 | conflict | Resource/idempotency conflict. | webset `externalId` exists |
| 9 | safety | Dangerous op refused for missing confirmation. | delete without `--yes`; batch delete without `--confirm` |
| 10 | partial | Batch had mixed success/failure. | some content chunks failed |
| 11 | no_input | Required stdin/input was empty. | `--input -` with empty stdin |
| 12 | interrupted | SIGINT or stream broke after partial output. | Ctrl-C during `--stream` |

Keep the dictionary, but agents should branch on `error.code` + `error.retryable` first; the exit code is the coarse fallback. The exit dictionary **and** the `error.code` dictionary (§5.1) are both published in `capabilities --json` (`exitCodes` / `errorCodes`).

This is a deliberate small-integer scheme, **not** sysexits — it is the published, golden-pinned source of truth, so an agent reads it from `capabilities` rather than assuming `64`–`78`. The one collision risk it creates — `clap`'s default parse-exit `2` vs `auth` here — is closed by the parse-error remap in §5 (all parse errors → exit 1).

**Empty result is a success with a next step.** Exit 0, `data: []`, `count: 0` — and the command SHOULD emit a `warnings[]` hint turning the dead end into a move, e.g. `{"ok":true,"count":0,"data":[],"warnings":["no matches; broaden the query or raise --num-results"]}`. Never exit 1 and never empty stdout for "ran, found nothing."

## 7. Retry & idempotency (transport-layer rule — D7)

- `--retry N` (default 2) auto-retries **only**: idempotent GETs, network failures (exit-4 class), HTTP 429 (honoring `Retry-After` when present, `--retry-after` default on), and 5xx.
- **Never** auto-retry a non-idempotent create-POST unless `--idempotency-key KEY` is supplied — i.e. anything that mints a billable async run *or* a resource whose duplicate creation is harmful. The authoritative list is the registry's `idempotency_sensitive` set; a Phase-1 test asserts this prose list equals it exactly. Affected: `agent runs create`, `research create`, `websets create`, `websets searches create`, `websets enrichments create`, `websets imports create`, `websets monitors create`, `websets webhooks create`, `monitor create`, `admin keys create`.
- **The key must reach the server, or "keyed" means nothing.** When `--idempotency-key KEY` is supplied for an `idempotency_sensitive` op, transport **injects it upstream as an `Idempotency-Key: KEY` header** at the auth chokepoint (architecture §5) — that header, honored by Exa for server-side dedup, is the *only* thing that makes a keyed auto-retry non-double-billing. The local flag and the upstream header are the same value. ⚠️ Whether Exa honors a client idempotency-key header on create-POSTs is a **carry-over validation** (decisions.md): if it does not, keyed auto-retry is disabled and `--idempotency-key` becomes a no-op the recovery path still uses for the pending-run record.
- Ambiguous create failure (request sent, no confirmed response): exit non-zero, write a **pending-run record** (append-only JSONL under the state dir), and set `suggestedCommand` to the exact recovery (`exa-agent agent runs list --limit 10` for Agent runs, or re-issue with `--idempotency-key` where listing is not the right recovery).
- The pending-run record is an **agent-facing recovery contract** — agents parse it, so its shape is frozen. Schema `exa.cli.pending_run.v1`, one JSON object per line: `{ "schema": "exa.cli.pending_run.v1", "requestId": "...", "command": "agent runs create", "operationId": "createAgentRun", "apiPath": "/agent/runs", "idempotencyKey": null, "attemptedAt": "<SOURCE_DATE_EPOCH-aware epoch seconds>", "recoveryCommand": "exa-agent agent runs list --limit 10" }`. Golden-pinned (§14).
- `retryable: true` in the error envelope means "a retry of *this exact request* is safe and may succeed." It is `false` for every un-keyed create failure.

## 8. Streaming contract

`--stream` selects SSE upstream where the endpoint supports it. Output shape depends on format:

| Invocation | stdout |
|---|---|
| `--stream` + `--raw` | Exact upstream SSE bytes. |
| `--stream` + `--format ndjson` (or default when piped) | One `exa.cli.event.v1` per line, then a terminal `exa.cli.response.v1` line with accumulated `data` + final cost. |
| `--stream` + `--format json` | No live events; final accumulated `exa.cli.response.v1` only (you waited for terminal status). |
| `--stream` + `--format human` (TTY) | Rendered progressively to stdout; diagnostics to stderr. |

`exa.cli.event.v1`:
```json
{ "schema": "exa.cli.event.v1", "type": "item", "command": "agent runs events", "seq": 3, "eventId": "...", "timestamp": "2026-06-29T00:00:00Z", "correlationId": null, "event": { } }
```

`type` is a stable top-level kind discriminator (`begin` | `delta` | `item` | `progress` | `summary` | `error` | `done`) so an agent routes records without unwrapping the opaque upstream `event` blob. `timestamp` is a JSON field (never free-text prose), `SOURCE_DATE_EPOCH`-aware and scrubbed in goldens; `correlationId` echoes `--correlation-id` as in §4. The terminal `exa.cli.response.v1` line still carries the accumulated `data` + final cost.

Interrupted stream → exit 12 + `exa.cli.error.v1` on stderr including the last observed `eventId` when available. `--last-event-id ID` resumes Agent event replay.

## 9. Output target & large payloads (D10)

- `-o/--output FILE` writes the envelope (or `--raw` bytes) to `FILE` instead of stdout; stdout then carries a small confirmation envelope with `dataPath` set.
- `--max-output-bytes N` is a **default-on** ceiling on the inline stdout payload (48 KiB by default) so one accidental `contents --text` over long pages can't blow the agent's context window even without `--output`. When the serialized `data` would exceed it, the CLI spills `data` as pretty-printed JSON to a temp file under the state dir and emits the envelope with `dataTruncated: true`, `dataPath`, `bytes` set and `data` elided — and a `warnings[]` note naming the ways forward (raise `--max-output-bytes`, pass `--output FILE`, or narrow fields). `--max-output-bytes 0` disables the ceiling.
- Auto-spill (threshold-gated): the same spill mechanism, also triggered by the `--max-output-bytes` ceiling above. The standalone *auto*-spill threshold (independent of `--output`) ships conservative; the manual `--max-output-bytes` ceiling is the v1 guarantee.
- **`count` and `dataHash` survive a spill.** A spilled envelope still carries `count` (item count) and `bytes`, so an agent can size and verify the spilled file without reading it (F1.4).
- Conservative content defaults reduce how often any of this fires: `search` defaults to query-aware highlights; bare `search --text` / `similar --text` cap text at 1500 characters per result; bare `contents --text` remains uncapped for deep reads.

## 10. Pagination contract

One uniform model over endpoint-specific cursors.

- Cursor-list commands expose `--limit`, `--cursor`, `--all`, `--max-pages`, `--page-delay`.
- The envelope's `pagination` block carries `nextCursor`, `hasMore`, `autoPaginated`, `page`, `pageCount`.
- `--all --json` → one accumulated envelope. `--all --ndjson` → one envelope per page (lower memory; preferred for agents).
- `--max-pages N` caps `--all`; reaching the cap is success with `hasMore: true` and a `warnings[]` note.
- Non-cursor endpoints reject `--all` with exit 1 and a `suggestedCommand` (search → `--num-results N (1..100)`; contents → `--chunk-size`).

## 11. Batch contract

- `contents --input urls.txt --chunk-size 100 --ndjson` emits one success/error envelope per chunk.
- A mixed batch exits **10** (partial) after emitting all per-chunk NDJSON, so agents parse partial success.
- `/contents` returns HTTP 200 with per-URL failures in `statuses[]`; the envelope preserves `data.statuses[]`. Future `--fail-on-url-error` can promote those to exit 10.

## 12. Redaction & determinism

- Known secrets are never emitted, by prevention at the source rather than a value-shape scrub of output: managed auth headers are injected only at send time and refused if user-supplied (below); secret-*named* headers and query params are redacted in request previews; and the one-time secrets returned by create ops (`apiKey`, `webhookSecret`, `secret`) are redacted from the default response envelope after `--secret-output` capture. `request.redacted` is `true` on these governed paths and `false` on the ungoverned `raw` escape hatch, which emits the upstream response as-is.
- `--header 'Name: value'` may *add* request headers but MUST NOT override the managed `Authorization` / auth headers (or any known secret header). An attempt is refused with exit 1, so credentials can't be shadowed, leaked via an injected header, or prompt-injected.
- Determinism applies to the **CLI's own output** — stable field/key ordering (the envelope's own fields serialize in fixed declaration order; the upstream `data` payload preserves insertion order — "stable" means *deterministic given identical input*, not alphabetized), no wall-clock timestamps in free text (timestamps live in JSON fields), `SOURCE_DATE_EPOCH` honored for any CLI-generated time. It does **not** apply to upstream search results (Exa is a live index; identical queries may return different results — that is expected and not a determinism violation).
- **Documented volatile fields** (the only fields exempt from byte-identical determinism, normalized/scrubbed before golden snapshots and excluded from any two-invocation determinism assertion): `request.requestId`, `request.upstreamRequestId`, `request.correlationId`, `diagnostics.durationMs`, `diagnostics.retries`, `event.timestamp`, and the pending-run `attemptedAt` (§7). `embeddedSpecSha256` and `dataHash` are **not** volatile — a change in either is a real signal. With `SOURCE_DATE_EPOCH` set and these fields held aside, two consecutive structured invocations of the same command on the same input are byte-identical.

## 13. `capabilities --json` — `exa.cli.capabilities.v1`

Offline, no network. Describes the CLI contract, not account state. `describe` is a documented alias of `capabilities` (the verb an agent is likely to guess first). All maps below are **fully populated**, not placeholders — an agent self-orients entirely from this surface.

```json
{
  "schema": "exa.cli.capabilities.v1",
  "tool": "exa-agent",
  "version": "0.1.0",
  "build": {
    "commit": "abc1234",
    "buildDate": "2026-06-29",
    "target": "aarch64-apple-darwin"
  },
  "api": {
    "specUrl": "https://exa.ai/docs/exa-spec.json",
    "specTitle": "Exa Public API",
    "specVersion": "2.0.0",
    "embeddedSpecSha256": "..."
  },
  "commands": [
    {
      "path": ["search"],
      "operationId": "search",
      "method": "POST",
      "apiPath": "/search",
      "readOnly": true,
      "destructive": false,
      "idempotencySensitive": false,
      "streaming": true,
      "pagination": "none",
      "supportsRawBody": true,
      "supportsPrintRequest": true,
      "requiresConfirm": false,
      "dangerous": false
    }
  ],
  "universalFlags": [
    { "flag": "--format", "values": ["human", "json", "ndjson"], "default": "auto" },
    { "flag": "--json" }, { "flag": "--ndjson" }, { "flag": "--raw" },
    { "flag": "--max-output-bytes", "default": 49152 },
    { "flag": "--correlation-id" }, { "flag": "--output" },
    { "flag": "--idempotency-key" }, { "flag": "--retry", "default": 2 },
    { "flag": "--timeout" }, { "flag": "--yes" }, { "flag": "--confirm" },
    { "flag": "--dry-run" }, { "flag": "--print-request" },
    { "flag": "--api-key" }, { "flag": "--api-key-stdin" }, { "flag": "--profile" }
  ],
  "outputFormats": ["human", "json", "ndjson", "raw"],
  "env": ["EXA_API_KEY", "EXA_SERVICE_KEY", "EXA_PROFILE", "EXA_OUTPUT", "EXA_CORRELATION_ID", "EXA_ADMIN_BASE_URL", "NO_COLOR", "SOURCE_DATE_EPOCH"],
  "configPrecedence": ["--api-key", "EXA_API_KEY", "keyring", "config-metadata"],
  "exitCodes": {
    "0": "success", "1": "usage", "2": "auth", "3": "config", "4": "network",
    "5": "upstream", "6": "rate_limit", "7": "not_found", "8": "conflict",
    "9": "safety", "10": "partial", "11": "no_input", "12": "interrupted"
  },
  "errorCodes": {
    "not_authenticated": { "category": "auth", "exit": 2, "retryable": false },
    "rate_limited": { "category": "rate_limit", "exit": 6, "retryable": true }
  },
  "doctor": {
    "exitCodes": { "0": "healthy", "1": "findings", "4": "refused-unsafe" },
    "detectors": ["config.parse", "key.present", "service-key.scope", "base-url", "spec.hash", "binary.version", "tty.discipline", "auth.online", "connectivity"]
  },
  "schemas": {
    "response": "exa.cli.response.v1",
    "error": "exa.cli.error.v1",
    "event": "exa.cli.event.v1",
    "capabilities": "exa.cli.capabilities.v1",
    "doctor": "exa.cli.doctor.v1",
    "pendingRun": "exa.cli.pending_run.v1"
  }
}
```

`errorCodes` is shown abbreviated above; the binary emits the **full** §5.1 dictionary. Each command entry carries the blast-radius triad an agent reasons about before calling: `readOnly` / `destructive` / `idempotencySensitive` (plus `requiresConfirm`, `dangerous`), all derived from the registry + overlay (D17). `build.commit`/`buildDate`/`target` are baked at compile time so an agent (or `doctor`) can detect a stale binary.

## 14. Golden-pinned surfaces

These outputs are frozen with insta snapshots (see implementation plan); any drift fails CI:
`capabilities --json` (incl. the populated `exitCodes`/`errorCodes`/`doctor` maps), the **error-code dictionary** (§5.1), `schema list --json`, `robot-docs guide`, one success envelope (with `count`/`nextActions`/`dataHash`), one error envelope, a **parse-error envelope** (the clap-remap path, §5), a **`not_authenticated` envelope** (with `details.checked`, §5.1), one paginated list (`--all --ndjson`), one streaming NDJSON path (with `type`/`timestamp`), one `--raw` passthrough, the exit-code table, a **`doctor --json` report** (§15), key-line assertions for each command's `--help`, and the pending-run record (`exa.cli.pending_run.v1`).

## 15. `doctor --json` — `exa.cli.doctor.v1`

`doctor` is a diagnostic, so it uses a **linter-style exit dictionary**, not the §6 categories: **0 = healthy, 1 = findings present, 4 = refused-unsafe** (a detector that could not safely complete). The per-finding `category` keeps the granularity an agent wants; the exit code stays in doctor's own small vocabulary so a `doctor` exit can never be confused with a real `auth`(2)/`config`(3) failure from another command. The detector list and this exit dictionary are published in `capabilities.doctor` (§13).

```json
{
  "schema": "exa.cli.doctor.v1",
  "ok": true,
  "status": "healthy",
  "findings": [
    { "id": "key.present", "status": "ok", "category": "auth", "message": "EXA_API_KEY resolved", "suggestedCommand": null },
    { "id": "spec.hash", "status": "warn", "category": "config", "message": "embedded spec differs from committed snapshot", "suggestedCommand": "exa-agent schema refresh --check" }
  ]
}
```

`status` is `healthy` (exit 0) when no finding is `fail`, else `findings` (exit 1). Each finding mirrors the error envelope's `suggestedCommand` so the fix is one paste-ready line (every `fail`/`warn` finding MUST name one). Read-only and offline by default; `--online` adds the networked detectors (D8).
