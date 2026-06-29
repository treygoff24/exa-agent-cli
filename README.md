# Exa Agent CLI

Agent-first CLI for the full Exa API surface.

This repo is currently in research/design mode. The goal is a CLI that lets agents use every Exa capability: search, contents, answer, context/code search, Agent runs, research compatibility, monitors, Websets, events/webhooks/imports/enrichments/exports, team/API-key administration, OpenAI-compatible surfaces, schemas, and raw passthrough.

## Current docs

**v2 (canonical) — the Rust design set.** Start at [`docs/v2/README.md`](docs/v2/README.md).

- [`docs/v2/decisions.md`](docs/v2/decisions.md) — the 39 locked decisions + rationale (D1–D39; D23–D39 from the skill-audit pass).
- [`docs/v2/contracts.md`](docs/v2/contracts.md) — agent-facing wire/output spec (envelopes, exit codes, retry, streaming).
- [`docs/v2/commands.md`](docs/v2/commands.md) — full command tree + flag taxonomy.
- [`docs/v2/architecture.md`](docs/v2/architecture.md) — Rust build-shape (crate layout, transport, build-time registry codegen).
- [`docs/v2/implementation-plan.md`](docs/v2/implementation-plan.md) — phased build, testing, CI, release.

**v1 (superseded, retained for traceability).**

- [`docs/research/exa-api-research.md`](docs/research/exa-api-research.md) — integrated Exa API research and source map.
- [`docs/cli-architecture.md`](docs/cli-architecture.md) — original language-agnostic command model and output contract.
- [`docs/implementation-plan.md`](docs/implementation-plan.md) — original staged build plan.
- [`CONTEXT.md`](CONTEXT.md) — project glossary and domain language (still current).

## Local research corpus

Primary-source snapshots and subagent lane reports are under [`work/research/`](work/research/). They are intentionally kept in-repo for traceability during planning.
