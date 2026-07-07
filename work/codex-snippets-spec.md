# Task: token-efficient content defaults for `exa-agent search`

You are working in the `exa-agent` Rust CLI repo (agent-first CLI over the Exa API). Read `AGENTS.md` and `CONTEXT.md` first. The OpenAPI spec the CLI is generated against is in `openapi/` (`exa-spec.json`) — check it for the exact shape of `contents.highlights` and `contents.text.maxCharacters` before wiring anything.

## Motivation (real incident)

An agent ran `exa-agent search "<query>" -n 5 --text` and got ~35KB of full page text inline — five entire GitHub issue threads — to answer a question whose answer was one sentence. The CLI's only content options for search are "nothing" or "entire page." Search is triage; it should return query-aware snippets by default.

## Changes (all in this task — they share the search dispatch path)

### 1. Highlights become the search default

- Default `search` behavior (no content flags given): request `contents.highlights` from the API so every result carries query-aware snippet(s). Use the current highlights options object (`query`, optional `maxCharacters`) and document server-default length in the flag help.
- `--highlights[=N]` — explicit control; N = per-result highlight character budget. Optional-value flag (see the `num_args = 0..=1` pattern already used for `--num-results` in `src/cli/mod.rs`).
- `--no-highlights` — metadata-only results (the old default behavior).

### 2. `--text` gets a cap and stops being the only option

- `--text` changes from `bool` to an optional-value flag: `--text` → server-side `text.maxCharacters` of **1500** per result; `--text N` → explicit cap; `--text full` (or `--text 0`) → uncapped, current behavior.
- Passing `--text` disables the default highlights unless `--highlights` is also explicitly given (both together is allowed and sends both).
- Apply the same `--text[=N|full]` treatment to `contents` and `similar` commands, BUT `contents` keeps **uncapped** as its default when `--text` is bare — contents is the deep-read command; the cap default only applies to `search` and `similar`. Keep the flag syntax identical across commands; only the bare-flag default differs. Document the asymmetry in the flag help text.
- If the API response gives any way to detect that text was truncated by maxCharacters, surface it per-result; if not (likely), when a cap was applied add a top-level `diagnostics`/warning note in the envelope: text capped at N chars per result, use `exa-agent contents <url> --text full` for complete text. Follow the existing warning/diag idioms in `src/lib.rs` (see the hidden `--limit`/`--count` correction flags for the house style of teaching the caller).

### 3. Recalibrate `--max-output-bytes`

- Default drops from `1_048_576` to `49_152` (48 KiB) — the consumer is an agent context window. Flag stays overridable; `0` still disables.
- Spill files (see `apply_output_ceiling` / `spill_data_to_file` in `src/lib.rs`) are currently written as compact JSON; write them **pretty-printed** instead so line-based tools (head/grep) work on spilled data. Update the spill warning text if it mentions format.

### 4. Oversized-response self-correction diag

- When a `search` response's serialized `data` exceeds ~10 KiB, append a warning diagnostic (existing warning mechanism) suggesting `--highlights` (default) or `--text 1500` instead of `--text full`. Do not add this to `contents` (deep reads are expected to be big).

### 5. Docs inside the repo

- Update the robot-docs / self-description surfaces (`schema show search`, capabilities, any `docs/` pages describing search/contents flags) so the CLI's self-description matches the new contract. Update `CHANGELOG.md` under an Unreleased heading — note the two behavior changes (highlights-by-default, 48KiB spill default) as breaking-ish pre-1.0 changes.

## Non-goals

- No `--budget-tokens` or token-denominated anything.
- No NDJSON/streaming changes.
- No changes outside this repo.

## Verification (required before you finish)

- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` all pass.
- Add/extend tests following the existing test style in `tests/` covering: default highlights request body, `--no-highlights`, `--text` bare → maxCharacters 1500 on search, `--text full` → no cap, contents bare `--text` → no cap, spill pretty-printing, and the oversized-search warning.
- Show the final request bodies (e.g. via existing dry-run/spec tests) for: `search q`, `search q --text`, `search q --text full --highlights`, `contents url --text`.

## Report format

Summarize: files touched, the exact default values chosen for highlights options, any API-spec surprises (e.g. truncation detectability), test results, and anything you deliberately left out.
