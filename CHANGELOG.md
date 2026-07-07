# Changelog

All notable changes to this project are documented here.

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
