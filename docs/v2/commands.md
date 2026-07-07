# v2 Commands & Flags

Date: 2026-06-29
Status: design reference for the `exa-agent` command tree and flag taxonomy. Carries [`work/research/lane-e-cli-taxonomy.md`](../../work/research/lane-e-cli-taxonomy.md) forward under the v2 decisions. Output shape, exit codes, envelopes, pagination, batch, and streaming are owned by [`contracts.md`](contracts.md) and are referenced, not restated. Locked product decisions live in [`decisions.md`](decisions.md) (D1–D22) and win on any conflict.

Binary/command name is `exa-agent` everywhere (D2). Crate is `exa-agent-cli`. Default output is **auto** (D3): JSON envelope when stdout is not a TTY, human when it is — so the agent-shaped examples below omit `--json` and still get parseable output.

---

## 1. Command tree

```text
exa-agent
├── search                         # POST /search
├── contents                       # POST /contents
├── similar                        # POST /findSimilar          [deprecated upstream]
├── answer                         # POST /answer
├── context                        # POST /context  (Exa Code: code/docs snippets)
├── monitor                        # top-level Search Monitors: /monitors
│   ├── create                     # POST   /monitors                       [create-POST]
│   ├── list                       # GET    /monitors
│   ├── get                        # GET    /monitors/{id}
│   ├── update                     # PATCH  /monitors/{id}
│   ├── delete                     # DELETE /monitors/{id}                  [--yes]
│   ├── trigger                    # POST   /monitors/{id}/trigger
│   ├── batch                      # POST   /monitors/batch                 [--yes for mutating ops]
│   └── runs
│       ├── list                   # GET    /monitors/{id}/runs
│       └── get                    # GET    /monitors/{id}/runs/{runId}
├── agent                          # Agent API: /agent/runs
│   ├── run                        # POST   /agent/runs    (alias of `runs create`) [create-POST]
│   └── runs
│       ├── create                 # POST   /agent/runs                     [create-POST]
│       ├── list                   # GET    /agent/runs
│       ├── get                    # GET    /agent/runs/{id}
│       ├── events                 # GET    /agent/runs/{id}/events
│       ├── cancel                 # POST   /agent/runs/{id}/cancel
│       └── delete                 # DELETE /agent/runs/{id}                [--yes]
├── research                       # Research API: /research/v1             [legacy; prefer agent]
│   ├── create                     # POST   /research/v1                    [create-POST]
│   ├── list                       # GET    /research/v1
│   └── get                        # GET    /research/v1/{researchId}
├── websets                        # Websets API: /v0/websets
│   ├── create                     # POST   /v0/websets                     [create-POST]
│   ├── list                       # GET    /v0/websets
│   ├── get                        # GET    /v0/websets/{id}
│   ├── update                     # POST   /v0/websets/{id}   (POST, not PATCH)
│   ├── delete                     # DELETE /v0/websets/{id}                [--yes]
│   ├── cancel                     # POST   /v0/websets/{id}/cancel         [--yes]
│   ├── preview                    # POST   /v0/websets/preview
│   ├── items
│   │   ├── list                   # GET    /v0/websets/{webset}/items
│   │   ├── get                    # GET    /v0/websets/{webset}/items/{id}
│   │   └── delete                 # DELETE /v0/websets/{webset}/items/{id} [--yes]
│   ├── searches
│   │   ├── create                 # POST   /v0/websets/{webset}/searches            [create-POST]
│   │   ├── get                    # GET    /v0/websets/{webset}/searches/{id}
│   │   └── cancel                 # POST   /v0/websets/{webset}/searches/{id}/cancel
│   ├── enrichments
│   │   ├── create                 # POST   /v0/websets/{webset}/enrichments         [create-POST]
│   │   ├── get                    # GET    /v0/websets/{webset}/enrichments/{id}
│   │   ├── update                 # PATCH  /v0/websets/{webset}/enrichments/{id}
│   │   ├── delete                 # DELETE /v0/websets/{webset}/enrichments/{id}    [--yes]
│   │   └── cancel                 # POST   /v0/websets/{webset}/enrichments/{id}/cancel [--yes]
│   ├── imports
│   │   ├── create                 # POST   /v0/imports   (returns uploadUrl)        [create-POST]
│   │   ├── list                   # GET    /v0/imports
│   │   ├── get                    # GET    /v0/imports/{id}
│   │   ├── update                 # PATCH  /v0/imports/{id}
│   │   └── delete                 # DELETE /v0/imports/{id}                         [--yes]
│   ├── monitors                   # Websets monitors (distinct from top-level `monitor`)
│   │   ├── create                 # POST   /v0/monitors                             [create-POST]
│   │   ├── list                   # GET    /v0/monitors
│   │   ├── get                    # GET    /v0/monitors/{id}
│   │   ├── update                 # PATCH  /v0/monitors/{id}
│   │   ├── delete                 # DELETE /v0/monitors/{id}                        [--yes]
│   │   └── runs
│   │       ├── list               # GET    /v0/monitors/{monitor}/runs
│   │       └── get                # GET    /v0/monitors/{monitor}/runs/{id}
│   ├── events
│   │   ├── list                   # GET    /v0/events
│   │   └── get                    # GET    /v0/events/{id}
│   └── webhooks
│       ├── create                 # POST   /v0/webhooks                     [create-POST]
│       ├── list                   # GET    /v0/webhooks
│       ├── get                    # GET    /v0/webhooks/{id}
│       ├── update                 # PATCH  /v0/webhooks/{id}
│       ├── delete                 # DELETE /v0/webhooks/{id}                        [--yes]
│       └── attempts list          # GET    /v0/webhooks/{id}/attempts
├── team
│   └── info                       # GET    /v0/teams/me   (quota/concurrency)
├── admin                          # GATED: EXA_SERVICE_KEY + admin host (D4)
│   └── keys
│       ├── create                 # POST   /api-keys                 [create-POST; metadata only]
│       ├── list                   # GET    /api-keys
│       ├── get                    # GET    /api-keys/{id}
│       ├── update                 # PUT    /api-keys/{id}   (PUT, not PATCH)
│       ├── delete                 # DELETE /api-keys/{id}            [--confirm <key-id>]
│       └── usage                  # GET    /api-keys/{id}/usage
├── capabilities                   # CLI self-description (offline); alias: `describe`
├── schema                         # embedded API/CLI schema export + validation (offline)
│   ├── list
│   ├── show
│   ├── export
│   ├── validate-input
│   └── refresh                    # --check reports live-spec drift; writes only with --output
├── robot-docs                     # paste-ready agent playbook (offline)
│   ├── guide
│   ├── commands
│   ├── errors
│   ├── examples
│   └── prompts
├── doctor                         # read-only diagnostics; --online for network checks (D8)
├── auth
│   ├── status
│   ├── test                       # network auth probe
│   ├── login                      # store key in OS keyring (reads stdin; never echoes)
│   └── logout                     # clear the keyring entry for the active profile
├── config
│   ├── list | get | set | unset
│   ├── path
│   └── profiles list | show | use | create | delete
├── ask                            # macro → `answer QUESTION --text` (§6)
├── fetch                          # macro → `contents URL... --text --summary-query ...` (§6)
└── raw                            # escape hatch: METHOD PATH [--body ...]
```

