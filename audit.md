# Dogfood audit — exa-agent 0.2.0 (cold-start agent test)

**Date:** 2026-07-07
**Method:** Claude drove the CLI with *no skill doc* — discovery purely via `--help`, error
messages, and naive first-attempt calls, the way a fresh agent encounters it. Every output was
byte-measured. All findings verified against live calls (a few dry-runs where noted).

---

## P0 — broken as shipped

### 1. `context` fails on its most obvious invocation
`exa-agent context "query"` → HTTP 400: upstream requires `tokensNum` and the CLI sends none.
The first call any agent makes to this command fails.

- No default is applied. Fix: default `--tokens` to `dynamic` (or a sane number) so the bare
  call works.
- **`--tokens dynamic` is silently dropped.** `--dry-run --print-request` shows body
  `{"query":"x"}` — the value never reaches the request, then upstream 400s. Numeric values
  (`--tokens 2000`) work. So the only working syntax is undiscoverable and the documented-by-
  upstream `dynamic` mode is unreachable.
- `context --help` shows `--tokens <TOKENS>` with **no help text at all** — no range, no
  `dynamic`, no hint it's effectively required.

### 2. `websets` tree appears entirely broken — wrong URL path
`exa-agent websets list` → GET `/v0/websets` → 404 with an **HTML** body. Probing
`raw GET /websets/v0/websets` returns a structured 401 ("your team does not have access…") —
i.e. that's the real endpoint; the CLI's `/v0/...` prefix is wrong. Likely every websets
subcommand 404s. (Full confirmation blocked by this team lacking Websets access, but
HTML-404 vs structured-401 is a strong signal.)

### 3. `team info` → 404 HTML
GET `/v0/teams/me` also 404s with HTML. Same smell as websets — path or host is wrong for
the current API. The `team` command is dead weight until fixed.

### 4. HTML/upstream error bodies destroy the error message
When upstream returns HTML, `error.message` is literally `"<!DOCTYPE html>"` — zero
information. When upstream returns JSON, the message is the raw JSON **double-encoded and
truncated mid-string** (`...\"tag\":\"INVALID_RE`). An agent can't parse either. Fix: parse
upstream JSON error bodies into `message`/`details`; for HTML, substitute
"upstream returned HTML error page (status 404)".

---

## P1 — bad agent ergonomics

### 5. `ask` macro floods context (44.8 KB)
`ask` expands to `answer QUESTION --text`, and that text is uncapped citation full-text.
Plain `answer` for the same question: 5 KB, fully sufficient (answer + citations). The
*convenience* macro is 9× more expensive than the primitive — exactly backwards. Fix: cap
citation text in `ask` (or drop `--text` from the macro).

### 6. `fetch`/`contents` reports total failure as success
`fetch https://www.charter-cities.org` (livecrawl timeout, 504, zero results) returned
`ok: true`, exit 0, `count: 0`, `warnings: []`. The failure is buried in
`data.statuses[].error`. A naive agent reads "ok, 0 results" and concludes the page is empty.
Fix: when all requested URLs fail, emit a warning (and arguably a distinct exit code); when
some fail, warn per-URL.

### 7. Highlights flags exist but are invisible
Search returns query-aware highlights **by default** (good), but `search --help` never
mentions highlights. `--no-highlights` and `--highlights <chars>` exist and work — discovered
only by guessing. An agent that finds the default too heavy has no discoverable way to slim
it. Fix: unhide both flags and add a line to the `--text` help explaining the
highlights-vs-text tradeoff.

