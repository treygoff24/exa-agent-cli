# Vendored spec provenance

The registry is generated from the committed specs and overlay below.
Operation counts, hashes, and verification dates identify the exact inputs; an
offline identity check does not establish that upstream has stayed unchanged.

## Vendored inputs

| File | Source URL | Format upstream | Identity |
|---|---|---|---|
| `exa-openapi.json` | `https://exa.ai/docs/exa-spec.json` | JSON | `Exa Public API` **2.0.0**, OpenAPI 3.1.0 |
| `team-management.json` | `https://exa.ai/docs/team-management-spec.yaml` | YAML → normalized to JSON | `Team Management API` **1.0.0**, OpenAPI 3.1.0 |
| `overlay.toml` | maintained CLI mappings and constraints | TOML | 70 spec ops mapped (64 public + 6 admin) + 1 overlay-defined (`context`) = 71 commands |

Operation counts, vendored SHA-256s, source SHA-256s, and verification dates live in
[`provenance.toml`](provenance.toml). `cargo run -p xtask -- vendor-spec --check` and the
`provenance_records_the_committed_specs` test in `tests/openapi_parity.rs` both assert the
recorded vendored SHA-256 and operation count against the committed files, so a re-vendor
that forgets to update the record fails offline instead of passing green.

The admin spec is served as YAML; the Rust-only `xtask` dev tool normalizes it to JSON on vendor,
so the shipped binary still carries no YAML parser (D21). `xtask vendor-spec` re-fetches and
re-verifies both; `--check` verifies offline (identity + overlay consistency + the
`provenance.toml` record). The embedded-spec SHA-256 is computed at build time over
`exa-openapi.json` and surfaced in `capabilities --json` / `doctor`.

The three partial specs under `work/research/` (Search 1.2.0, Websets 0, Team-Management
1.0.0) are **not** vendor sources (D22). The initial team-management comparison found identical bytes. That is a dated
observation, not proof that no newer version exists. Fresh fetches must be compared
with the committed provenance each maintenance cycle.

## Docs-only surfaces (no OpenAPI path)

| Surface | Disposition | Where |
|---|---|---|
| `POST /context` (Exa Code) | **overlay-defined** compatibility command (`exa-agent context`), now undocumented upstream; live support is not established by offline validation | `overlay.toml` → `[operations."context"]` |
| `POST /chat/completions`, `POST /responses` (OpenAI-compat) | **raw-only** (D16) | `exa-agent raw POST /chat/completions --body @…` |
| `/agent/monitors` and related entities, changes, and backtest routes | **omitted from the typed surface**: SDK-only beta, absent from the public spec and reference index as of 2026-10-07 | Raw access requires an independently verified request contract and beta context; stable monitors and Websets remain typed |

## Payment access modes

The public spec annotates `POST /search` and `POST /contents` with x402/MPP payment metadata.
The CLI does **not** implement wallet custody or signing. It supports only signed stdin
pass-through (`--x402-payment-stdin` / `--mpp-payment-stdin`) and unauthenticated discovery
(`--payment-discovery`) through `raw POST /search|/contents` on the default Exa host.

## Runtime evidence

Vendored schemas describe request shapes; they do not prove account entitlements,
OpenAI-compatible model availability, response headers, or upstream idempotency
behavior. Verify those against an authorized live call when needed. The October
local candidate was checked with live search and contents only; no paid Ultra or
live Agent/Websets mutation was part of that verification.
