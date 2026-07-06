# Changelog

All notable changes to this project are documented here.

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