Macros `ask` and `fetch` are top-level (§6); `describe` is a documented alias of `capabilities`. The configurable preset/macro registry (`preset show`, `macro show`) is **deferred** post-v1 (D12).

Approximate size: ~20 top-level namespaces, ~100 leaf commands.

### Why this shape (unchanged from lane-e)

- `search` / `contents` / `similar` / `answer` / `context` are top-level — Exa's core synchronous primitives.
- `agent` is top-level because `/agent/runs` is an async workflow API (runs, events, cancel/delete, SSE, structured output).
- `monitor` (top-level `/monitors`) and `websets monitors` (`/v0/monitors`) are kept **separate** — same noun, different API families. Do not collapse. Because they differ only by singular/plural+nesting (a predictable agent mistype), the CLI ships a **custom did-you-mean**: a bare `exa-agent monitors …` (plural, nonexistent) and `monitor` invoked with webset-shaped args both emit `did you mean 'exa-agent monitor' (Search Monitors, /monitors) or 'exa-agent websets monitors' (Websets monitors, /v0/monitors)?`, and each group's `--help`/`about` names the sibling + its API path.
- `admin` is walled off (D4): separate credential, separate host, confirm-by-id deletes.
- `raw` is mandatory to avoid false completeness — any endpoint Exa ships before the registry catches up is still callable with the same auth/output/error contracts.

---

## 2. Operation-to-command mapping

> The HTTP method per operation (PATCH vs POST vs PUT) is registry-driven (D17) and exposed per-command in `capabilities --json`; a uniform `update` verb abstracts it rather than hiding it.

Every official Exa operation maps to exactly one canonical command. `[create-POST]` marks operations subject to the no-auto-retry rule (D7 / contracts §7).

