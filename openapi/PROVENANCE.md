# Vendored spec provenance

Phase-0 deliverable (D22). Records where the registry's build inputs come from and which
real Exa surfaces are *not* in any OpenAPI (so they are overlay-defined or raw-only).

## Vendored inputs

| File | Source URL | Format upstream | Identity |
|---|---|---|---|
| `exa-openapi.json` | `https://exa.ai/docs/exa-spec.json` | JSON | `Exa Public API` **2.0.0**, OpenAPI 3.1.0 |
| `team-management.json` | `https://exa.ai/docs/team-management-spec.yaml` | YAML → normalized to JSON | `Team Management API` **1.0.0**, OpenAPI 3.1.0 |
| `overlay.toml` | hand-curated from `docs/v2/commands.md` and the Websets API guide | TOML | 70 spec ops mapped (64 public + 6 admin) + 1 overlay-defined (`context`) = 71 commands |

Operation counts, vendored SHA-256s, source SHA-256s, and verification dates live in
[`provenance.toml`](provenance.toml) — one machine-readable copy rather than a prose table
that can drift. `cargo run -p xtask -- vendor-spec --check` and the
`provenance_records_the_committed_specs` test in `tests/openapi_parity.rs` both assert the
recorded vendored SHA-256 and operation count against the committed files, so a re-vendor
that forgets to update the record fails offline instead of passing green.

The admin spec is served as YAML; the Rust-only `xtask` dev tool normalizes it to JSON on vendor,
so the shipped binary still carries no YAML parser (D21). `xtask vendor-spec` re-fetches and
re-verifies both; `--check` verifies offline (identity + overlay consistency + the
`provenance.toml` record). The embedded-spec SHA-256 is computed at build time over
`exa-openapi.json` and surfaced in `capabilities --json` / `doctor`.

The three partial specs under `work/research/` (Search 1.2.0, Websets 0, Team-Management
1.0.0) are **not** vendor sources (D22). The live team-management spec was byte-compared to
the research copy and is identical — there is no newer published version; "stale" meant
"don't trust the research copy blindly," not "a newer one exists."

## Docs-only surfaces (no OpenAPI path)

| Surface | Disposition | Where |
|---|---|---|
| `POST /context` (Exa Code) | **overlay-defined** typed command (`exa-agent context`) | `overlay.toml` → `[operations."context"]` |
| `POST /chat/completions`, `POST /responses` (OpenAI-compat) | **raw-only** in v1 (D16) | `exa-agent raw POST /chat/completions --body @…` |

## Payment access modes

The public spec annotates `POST /search` and `POST /contents` with x402/MPP payment metadata.
The CLI does **not** implement wallet custody or signing. It supports only signed stdin
pass-through (`--x402-payment-stdin` / `--mpp-payment-stdin`) and unauthenticated discovery
(`--payment-discovery`) through `raw POST /search|/contents` on the default Exa host.

## Carry-over runtime validations (not blockers)

Resolved in the phase that touches the surface, via `raw`/`--body`/`--set`/`schema refresh`:
OpenAI `/responses` model names; whether 429 returns `Retry-After`; whether key-create returns a
one-time secret; admin `rateLimit`
semantics; whether Exa honors a client `Idempotency-Key` header (D25, Phase 3).