### 8. Bare parent commands error with their own description string
`exa-agent team` / `robot-docs` / (any parent) →
`{"code":"invalid_value","message":"Team quota and concurrency (GET /v0/teams/me)"}`.
The *about string* as the error message, with a misleading code (`invalid_value`, should be
`missing_subcommand`), and no list of valid subcommands. Compare the excellent bad-category
error (#P — see positives) which lists all valid values. Fix: emit
`missing_subcommand` + `details.subcommands: [...]` + `suggestedCommand`.

### 9. `team` requires `team info` — a parent with exactly one child
Pure ceremony. Either make bare `team` run `info`, or at minimum fix #8 so the error says so.

### 10. Unknown-subcommand errors don't suggest alternatives
`exa-agent agent list` → `unrecognized subcommand 'list'` — no mention that `runs list`
exists. Clap has suggestions for flags; subcommand errors should carry
`details.subcommands` too.

### 11. Default search output is still heavy: ~24.6 KB for 10 results
~6k tokens per default search. Highlights average ~1.5–2 KB/result and lead with nav junk
("Thursday, 25 June … Almaty 67 °F"). Options worth considering:
- default `-n` lower than 10, or a smaller default highlight cap (the suggested
  `--highlights 800` in the CLI's own error hint is a better default than current);
- Oddly, `-n 3 --text` (capped 1500) was *half* the size of `-n 3` with default highlights
  (7 KB vs 14.6 KB). When capped text beats highlights on size, the highlight cap is too high.

### 12. `--format human` is not human — it's pretty JSON
Same envelope, just indented. For an agent-first tool maybe human format is out of scope,
but then the flag shouldn't advertise a mode that doesn't exist. Either render an actual
terse text format (title + url per line would be genuinely useful for agents too) or drop
the value.

### 13. `--ndjson` on search returns one envelope line, not per-result lines
Indistinguishable from `--compact`. If NDJSON only applies to list endpoints, say so in help;
if it's meant to stream results, search should emit one result per line.

### 14. `capabilities` is all-or-nothing at 35.6 KB
~9k tokens to discover anything. There's no `capabilities <command>` filter
(`unexpected argument 'search'`). robot-docs partially covers this but a per-command
capability slice (`capabilities --command search`) would let agents pay only for what they
need. `schema show <name>` exists but requires already knowing the name.

---

## P2 — polish

15. **`resolvedSearchType` is always `""`** in search responses. Dead field or upstream
    regression; either populate or drop it.
16. **Envelope nulls**: `pagination: null, bytes: null, dataPath: null, nextActions: [],
    upstreamRequestId: null` on every response — ~200 bytes of always-null noise per call.
    Consider omitting null/empty envelope fields in compact mode.
17. **`--tokens` and many flags have no help text** (blank descriptions in `--help` for
    `--format`, `--json`, `--output`, `--set`, `--body`, etc.). The global-flag wall (~30
    global options repeated in every subcommand help) buries the 5 flags that matter for the
    subcommand. Consider `help_heading` grouping or hiding globals behind
    `--help-global`.
18. **No update signal.** This machine ran 0.1.0 a day after 0.2.0 shipped; nothing in the
    CLI (or `doctor`) hints a newer release exists. `doctor --online` could compare against
    the latest GitHub release tag. (This is how the whole audit almost tested the wrong
    binary.)
19. `capabilities` reports `"buildDate": "unknown"` (also in `doctor`) — build stamp not
    wired for cargo-install builds.
20. Spill filenames (`97106-req_local_...-0.json`) are opaque; fine functionally, but a
    query slug prefix would make multi-spill sessions navigable.

---

## What's genuinely good (keep and defend)

- **Structured error envelope** with stable `code`, `category`, `exitCode`, `retryable`,
  and especially `suggestedCommand` — the `--highlights false` error ("must be a character
  cap from 1 to 10000… try `--highlights 800`") is best-in-class agent ergonomics.
- **Bad-enum errors list every valid value** (`--category startup` → full category list).
- **Spill UX works exactly as designed**: clear warning with byte counts, spill path, and
  three concrete remedies; spill file present and pretty-printed.
- **Deprecation surfaced in-band**: `similar` returns a `deprecated_upstream` warning with a
  concrete replacement command. Perfect pattern — reuse it elsewhere.
- **`doctor` is compact (1.2 KB), accurate, and honest about skipped online checks.**
- **Cost surfacing** (`costDollars`) on every paid call.
- **`--dry-run --print-request`** works and made half these diagnoses possible.
- **`auth status`** is compact, redacted (fingerprint + last4), and unambiguous.
- `answer` output (5 KB with citations) is well-calibrated. `contents` default (~7 KB/page,
  clean text) is right.
- `robot-docs guide` is small (630 B) and its advice is correct — including flags
  (`--no-highlights`, `--text 1500`) that `--help` itself hides (see #7).

---

## Output-size reference (this session)

| Call | Bytes | Verdict |
|---|---|---|
| `search Q` (default, 10 results) | 24,637 | heavy (#11) |
| `search Q -n 3` (highlights) | 14,620 | heavy per-result |
| `search Q -n 3 --text` | 7,053 | good |
| `contents URL` | 6,802 | good |
| `answer Q` | 5,023 | good |
| `ask Q` | 44,797 | flood (#5) |
| `context Q --tokens 2000` | 9,973 | good |
| `similar URL -n 3` | 1,413 | good |
| `capabilities` | 35,583 | heavy (#14) |
| `robot-docs guide` | 630 | good |
| `doctor` | 1,253 | good |

## Suggested fix order

1. #1 context default + `dynamic` parsing (first-call failure on a headline command)
2. #2/#3 websets + team URL paths (whole subtrees dead)
3. #4 upstream error-body parsing (multiplies the pain of every other failure)
4. #6 fetch/contents all-failed = silent success (agent draws wrong conclusions)
5. #5 ask macro cap, #7 unhide highlights flags, #8/#10 parent/subcommand errors
6. The rest.