| Official operation | Canonical command | Notes |
|---|---|---|
| `POST /search` | `exa-agent search QUERY` | `type`: `auto`(default)`,fast,instant,deep-lite,deep,deep-reasoning`. Content extraction nested under `contents.*`. Streaming only when `--output-schema` is set. Uses `--num-results` (1..100), **not** `--limit`. |
| `POST /contents` | `exa-agent contents URL...` | Accepts `--ids` as alternative to URLs (mutually exclusive). 1..100 urls/ids; >100 needs `--chunk-size`. Top-level `text/highlights/summary` upstream. |
| `POST /findSimilar` | `exa-agent similar URL` | **Deprecated upstream**; warns on stderr, suggests `exa-agent search --similar-to URL`. Kept for breadth. |
| `POST /answer` | `exa-agent answer QUESTION` | `--text`, `--output-schema`, `--stream`. Returns `data.answer` + `data.citations`. |
| `POST /context` | `exa-agent context QUERY` | Exa Code. **Docs-only** — not in the OpenAPI, so it is an **overlay-defined** op (D22). `--tokens dynamic|N` (50..100000). Returns `data.response` + counts. |
| `POST /monitors` | `exa-agent monitor create` | `[create-POST]`. Returns `webhookSecret` once — capture with `--secret-output FILE`. |
| `GET /monitors` | `exa-agent monitor list` | Cursor: `--limit/--cursor/--all`. Filters: status/name/metadata. |
| `GET/PATCH/DELETE /monitors/{id}` | `exa-agent monitor get/update/delete` | `delete` requires `--yes`. `update` takes `--set`/`--body`/named flags. |
| `POST /monitors/{id}/trigger` | `exa-agent monitor trigger ID` | Mutating but cheap; supports `--dry-run --print-request`. |
| `POST /monitors/batch` | `exa-agent monitor batch` | Defaults to upstream `dry_run: true`; mutating ops (delete/pause/unpause) require `--yes`. |
| `GET /monitors/{id}/runs[/runId]` | `exa-agent monitor runs list/get` | Cursor on list. |
| `POST /agent/runs` | `exa-agent agent run QUERY` or `agent runs create QUERY` | `[create-POST]`. JSON or SSE (`--stream`). `--effort`, `--output-schema`, `--input`, `--data-source`. |
| `GET /agent/runs` | `exa-agent agent runs list` | Cursor. |
| `GET /agent/runs/{id}` | `exa-agent agent runs get ID` | Status read; surface `stopReason`. |
| `GET /agent/runs/{id}/events` | `exa-agent agent runs events ID` | JSON list by default; `--stream` for SSE replay; `--last-event-id`. |
| `POST /agent/runs/{id}/cancel` | `exa-agent agent runs cancel ID` | Safe; returns terminal run if already done. |
| `DELETE /agent/runs/{id}` | `exa-agent agent runs delete ID --yes` | Destructive. |
| `GET/POST /research/v1` | `exa-agent research list/create` | `create` is `[create-POST]`. **Legacy**; warns and points to `agent`. |
| `GET /research/v1/{id}` | `exa-agent research get ID` | Status read. |
| `GET/POST /v0/websets` | `exa-agent websets list/create` | `create` is `[create-POST]`. Body-first; `--body @file` + `--set`. |
| `GET/POST/DELETE /v0/websets/{id}` | `exa-agent websets get/update/delete` | `update` is **POST**, not PATCH. `delete` requires `--yes`. |
| `POST /v0/websets/{id}/cancel` | `exa-agent websets cancel ID --yes` | Discards running work. |
| `POST /v0/websets/preview` | `exa-agent websets preview` | Plan before create; promoted in help. |
| `GET/DELETE /v0/websets/{w}/items` | `exa-agent websets items list/get/delete` | List: `--limit/--cursor/--all`, `--source-id`. `delete` `--yes`. |
| `POST/GET/cancel searches` | `exa-agent websets searches create/get/cancel` | `create` is `[create-POST]`. |
| `POST/GET/PATCH/DELETE/cancel enrichments` | `exa-agent websets enrichments create/get/update/delete/cancel` | `create` is `[create-POST]`. `delete`/`cancel` require `--yes`. |
| `POST/GET/PATCH/DELETE imports` | `exa-agent websets imports create/list/get/update/delete` | `create` is `[create-POST]`, returns `uploadUrl`; `--csv FILE` uploads only when explicit. `delete` `--yes`. |
| `POST/GET/PATCH/DELETE /v0/monitors` | `exa-agent websets monitors create/list/get/update/delete` | `create` is `[create-POST]`. Distinct from top-level `monitor`. |
| `GET /v0/monitors/{m}/runs[/id]` | `exa-agent websets monitors runs list/get` | Cursor on list. |
| `GET /v0/events[/id]` | `exa-agent websets events list/get` | List: cursor, `--type`, `--created-before/after`. |
| `POST/GET/PATCH/DELETE webhooks` | `exa-agent websets webhooks create/list/get/update/delete` | `create` is `[create-POST]` (mints a webhook + signing secret). `delete` `--yes`. `--secret-output FILE` captures signing secret. |
| `GET /v0/webhooks/{id}/attempts` | `exa-agent websets webhooks attempts list` | Cursor. |
| `GET /v0/teams/me` | `exa-agent team info` | Read-only account/limits. |
| `POST /api-keys` | `exa-agent admin keys create` | `[create-POST]`. Admin host. Body: `--name`, `--rate-limit`, `--budget-cents`. Response is **metadata only** (no raw secret). |
| `GET /api-keys` | `exa-agent admin keys list` | Admin host. |
| `GET /api-keys/{id}` | `exa-agent admin keys get ID` | Admin host. |
| `PUT /api-keys/{id}` | `exa-agent admin keys update ID` | **PUT**, not PATCH. `--name`, `--rate-limit`, `--budget-cents` (null clears budget). |
| `DELETE /api-keys/{id}` | `exa-agent admin keys delete ID --confirm ID` | Irreversible, team-wide. Confirm-by-id (D4). |
| `GET /api-keys/{id}/usage` | `exa-agent admin keys usage ID` | `--start-date`, `--end-date` (≤180d lookback), `--group-by hour|day|month`. |
| (any new/uncovered op) | `exa-agent raw METHOD PATH` | Escape hatch with full auth/output/error contracts. |

---

## 3. Universal flags

Work on every command unless explicitly irrelevant. Output flags follow D6 / contracts §2 — do not add `--raw-response`, `--raw-sse`, or `--format raw`.

### Output & format (D6, contracts §2)

| Flag | Meaning |
|---|---|
| `--format human\|json\|ndjson` | Canonical format selector. |
| `--json` | Alias for `--format json`. |
| `--ndjson` | Alias for `--format ndjson` (one envelope per line; streams/batches/`--all`). |
| `--raw` | Exact upstream bytes, **no** CLI envelope. `--raw --stream` = raw SSE. Single spelling. |
| `--pretty` / `--compact` | Whitespace only. Default: pretty in TTY, compact when piped. |
| `-o, --output FILE` | Write the envelope (or `--raw` bytes) to FILE; stdout carries a small confirmation envelope with `dataPath` (D10, contracts §9). Context-window protection. |
| `--max-output-bytes N` | Default-on ceiling on inline stdout payload (default 48 KiB); over-ceiling spills pretty-printed JSON to a file + handle (contracts §9). `0` disables. |
| `--correlation-id ID` | Agent-supplied id echoed into `request.correlationId` across stdout/stderr/`--trace` (contracts §4). Env: `EXA_CORRELATION_ID`. |

Default with no flag is **auto** (D3): JSON when piped, human in a TTY. Precedence: explicit `--format`/`--json`/`--ndjson`/`--raw` > `EXA_OUTPUT` > auto. `EXA_OUTPUT=human|json|ndjson` sets the default.

### Diagnostics (stderr only)

| Flag | Meaning |
|---|---|
| `--quiet` | Suppress non-error diagnostics on stderr. |
| `--verbose` | Request summary + retry diagnostics on stderr. |
| `--trace FILE` | Redacted request/response trace to FILE (secrets redacted, contracts §12). |
| `--no-color` | Disable ANSI; auto-honored for `NO_COLOR`/`CI`/`TERM=dumb`/non-TTY/non-human format. |

### Auth & transport

