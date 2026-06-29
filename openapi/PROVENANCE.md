# Vendored spec provenance

Phase-0 deliverable (D22). Records where the registry's build inputs come from and which
real Exa surfaces are *not* in any OpenAPI (so they are overlay-defined or raw-only).

## Vendored inputs

| File | Source URL | Format upstream | Identity (verified 2026-06-29) |
|---|---|---|---|
| `exa-openapi.json` | `https://exa.ai/docs/exa-spec.json` | JSON | `Exa Public API` **2.0.0**, OpenAPI 3.1.0, 61 operations |
| `team-management.json` | `https://exa.ai/docs/team-management-spec.yaml` | YAML → normalized to JSON | `Team Management API` **1.0.0**, OpenAPI 3.1.0, 6 operations |
| `overlay.toml` | hand-curated from `docs/v2/commands.md` | TOML | 67 spec ops mapped + 1 overlay-defined (`context`) |

The admin spec is served as YAML; it is normalized to JSON on vendor so the shipped binary
carries no YAML parser (D21). `xtask vendor-spec` re-fetches and re-verifies both; `--check`
verifies offline (identity + overlay consistency). The embedded-spec SHA-256 is computed at
build time over `exa-openapi.json` and surfaced in `capabilities --json` / `doctor`.

The three partial specs under `work/research/` (Search 1.2.0, Websets 0, Team-Management
1.0.0) are **not** vendor sources (D22). The live team-management spec was byte-compared to
the research copy and is identical — there is no newer published version; "stale" meant
"don't trust the research copy blindly," not "a newer one exists."

## Docs-only surfaces (no OpenAPI path)

| Surface | Disposition | Where |
|---|---|---|
| `POST /context` (Exa Code) | **overlay-defined** typed command (`exa-agent context`) | `overlay.toml` → `[operations."context"]` |
| `POST /chat/completions`, `POST /responses` (OpenAI-compat) | **raw-only** in v1 (D16) | `exa-agent raw POST /chat/completions --body @…` |

## Carry-over runtime validations (not blockers)

Resolved in the phase that touches the surface, via `raw`/`--body`/`--set`/`schema refresh`:
Websets export endpoints; Research v1/v0 status; OpenAI `/responses` model names; whether 429
returns `Retry-After`; whether key-create returns a one-time secret; admin `rateLimit`
semantics; whether Exa honors a client `Idempotency-Key` header (D25, Phase 3).