| Flag | Meaning |
|---|---|
| `--profile NAME` | Select config profile (D11/D12). |
| `--api-key KEY` | One-shot key; **never persisted** (D11). ⚠️ Leaks into `ps`/shell history/agent transcript — prefer `--api-key-stdin` in untrusted shells. |
| `--api-key-stdin` | Read the one-shot key from stdin instead of argv (no process-table leak). |
| `--base-url URL` | Override API host (default `https://api.exa.ai`). |
| `--header 'Name: value'` | Extra header; repeatable; secret headers redacted in traces. |
| `--beta VALUE` | Sets the Exa beta header where required. |
| `--timeout DURATION` / `--connect-timeout DURATION` | e.g. `30s`. |
| `--retry N` | Retry count for retryable failures (default 2). Auto-retry applies only to GETs, network (exit-4), 429, 5xx — **never** un-keyed create-POSTs (D7, contracts §7). |
| `--retry-after` | Honor `Retry-After` on 429 (default on). |
| `--idempotency-key KEY` | Client idempotency key. Required to make a create-POST auto-retryable (D7). |

### Request assembly & preview

| Flag | Meaning |
|---|---|
| `--input FILE\|-` | Read body / query list / URL list / rows from file or stdin. |
| `--input-format text\|json\|jsonl\|csv\|auto` | Interpretation of `--input`. |
| `--body JSON\|@file\|-` | Whole request body, **deep-merged over** named-flag values — fixed precedence, last-writer-wins per JSON path: `registry defaults < named flags < --body < --set` (architecture §4). Not a silent override of flags: the resolved body is always inspectable via `--print-request`. (`--body -` reads stdin, and is refused when stdin is a TTY → exit 11, never blocks.) |
| `--set path=value` | Patch one body field; repeatable; always applied **last**. e.g. `--set contents.text.maxCharacters=1000`. |
| `--print-request` | Print the redacted upstream request to stdout and exit 0 **without** calling the API (short-circuits before transport — no network, no quota). |
| `--dry-run` | **Local, no-network** preview — equivalent to `--print-request` on every command (read or mutating). It never spends a request. A genuine *server-side* preview that costs an upstream call is always a distinct, explicitly-named path (`websets preview`, or upstream `dry_run: true` on `monitor batch`), never `--dry-run`. |

### Pagination (cursor-list commands only, contracts §10)

| Flag | Meaning |
|---|---|
| `--limit N` | Page size; endpoint max enforced. |
| `--cursor TOKEN` | Resume from cursor. |
| `--all` | Follow cursors until exhausted. Rejected (exit 1) on non-cursor endpoints. |
| `--max-pages N` | Cap `--all`; hitting the cap is success with `hasMore: true` + a `warnings[]` note. |
| `--page-delay DURATION` | Delay between paginated calls. |

### Streaming (contracts §8)

| Flag | Meaning |
|---|---|
| `--stream` | Use SSE where the endpoint supports it. |
| `--last-event-id ID` | Resume Agent event replay. |

### Safety (contracts §6, exit 9)

| Flag | Meaning |
|---|---|
| `--yes` | Required for irreversible deletes/cancels in non-TTY. |
| `--confirm <id>` | Stronger confirm-by-id for high-blast-radius ops (`admin keys delete`). |

### Input forgiveness (coerce at the edges, stay deterministic)

Normalization happens in the clap `value_parser`/`ValueEnum` layer so the rest of the program sees only canonical values (design-principle "Input forgiveness"; architecture §6):

- **Enums are case-insensitive.** Every `ValueEnum` flag (`--type`, `--format`, `--effort`, `--category`, `--livecrawl`, `--input-format`, enrichments `--format`, admin `--group-by`, …) sets `ignore_case = true`, so `--type Fast`, `--format JSON`, `--effort Medium` all resolve; an invalid choice lists the valid values (clap's possible-value suggestion). `--category` is a `ValueEnum` with multi-word variants (`research paper`), not a free string, so typos get suggestions too. The canonical (lowercase/kebab) spelling is what reaches the body.
- **Content flags are forgiving.** `--text[=N|full]` accepts a character cap, `full`, or `0`; `--highlights[=N]` accepts a positive character cap.
- **Placeholders are caught, not forwarded.** A positional that looks like a literal placeholder (`<id>`, `$VAR`, `YOUR_KEY`, `…`) fails at the parse boundary with `placeholder_argument` (exit 1) naming the discovery step (`exa-agent … list`), rather than sending the literal to the API for a confusing 400/404.
- **IDs are opaque** — Exa ids carry no CLI-strippable prefix, so no prefix coercion is applied (documented so its absence isn't read as an oversight).

---

## 4. Per-command flag reference

Universal flags (§3) are assumed throughout; only command-specific flags are listed. Local validation guards run **before** the API call and exit 1 (usage) with a copy-pasteable `suggestedCommand`.

**Success-path `nextActions` (contracts §4).** Async-create and cursor-paginated commands populate the envelope's `nextActions[]` with paste-ready follow-ups carrying the returned id: e.g. `agent run`/`agent runs create` → `agent runs get <id>` + `agent runs events <id> --stream`; `websets create` → `websets get <id>` (+ `websets items list <id> --all`); `research create` → `research get <id>`; `monitor create` → `monitor get <id>`; any `--all`-capable list that stops at `--max-pages` → the `--cursor <next>` continuation. This is the success-path analogue of an error's `suggestedCommand` — the agent never has to guess the next call.

### `search` — `POST /search`

```text
exa-agent search QUERY
  --type auto|fast|instant|deep-lite|deep|deep-reasoning   # default auto
  --fast | --instant | --deep | --deep-reasoning           # shortcuts for --type
  --num-results N                                           # maps numResults; 1..100. Short alias: -n. NOT --limit
  --category 'company|people|research paper|news|personal site|financial report'
  --include-domain DOMAIN        # repeatable; includeDomains[]; supports paths + wildcards
  --exclude-domain DOMAIN        # repeatable; excludeDomains[]
  --start-published-date ISO     # alias --published-after
  --end-published-date ISO       # alias --published-before
  --start-crawl-date ISO         # alias --crawled-after
  --end-crawl-date ISO           # alias --crawled-before
  --include-text PHRASE          # includeText[]; single phrase only
  --exclude-text PHRASE          # excludeText[]; single phrase only
  --user-location CC             # 2-letter ISO country
  --moderation / --no-moderation # default off
  --compliance hipaa             # enterprise-only
  --additional-query QUERY       # repeatable; deep-* variants only
  --system-prompt TEXT|@file
  --output-schema JSON|@file     # enables synthesized output + streaming
  --similar-to URL               # future-safe replacement for `similar`
  --stream                       # SSE; valid only with --output-schema
  # ---- content extraction (nested under contents.*) ----
  --text[=N|full]                # bare search/similar: maxCharacters=1500; full or 0 uncapped
  --text-verbosity compact|standard|full
  --include-section S            --exclude-section S            # repeatable
  --include-html-tags
  --highlights[=N]               # search default: query-aware, server length; N caps chars/result
  --no-highlights                # metadata-only search results
  --highlight-query TEXT         --highlight-max-characters N   # max 10000
  --summary[=QUERY]              --summary-query TEXT  --summary-schema JSON|@file
  --extras-links N               --extras-image-links N
  --extras-rich-links N          --extras-rich-image-links N    --extras-code-blocks N
  --subpages N                   # 0..100  --subpage-target TEXT (repeatable)
  # ---- freshness ----
  --max-age-hours N              # maxAgeHours; -1..720
  --fresh                        # maxAgeHours=0 (always livecrawl)
  --cache-only                   # maxAgeHours=-1 (never livecrawl)
  --livecrawl-timeout MS         # 0<x<=90000; default 10000
  --livecrawl never|always|fallback|preferred   # DEPRECATED → use --max-age-hours/--fresh/--cache-only
  --context[=true|false]         # DEPRECATED → use --highlights/--text
  --context-max-characters N     # DEPRECATED
```

Guards:
- `--all` → exit 1: `Use --num-results N (1..100); --all is only for cursor-paginated list commands.`
- `--limit` on `search` → exit 1 did-you-mean: `search isn't cursor-paginated; use --num-results N (1..100).` (`--limit` is page size on list commands only; never silently aliased to `--num-results`, D20).
- `--count` on `search` → exit 1 did-you-mean: `search uses --num-results N (1..100); --count is the Websets result-count flag.` (Reciprocal of the websets guard below — the result-count flag is `--num-results` on `search` and `--count` on `websets` creates; neither is aliased, both redirect, D20.)
- `--livecrawl` + `--max-age-hours` → exit 1 (upstream forbids sending both).
- `--category company` with `--start/end-published-date` or `--exclude-domain` → exit 1 (unsupported; upstream may 400).
- `--category people` with `--start/end-published-date` or `--exclude-domain` → exit 1; `--include-domain` accepts LinkedIn domains only.
- More than one `--include-text` / `--exclude-text` → exit 1 (single-phrase arrays only).
- `--offset` / `--page` are not defined (search has no offset pagination).
- `--stream` without `--output-schema` → `warnings[]`: streaming has no effect; falls back to a single JSON envelope.
- Deprecated knobs (`--livecrawl`, `--context*`) used → non-fatal `warnings[]` with replacement.

### `contents` — `POST /contents`

```text
exa-agent contents URL...
exa-agent contents --input urls.txt
exa-agent contents --ids ID...                 # alternative to URLs (mutually exclusive)
  --chunk-size N                                # split >100 inputs into N-sized batches → NDJSON per chunk
  # all content-extraction + freshness flags from `search` apply, e.g.:
  --text[=N|full]  --text-verbosity compact|standard|full  # bare contents --text is uncapped
  --include-section S  --exclude-section S  --include-html-tags
  --highlights  --highlight-query TEXT  --highlight-max-characters N
  --summary[=QUERY]  --summary-query TEXT  --summary-schema JSON|@file
  --extras-links N  --extras-image-links N  --extras-rich-links N
  --extras-rich-image-links N  --extras-code-blocks N
  --subpages N  --subpage-target TEXT
  --max-age-hours N | --fresh | --cache-only   --livecrawl-timeout MS
```

Guards:
- `--urls` (positional) and `--ids` both supplied → exit 1 (choose one).
- >100 urls/ids without `--chunk-size` → exit 1 with the exact `--chunk-size 100` command.
- `--stream` → exit 1 (contents does not stream).
- `--livecrawl` + `--max-age-hours` → exit 1.
- Per-URL upstream failures arrive in `data.statuses[]` under HTTP 200; batch with mixed outcomes exits 10 (contracts §11). Codes: `CRAWL_NOT_FOUND`, `CRAWL_TIMEOUT`, `CRAWL_LIVECRAWL_TIMEOUT`, `SOURCE_NOT_AVAILABLE`, `UNSUPPORTED_URL`, `CRAWL_UNKNOWN_ERROR`.

### `similar` — `POST /findSimilar` (deprecated upstream)

```text
exa-agent similar URL
  --exclude-source-domain
  --category ...               --num-results N
  --include-domain DOMAIN      --exclude-domain DOMAIN
  --start/end-published-date   --start/end-crawl-date
  # content-extraction + freshness flags from `search` apply
```

Help banner (stderr): `Deprecated upstream: prefer 'exa-agent search --similar-to URL "..."'. Kept for full API coverage.` Emits a non-fatal `warnings[]` entry on every call.

### `answer` — `POST /answer`

```text
exa-agent answer QUESTION
  --text / --no-text           # text:true returns full citation text; default off
  --output-schema JSON|@file   # structured answer object instead of string
  --stream                     # SSE
```

Returns `data.answer`, `data.citations`, `costDollars`. `--stream --ndjson` emits `exa.cli.event.v1` lines then a terminal `exa.cli.response.v1` (contracts §8).

### `context` — `POST /context` (Exa Code)

```text
exa-agent context QUERY
  --tokens dynamic|N           # tokensNum; default dynamic; exact range 50..100000
```

Returns `data.response`, `data.resultsCount`, `data.searchTime`, `data.outputTokens`, `costDollars`. Query max 2000 chars (exit 1 if exceeded).

### `agent run` / `agent runs create` — `POST /agent/runs` [create-POST]

```text
exa-agent agent run QUERY
exa-agent agent runs create QUERY
  --output-schema JSON|@file   # JSON Schema draft-07/2019-09/2020-12
  --input JSON|@file           # request.input (rows + exclusions)
  --input-row JSON             # repeatable convenience → input.data[]
  --exclusion JSON|@file       # input.exclusion
  --previous-run-id ID         # continue a completed run (same team)
  --effort auto|minimal|low|medium|high|xhigh   # default auto; `medium` good single-entity default
  --data-source PROVIDER       # repeatable; max 5 (e.g. similarweb, fiber_ai)
  --metadata JSON
  --stream                     # SSE via Accept: text/event-stream
  --beta VALUE
```

Read/event:

```text
exa-agent agent runs list   --limit N --cursor TOKEN --all
exa-agent agent runs get    ID
exa-agent agent runs events ID --limit N --cursor TOKEN          # JSON pages
exa-agent agent runs events ID --stream --last-event-id ID       # SSE replay
exa-agent agent runs cancel ID
exa-agent agent runs delete ID --yes
```

Guards / notes:
- Un-keyed `create` is never auto-retried; an ambiguous create failure writes a pending-run record and `suggestedCommand` points at `agent runs list --since ...` (D7, contracts §7).
- `--data-source` count > 5 → exit 1.
- Surface `stopReason` in output; treat `budget_reached` as not-fully-complete (do not silently report success).

### `research` — `/research/v1` (legacy)

```text
exa-agent research create QUERY   # [create-POST]; --stream where supported
exa-agent research list           --limit N --cursor TOKEN --all
exa-agent research get RESEARCH_ID
```

Emits a non-fatal `warnings[]` entry: research v1 is legacy; new work should target `agent`. Kept for breadth.

### `monitor` — top-level Search Monitors `/monitors`

```text
exa-agent monitor create   --name NAME --query QUERY --schedule CRON
                           --webhook-url URL --secret-output FILE   # capture webhookSecret (shown once)
                           --body @file --set path=value            # [create-POST]
exa-agent monitor list     --limit N --cursor TOKEN --all  --status S  --name N
exa-agent monitor get      ID
exa-agent monitor update   ID --set path=value --body @file
exa-agent monitor delete   ID --yes
exa-agent monitor trigger  ID                          # --dry-run --print-request supported
exa-agent monitor batch    --body @ops.json            # mutating ops require --yes; default dry_run:true
exa-agent monitor runs list ID --limit N --cursor TOKEN --all
exa-agent monitor runs get  ID RUN_ID
```

Guard: `monitor create` with `--webhook-url` but no `--secret-output` and a non-TTY → `warnings[]`: the `webhookSecret` is returned once and will be lost unless captured.

### `websets` — `/v0/websets`

```text
exa-agent websets create   --query TEXT --count N
                           --body @webset.json --set path=value      # [create-POST]
exa-agent websets list     --limit N --cursor TOKEN --all
exa-agent websets get      ID
exa-agent websets update   ID --set path=value --body @file          # POST upstream, not PATCH
exa-agent websets delete   ID --yes
exa-agent websets cancel   ID --yes                                  # discards running work
exa-agent websets preview  --query TEXT --criteria TEXT --count N    # plan before create
```

Guard: `--num-results` or `--limit` on a `websets create`/`searches create`/`preview` → exit 1 did-you-mean: `Websets use --count N; --num-results is the search flag.` (Reciprocal of the `search` guard; result-count is `--count` here, `--num-results` on `search` — neither aliased, both redirect.)

Sub-resources (every create/update accepts `--body @file` + `--set`):

```text
# items
exa-agent websets items list   WEBSET --limit N --cursor TOKEN --all --source-id ID
exa-agent websets items get    WEBSET ITEM_ID
exa-agent websets items delete WEBSET ITEM_ID --yes

# searches
exa-agent websets searches create WEBSET --query TEXT --count N --criteria TEXT --scope JSON|@file  # [create-POST]
exa-agent websets searches get    WEBSET SEARCH_ID
exa-agent websets searches cancel WEBSET SEARCH_ID

# enrichments
exa-agent websets enrichments create WEBSET --description TEXT \
    --format text|number|date|boolean|options --body @json          # [create-POST]
exa-agent websets enrichments get    WEBSET ENRICHMENT_ID
exa-agent websets enrichments update WEBSET ENRICHMENT_ID --set path=value
exa-agent websets enrichments delete WEBSET ENRICHMENT_ID --yes
exa-agent websets enrichments cancel WEBSET ENRICHMENT_ID --yes

# imports
exa-agent websets imports create --source csv --url URL             # [create-POST]; returns uploadUrl
exa-agent websets imports create --csv FILE                         # convenience: create then upload (explicit only)
exa-agent websets imports list   --limit N --cursor TOKEN --all
exa-agent websets imports get    IMPORT_ID
exa-agent websets imports update IMPORT_ID --set path=value
exa-agent websets imports delete IMPORT_ID --yes

# monitors (Websets) — distinct from top-level `monitor`
exa-agent websets monitors create --body @json                      # [create-POST]
exa-agent websets monitors list   --limit N --cursor TOKEN --all
exa-agent websets monitors get    MONITOR_ID
exa-agent websets monitors update MONITOR_ID --set path=value
exa-agent websets monitors delete MONITOR_ID --yes
exa-agent websets monitors runs list MONITOR_ID --limit N --cursor TOKEN --all
exa-agent websets monitors runs get  MONITOR_ID RUN_ID

# events
exa-agent websets events list --limit N --cursor TOKEN --all \
    --type T --created-before ISO --created-after ISO
exa-agent websets events get  EVENT_ID

# webhooks
exa-agent websets webhooks create --url URL --event EVENT --secret-output FILE
exa-agent websets webhooks list   --limit N --cursor TOKEN --all
exa-agent websets webhooks get    WEBHOOK_ID
exa-agent websets webhooks update WEBHOOK_ID --set path=value
exa-agent websets webhooks delete WEBHOOK_ID --yes
exa-agent websets webhooks attempts list WEBHOOK_ID --limit N --cursor TOKEN --all
```

### `team`

```text
exa-agent team info        # GET /v0/teams/me; concurrency/quota; read-only
```

### `admin keys` — GATED (D4)

Uses `EXA_SERVICE_KEY` (never `EXA_API_KEY`) and the admin host `EXA_ADMIN_BASE_URL` (default `https://admin-api.exa.ai/team-management`). A separate keyring scope. The CLI **refuses** to use a normal API key as a service key or vice versa, with an actionable error.

```text
exa-agent admin keys create   --name NAME --rate-limit N --budget-cents N   # [create-POST]; response is metadata only
exa-agent admin keys list
exa-agent admin keys get      KEY_ID
exa-agent admin keys update   KEY_ID --name NAME --rate-limit N --budget-cents N|null   # PUT upstream
exa-agent admin keys delete   KEY_ID --confirm KEY_ID                       # irreversible, team-wide
exa-agent admin keys usage    KEY_ID --start-date ISO --end-date ISO --group-by hour|day|month
```

Guards / notes:
- `delete` without `--confirm <key-id>` (or a mismatched id) → exit 9 (safety). Confirm-by-id, not a bare `--yes`.
- A normal `EXA_API_KEY` presented where a service key is required (or vice versa) → exit 2 (auth) with the exact env-var fix.
- `create` returns key **metadata only** (`id`, `name`, `rateLimit`, `budgetCents`, `isOverBudget`, `teamId`, `userId`, `createdAt`) — no raw secret in the response per the team-management spec. If runtime shows a one-time secret, display once and require explicit `--secret-output FILE`.
- `usage` lookback ≤ 180 days; `--group-by` is currently reserved upstream (does not change response shape). `rateLimit` units are documented inconsistently (per-second vs per-minute) — surface verbatim as returned, do not normalize.

### Self-description surfaces (offline; no network unless stated)

```text
exa-agent capabilities             # exa.cli.capabilities.v1 (contracts §13); embeds spec SHA-256
exa-agent schema list              # registered schemas
exa-agent schema show NAME         # e.g. SearchRequest, exa.cli.response.v1 — from embedded spec
exa-agent schema export --api openapi --output exa-spec.yaml
exa-agent schema export --cli jsonschema --output cli-schemas.json
exa-agent schema validate-input search --body @request.json
exa-agent schema refresh --check   # compare embedded vs live spec; report drift; writes only with --output
exa-agent robot-docs guide         # paste-ready agent playbook (--format markdown|json)
exa-agent robot-docs commands      # machine-readable command list
exa-agent robot-docs errors        # error/exit-code table
exa-agent robot-docs examples --task search|answer|agent-run|websets|monitor
exa-agent robot-docs prompts       # copy-paste prompts for coding agents
exa-agent doctor                   # read-only, offline (D8); exit 0 healthy / 1 findings (contracts §15)
exa-agent doctor --online          # add the networked detectors (auth.online, connectivity)
exa-agent doctor --check config.parse,key.present,base-url,spec.hash,tty.discipline   # subset by detector id
                                   # detector ids are published in `capabilities.doctor.detectors`
exa-agent auth status              # {authenticated, source, profile, key_fingerprint, can_admin, warnings[]}
exa-agent auth test                # network auth probe
exa-agent auth login               # store key in OS keyring; reads stdin; never echoes (D11)
exa-agent auth logout              # clear the keyring entry for the active profile (best-effort)
exa-agent config list [--effective]   get PATH   set PATH VALUE   unset PATH
exa-agent config path
exa-agent config profiles list | show NAME | use NAME | create NAME | delete NAME
exa-agent raw METHOD PATH [--body @file] [--query k=v]
```

Notes:
- `capabilities`, `schema show/list`, and default `doctor` never touch the network (D8/D9). Drift detection (`schema refresh --check`) is the only schema command that reaches out, and only on demand.
- `doctor` is diagnose-and-suggest only — no `--fix`/undo/backup machinery in v1 (D8). Every `fail`/`warn` finding names its exact fix command. Output is `exa.cli.doctor.v1` (contracts §15) with the **linter-style exit dictionary 0 = healthy / 1 = findings / 4 = refused-unsafe** — deliberately *not* the §6 categories, so a `doctor` exit can't be confused with a real `auth`(2)/`config`(3) failure. The detector ids + exit dictionary are published in `capabilities.doctor`.
- `auth logout` clears the keyring entry for the active profile (best-effort; no error if absent).
- Config stores profile metadata and env-var names, never plaintext keys by default (D11).

---

## 5. Deprecations

Kept for breadth; each warns on stderr (`warnings[]`) with the recommended replacement.

| Surface / flag | Status | Replacement |
|---|---|---|
| `similar` / `POST /findSimilar` | Deprecated upstream | `search --similar-to URL` |
| `--livecrawl never\|always\|fallback\|preferred` | Deprecated | `--max-age-hours N` / `--fresh` / `--cache-only` |
| `--context` / `--context-max-characters` | Deprecated | `--highlights` / `--text` |
| `research` / `/research/v1` | Legacy | `agent run` |
| `useAutoprompt` | Removed from schema | not exposed; not a flag |
| legacy highlight count/sentence sizing | Deprecated | `--highlights N` / `--highlight-max-characters` |
| `resolvedSearchType`, response `context` | Deprecated upstream fields | ignore; not surfaced as flags |
| legacy `--neural` / `--keyword` types | Legacy | `--type auto\|fast\|...`; hidden aliases only if runtime confirms |

---

## 6. Macros (`ask`, `fetch`)

Thin, transparent expansions (D12). Inspect with `--dry-run --print-request`. No macro hides the underlying API shape. The two v1 macros (default output is auto, so no forced `--json`):

| Macro | `expands_to` | Use |
|---|---|---|
| `ask QUESTION` | `answer QUESTION --text` | One-shot cited Q&A. |
| `fetch URL...` | `contents URL... --text --summary-query "Summarize the page"` | Pull + summarize known pages. |

**Deferred post-v1 (D12):** the configurable preset/macro registry — `preset show`, presets-in-TOML (`[presets.X]`), `macro show`, and the additional macros lane-e sketched (`cite`, `investigate`, `watch`). v1 ships only `ask` and `fetch` plus `--profile` and a minimal config (base-url, default format, timeout, retry). The OpenAI-compat surfaces (`chat-completions`, `responses`) are also deferred (D16); v1 covers them via `raw` (e.g. `exa-agent raw POST /chat/completions --body @req.json`).

---

## 7. First-try examples (agent-shaped)

```bash
# Search with token-efficient highlights (auto JSON when piped)
exa-agent search "latest AI chip launches" --fast --highlights --summary-query "What changed?"

# Deep synthesized search with a schema
exa-agent search "Who is the current CEO of OpenAI?" \
  --deep --additional-query "OpenAI leadership official source" \
  --system-prompt "Prefer official sources; avoid duplicates" \
  --output-schema @leader.schema.json

# Contents for known URLs (highlights default; --output protects context)
exa-agent contents https://exa.ai/docs/reference/search --text --summary-query "CLI-relevant fields" -o page.json

# Batch a big URL list, NDJSON per chunk
cat urls.txt | exa-agent contents --input - --input-format text --chunk-size 100 --ndjson

# Cited answer
exa-agent answer "What is Exa's maximum public numResults limit?" --text

# Async agent run, then replay events as NDJSON
exa-agent agent run "Find five recently launched AI-agent eval tools" --effort medium
exa-agent agent runs events agent_run_... --stream --ndjson

# Safe create with idempotency (auto-retryable)
exa-agent websets create --query "Tech companies in SF" --count 10 --idempotency-key 7f3a-...

# Websets plan-then-create
exa-agent websets preview --query "Tech companies in San Francisco" --count 10
exa-agent websets create  --query "Tech companies in San Francisco" --count 10

# Safe destructive flow
exa-agent websets delete webset_123          # stderr: refuses without --yes; suggests `get` first
exa-agent websets delete webset_123 --yes

# Admin (gated): service key + confirm-by-id delete
EXA_SERVICE_KEY=... exa-agent admin keys list
EXA_SERVICE_KEY=... exa-agent admin keys delete key_abc --confirm key_abc

# Escape hatch for a newly added endpoint
exa-agent raw POST /new/endpoint --body @request.json
```

---

## Open seams

**Resolved by the coordinator (2026-06-29):** all five closed — see `decisions.md` Addenda (D14–D21). (1) `admin keys create` added to the create-POST no-retry list (contracts §7). (2) `--num-results` stays canonical with `-n` alias; `--limit` on search gives a did-you-mean, not an alias (D20). (3) `research` stays narrow/legacy. (4) uniform `update` is registry-driven, not hidden (D17). (5) keyring scopes locked to `exa-agent:api:<profile>` / `exa-agent:service:<profile>` (D15). Originals retained below for traceability:

For the coordinator — points where decisions.md, contracts.md, and lane-e do not fully agree and I did not silently pick:

1. **`admin keys create` and the create-POST retry list.** D7 frames the no-auto-retry rule as "anything that mints a billable async run," and contracts §7 enumerates exactly those (agent/websets/research/monitor/imports). `admin keys create` is a non-idempotent create that mints a *team-wide resource* but is **not** a billable async run and is **not** in the §7 list. I marked it `[create-POST]` here for safety and to block un-keyed auto-retry, but contracts §7's affected list should be updated explicitly if that's intended — otherwise the transport layer won't treat it as protected. Decide: add `admin keys create` (and possibly `websets searches create`, already implied) to §7, or document that admin creates are exempt.

2. **Search count flag name.** Contracts §5/§10 use `--num-results` for search; lane-b proposed `--limit` mapping to `numResults`. I used `--num-results` (contracts wins) and reserved `--limit` for cursor-list commands only. Confirm we don't also want `--limit`/`-n` as a search alias.

3. **`research` subcommand breadth.** Lane-e exposes only `create/list/get` for `/research/v1`; lane-c describes a richer research lifecycle (cancel/events/delete) but maps it onto the Agent API, not `/research/v1`. I kept the narrow legacy surface. Confirm `/research/v1` truly has no cancel/events/delete before freezing, or accept that those live only under `agent`.

4. **Update verb drift.** Three different update methods upstream: `monitor update` = PATCH, `websets update` = POST, `admin keys update` = PUT. All are documented; just flagging that the CLI's uniform `update` verb hides three HTTP methods (captured in capabilities `method`).

5. **`auth login` / keyring scope.** D11 makes keyring optional and env-first. I included `auth login` (keyring store) and a separate service-key scope, but the exact keyring identifiers (`exa:api:<profile>` vs `exa:service:<team>`) are an implementation detail not locked in decisions.md — confirm naming if it needs to be golden/stable.
