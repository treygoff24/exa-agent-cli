# v2 Decisions (locked)

Date: 2026-06-29
Status: these supersede the open questions in the v1 `docs/` set. Where v1 and v2 disagree, v2 wins.

This is the ADR-style record of the decisions that gate everything else. Each entry: the call, why, and what we rejected. The contracts in [`contracts.md`](contracts.md) and the architecture in [`architecture.md`](architecture.md) implement these; if a later doc contradicts one of these, the decision here is canonical.

---

## D1 — Language: Rust, single static binary

**Call.** Build the CLI in Rust, distributed as a single statically-linked binary.

**Why.** The primary user is an agent that spawns the binary per call. That makes distribution the dominant constraint: one file an agent can `curl | sh` or `brew install`, no runtime to provision, no version drift. Rust gives a single binary, trivial cross-compilation, `serde` for the JSON shaping this tool is mostly made of, and `clap` — whose built-in suggestion engine delivers the intent-inference axiom (did-you-mean on flags and subcommands) nearly for free.

**Rejected.**
- **Node/Python as the distribution runtime** — "agent needs the right node + global install" or venv/uv hell. The transport/auth/retry/envelope layer is hand-built here regardless, so the TS-SDK-alignment argument buys nothing; the SDK would fight our envelope and raw contracts.
- **Hybrid "generated CLI"** — we do not need full OpenAPI client codegen. We parse the spec **at build time into a data table** (the operation registry) and hand-write commands over it. That is a build script, not a codegen toolchain (see [D9](#d9--operation-registry-generated-from-openapi-at-build-time)).

## D2 — Names: binary `exa-agent`, crate `exa-agent-cli`

**Call.** Invoked command and installed binary: `exa-agent`. Published crate: `exa-agent-cli`. Throughout the docs, examples use `exa-agent`.

**Why.** `exa` is contested: `crates.io/exa` is the old `ls` replacement (v0.10.1, "A modern replacement for ls"), npm `exa` and `exa-cli` are both taken, and the ecosystem successor `eza` is commonly installed (it is on this machine). For an agent tool a *colliding* name is actively harmful — in any environment where ls-`exa` is on `PATH`, the agent's command resolves to the wrong binary. An unambiguous name is the feature.

**Rejected.** Aliasing to `exa` (shadowing risk); `exax`/`exa-cli` (shortlisted but less clear / npm-taken).

## D3 — Default output: auto (JSON when piped, human in a TTY)

**Call.** With no format flag: emit the JSON envelope when stdout is **not** a TTY (the agent case), and human-readable output when stdout **is** a TTY. Override with `EXA_OUTPUT=human|json|ndjson` or an explicit `--format`/`--json`/`--human`.

**Why.** Serves first-try inevitability (Axiom 0): an agent's first guess `exa-agent search "x"` returns parseable data without having to remember `--json`. It is deterministic *within* the agent context — always non-TTY → always JSON. Humans still get pretty output interactively.

**Rejected.** "Always human unless `--json`" (too conservative for an agent-first tool — makes the agent's first guess fail to parse). "Always JSON unless `--human`" (worse interactive ergonomics for the human author with no upside for agents).

**Note on the determinism tension.** Same command can produce different bytes in a pipe vs a terminal. This is acceptable because (a) the agent context is uniformly non-TTY, and (b) any CI/agent harness that wants to pin output sets `EXA_OUTPUT` or passes `--format`. The determinism guarantees in [`contracts.md`](contracts.md) (stable ordering, no timestamp leakage) hold *within* a chosen format.

## D4 — Admin / Team-Management: gated `admin` namespace, shipped in v1

**Call.** Ship `exa-agent admin keys create|list|get|update|delete|usage` in v1, walled off:
- reads a **separate** credential, `EXA_SERVICE_KEY` (never `EXA_API_KEY`), stored under a distinct keyring scope;
- targets the admin host (`EXA_ADMIN_BASE_URL`, default `https://admin-api.exa.ai/team-management`);
- `delete` requires `--confirm <key-id>` (confirm-by-id, not a bare `--yes`);
- the CLI refuses to use a normal API key as a service key and vice versa, with an actionable error.

**Why.** Full surface, safely contained. Key deletion is irreversible and team-wide; mixing a high-privilege admin key into the same store as the everyday search key is a security smell.

**Rejected.** Deferring admin entirely (the user wants full surface in v1); inline/no-gating (footgun).

## D5 — Stateless client; no persistent cache

**Call.** No response cache, no hidden local state. The only local persistence is small **pending-run records** used solely for idempotency on ambiguous async creates (see [D7](#d7--retry-and-idempotency-no-auto-retry-on-creates)), written as append-only JSONL under the run/state dir — not a database.

**Why.** Agents manage their own state; a hidden cache creates non-determinism and surprise. This also means the `rust-cli-with-sqlite` skill does **not** apply — the idempotency record is a JSONL append, not SQLite.

## D6 — Output-format flags consolidated

**Call.** One coherent model (full detail in [`contracts.md`](contracts.md)):
- `--format human|json|ndjson` is the canonical knob.
- `--json` and `--ndjson` are exact documented aliases of `--format json` / `--format ndjson`.
- `--raw` is the **single** spelling for "emit exact upstream bytes, no CLI envelope" (replaces v1's overlapping `--raw-response` *and* `--format raw`). `--raw --stream` yields raw SSE (replaces `--raw-sse`).
- `--pretty` / `--compact` only tweak whitespace; default is pretty in a TTY, compact when piped.

**Why.** v1 had six output flags with overlapping semantics and two ways to ask for raw — a choice-paradox that breaks first-try inevitability and bloats the test matrix. Now: format selects *structure*, `--raw` bypasses the *envelope*, pretty/compact tweak *whitespace* — orthogonal.

## D7 — Retry and idempotency: no auto-retry on creates

**Call.** Automatic retry (`--retry`, default 2) applies to idempotent GETs, network failures, HTTP 429, and 5xx. It **never** auto-retries a create-POST (anything that mints a billable async run: `agent runs create`, `websets create`, `research create`, etc.) unless an `--idempotency-key` is supplied. An ambiguous create failure (request sent, no confirmed response) exits non-zero, writes a pending-run record, and the error names the exact recovery command.

**Why.** This is the one place "agent-friendly auto-retry" and "don't burn the user's money" collide. Retrying a create on a post-send timeout double-bills. The rule lives in the transport layer so no command can get it wrong.

## D8 — Doctor: read-only diagnostics in v1 (right-sized)

**Call.** `exa-agent doctor` is read-only and offline by default; network checks behind `--online`. Every finding names its exact fix command (mirroring the error envelope's `suggestedCommand`). **No `--fix`/`undo`/backup machinery in v1.**

**Why.** The `cli-doctor-mode` skill's heavy machinery (`mutate()` chokepoint, backups, `actions.jsonl`, `undo`) is for tools with mutable local state — databases, installers. This is a near-stateless API client; its broken states (missing/bad key, dead base-url, malformed config, stale embedded spec, TTY/color misconfig) are diagnose-and-suggest, not mutate-and-undo. We deliberately use the *lean* doctor, not the premium `world-class-doctor-mode-for-cli-tools` v7.

**Upgrade path.** If `doctor --fix` is ever added, the only legitimate targets are config-file rewrites — and *only then* do we adopt the full chokepoint + backup + `undo` discipline. That boundary is written down so it is not over-built early.

## D9 — Operation registry generated from OpenAPI at build time

**Call.** A `build.rs` step parses the embedded Exa OpenAPI snapshot into a static operation registry (operation id, method, path, request fields, pagination style, streaming support, deprecation, safety metadata). Commands, `capabilities`, `schema`, and local validation read from it. The embedded spec's SHA-256 is surfaced in `capabilities --json`. We commit the build **inputs** (the vendored JSON spec(s) + `overlay.toml`); the registry is generated into `OUT_DIR` at build time and is **not** committed — it is regenerated reproducibly from the committed inputs, and a CI gate diffs a fresh re-generation. Drift is detected by `schema refresh --check` and a CI job — never on the hot path.

**Why.** Keeps full-surface coverage honest without hand-maintaining endpoint breadth, while staying a single binary with offline self-description.

## D10 — Context-window-aware output: `--output FILE`

**Call.** Universal `-o/--output FILE`. Default content controls stay conservative (highlights over full text; `--text-max-characters`). When a single response's `data` exceeds a threshold, the envelope may carry `{ "dataTruncated": true, "dataPath": "...", "bytes": N }` instead of inlining a huge payload (auto-spill is opt-in/threshold-gated — ship `--output` for v1, enable auto-spill only if agents hit the wall).

**Why.** `contents --text` over a few long pages is 100k+ tokens dumped to stdout — it blows the agent's context window. Writing the big payload to disk and returning a handle is the agent-ergonomic move.

## D11 — Auth storage: env-first, optional keyring, no plaintext by default

**Call.** Credential precedence: `--api-key` (one-shot, never persisted) > `EXA_API_KEY` env > OS keyring profile > config metadata. Service key is the parallel chain with `EXA_SERVICE_KEY` and a separate keyring scope. Config files store profile *metadata and env-var names*, never plaintext keys by default. Keys are redacted everywhere (last4/fingerprint only).

**Why.** Env-first is what agents and CI already do; keyring is the opt-in upgrade; plaintext-in-config is a footgun we don't ship.

## D12 — v1 trims the preset/profile system

**Call.** v1 ships: `EXA_API_KEY` env-first auth, a minimal config file (base-url, default format, timeout, retry), `--profile` selecting a `[profiles.X]` block, and two thin macros (`ask`, `fetch`). The full configurable **preset/macro registry** (`preset show`, presets-in-TOML, `macro show`) is deferred to a later phase.

**Why.** The contracts (envelope, exit codes, capabilities, raw) earn their rigor — that's load-bearing. The preset/profile *system* is surface we don't need to ship to be useful. Macros stay because they're cheap expansions that serve first-try inevitability; the configurable engine behind them can wait.

## D13 — Parser: `clap` (derive), lean on its suggestion engine

**Call.** Use `clap` v4 with the derive API. Use `ValueEnum` for fixed-set flags (`--type`, `--format`), `clap`'s arg-conflict rules for local validation, and its built-in suggestions for did-you-mean on flags and subcommands. There is no clap-specific skill; framework patterns come from the `rust-engineer` skill + clap docs via `ctx7`.

**Why.** clap's suggestion engine is the cheapest path to the intent-inference axiom; `ValueEnum` makes invalid-choice errors self-documenting.

---

## Addenda (reconciliation pass, 2026-06-29)

These ratify or refine D1–D13 after v2 doc drafting surfaced concrete sub-decisions. They are canonical.

**D14 — HTTP client: `ureq` + rustls (no async runtime).** Refines D1. `reqwest::blocking` embeds an internal current-thread tokio runtime, which would drag tokio+hyper into a tool that never writes `async`. `ureq` (rustls, blocking, plain `Read` for SSE) has no runtime and the smallest dep tree; it sits behind a one-method `Transport` trait so it stays swappable. rustls (not native-tls/OpenSSL) keeps musl static-linking clean.

**D15 — Keyring is feature-gated.** Refines D11. The `keyring` dependency is a default cargo feature, **on** for the macOS/Windows release artifacts and **off** for the musl Linux artifacts (secret-service/dbus does not static-link cleanly on musl). Env-first auth (`EXA_API_KEY` / `EXA_SERVICE_KEY`) works on every artifact regardless. Keyring scopes are `exa-agent:api:<profile>` and `exa-agent:service:<profile>`.

**D16 — OpenAI-compat surfaces deferred to post-v1.** The `openai` namespace (chat-completions / responses) is **not** a v1 command; `raw` covers it (`exa-agent raw POST /chat/completions --body @...`). Thin wrappers are a post-v1 phase.

**D17 — Registry = OpenAPI snapshot + curated `overlay.toml`.** Refines D9. Vanilla OpenAPI can't express the CLI command path, `dangerous`, idempotency-sensitivity, or cursor field names. A second committed build input, `openapi/overlay.toml`, supplies that metadata; the build merges spec + overlay into the registry. The HTTP method per command therefore lives in the registry (and is surfaced in `capabilities --json`), so a uniform `update` verb over PATCH/POST/PUT abstracts the method rather than hiding it. **Two refinements from the Phase-0 spec audit (see D22):** (1) the registry merges **two** upstream specs — the public `exa-spec.json` and the separate Team-Management spec (admin keys, host `admin-api.exa.ai`); (2) `overlay.toml` may also **fully define** operations that are real but absent from any OpenAPI (e.g. `/context`), not just annotate existing ones — an overlay-defined op carries the same `OperationDef` fields as a spec-derived one.

**D18 — `--header` cannot override managed auth headers.** An injected `--header` may add headers but is refused (exit 1) if it targets `Authorization` or a known secret header — prevents credential shadowing / leak / prompt-injection.

**D19 — "Stateless" means no cache, not "never writes disk."** Refines D5. The client legitimately writes the config file, the OS keyring, append-only pending-run JSONL (D7), `--trace` files, and auto-spill temp files (D10). None is a response cache; none affects result determinism.

**D20 — Search count is `--num-results` (`-n`), never `--limit`.** `--num-results N` (1..100, maps `numResults`) is canonical for `search`, with `-n` as the short alias. `--limit` is reserved for cursor-paginated lists; passing it to `search` yields a did-you-mean naming `--num-results`, not a silent alias (search isn't cursor-paginated).

**D21 — Spec vendored as normalized JSON; no YAML parser in the binary.** Resolves an architecture/plan split. `xtask vendor-spec` fetches the upstream `exa-spec.json` **directly** (Exa serves it as JSON — no YAML conversion needed) into committed `openapi/exa-openapi.json`, and the Team-Management spec into `openapi/team-management.json`; `build.rs` parses those (+ `overlay.toml`) into the registry, and the binary embeds the JSON. The shipped binary carries neither an async runtime (D14) nor a YAML parser; `serde_norway` is pulled into the `xtask` tool only if a future source is YAML-only. SIGINT during a blocking SSE read is handled by an interrupt flag checked in the read loop (→ exit 12), so async is unnecessary.

**D22 — Canonical spec sourcing (verified 2026-06-29).** The registry's primary source is the live consolidated **`https://exa.ai/docs/exa-spec.json`** — verified to report `info.title = "Exa Public API"`, `info.version = "2.0.0"`, and to contain `/search`, `/contents`, `/answer`, `/findSimilar`, `/agent/runs`(+sub), `/monitors`(+sub), `/research/v1`, `/v0/websets` with items/searches/enrichments/imports/events/webhooks/monitors, and `/v0/teams/me`. **Absent from the OpenAPI (docs-only):** `/context` → an **overlay-defined** typed command (D17); `/chat/completions` + `/responses` → raw-only (D16). **Admin keys** (`/api-keys`) come from the separate Team-Management spec (`admin-api.exa.ai`), vendored alongside. The three partial specs in `work/research/` (Search 1.2.0, Websets 0, Team-Management 1.0.0) are **stale and must not be the vendor source**. Vendoring the live specs is a **Phase-0 blocker** — the Phase-1 goldens freeze the title/version/`embeddedSpecSha256` derived from them.

**Spec-vs-live drift (verified 2026-07-01).** `GET /v0/teams/me` is in the spec but **upstream does not serve it** — the live API returns a 404 with an HTML catch-all body (not a JSON API 404), against both the vendored snapshot and the current live spec. So `team info` returns a clean `not_found` (exit 7) by design; this is **not** a CLI bug and the OpenAPI-parity harness should not treat it as one. It will start working automatically if Exa ships the endpoint. Consequently the online credential probe (`auth test`, `doctor --online` `auth.online`) does **not** use this endpoint — it uses a billing-free `POST /search` with an empty body (auth is validated before the body: good key → 400 `INVALID_REQUEST_BODY`, bad key → 401/403, 5xx/429 → inconclusive).

---

## Addenda (skill-audit pass, 2026-06-29)

These ratify the changes from the `rust-agent-cli` skill audit ([`reviews/rust-agent-cli-skill-audit.md`](reviews/rust-agent-cli-skill-audit.md)). They are canonical; full detail lives in the contracts/architecture/commands/plan sections cited.

**D23 — clap parse errors are remapped, never leaked.** `lib.rs::run` catches `clap::Error` via `try_parse`: `--help`/`--version` → stdout, exit 0; every other kind → `CliError::Usage` (exit **1**), rendered as `exa.cli.error.v1` with clap's suggestion mirrored into `error.details.didYouMean`. clap's default exit **2** (which collides with `auth`) and raw text never surface. (contracts §5/§6, arch §6/§10)

**D24 — the `error.code` vocabulary is published.** `error.code` is the agent's primary branch signal, so the full dictionary is enumerated (contracts §5.1), surfaced in `capabilities --json` as a populated `errorCodes` (and `exitCodes`), and golden-pinned. No more `"exitCodes": {}` placeholder.

**D25 — `--idempotency-key` is forwarded upstream as an `Idempotency-Key` header**, not just used as a local retry gate — that header is what makes a keyed auto-retry non-double-billing. Whether Exa honors it is a new carry-over validation (below); if it does not, keyed auto-retry is disabled. (contracts §7, arch §5)

**D26 — the installer is a specified contract, not a one-liner.** `curl|sh` MUST verify the checksum before installing, be non-TTY-safe (flag/env-driven, zero prompts), never silently source-compile on a triple mismatch, be idempotent, and end with a grep-able `INSTALL_OK …` line. Generated via **cargo-dist** baseline. Three acquisition paths: curl|sh, `cargo binstall`, `cargo install`. (plan → Release)

**D27 — `capabilities` surfaces the per-command blast-radius triad** (`readOnly`/`destructive`/`idempotencySensitive`, + `requiresConfirm`/`dangerous`), derived from the registry+overlay, so an agent sees *before calling* which creates need `--idempotency-key`. (contracts §13, arch §3)

**D28 — the success envelope gains `nextActions`, `count`, `dataHash`, `request.correlationId`.** `nextActions[]` = paste-ready follow-ups on async-create/paginated commands (the success-path analogue of `suggestedCommand`); `count` survives a spill; `dataHash` is a change-fingerprint; `pagination.total` is nullable. (contracts §4)

**D29 — `--max-output-bytes` is a default-on output ceiling** (≈1 MiB) that spills over-ceiling payloads to a file + handle, so an unguarded `contents --text` can't blow the context window even without `--output`. Complements (does not replace) the deferred auto-spill of D10. (contracts §9)

**D30 — `--correlation-id` / `EXA_CORRELATION_ID`** is echoed verbatim into `request.correlationId` across stdout/stderr/`--trace`, so an orchestrator can stamp its own key instead of scraping `requestId`. (contracts §4)

**D31 — input forgiveness is specified at the parse boundary.** Every `ValueEnum` sets `ignore_case = true`; optional bools use `BoolishValueParser` (`true/1/yes/on`); placeholder literals (`<id>`, `$VAR`, `YOUR_*`) are rejected with a discovery hint (`placeholder_argument`). Opaque Exa ids get no prefix coercion (documented non-feature). (commands §3, arch §6)

**D32 — `doctor` uses a linter-style exit dictionary** — `0 = healthy / 1 = findings / 4 = refused-unsafe` — **not** the §6 categories, so a `doctor` exit can't be confused with a real `auth`/`config` failure. Output is `exa.cli.doctor.v1`, golden-pinned; detector ids + exit map published in `capabilities.doctor`. (contracts §15, arch §9)

**D33 — auth failures are two distinct codes.** `not_authenticated` (nothing resolved locally; `details.checked` lists the ladder rungs tried) vs `reauth_required` (a key was sent and upstream rejected it), so an agent branches "set a key" vs "rotate the key." Keyring reads are **non-blocking / fail-fast** (a macOS Keychain prompt must not hang a per-call agent). (contracts §5.1, arch §8)

**D34 — the streaming event gains a top-level `type` discriminator, `timestamp`, and `correlationId`** so an agent routes records without unwrapping the opaque upstream blob. (contracts §8)

**D35 — the pending-run JSONL has a crash-safe write contract**: one newline-terminated object via a single `O_APPEND` write (atomic for a sub-`PIPE_BUF` line), flushed before exit, bounded/rotated, never read back by the CLI. (contracts §7, arch §5)

**D36 — supply-chain scanning is in the gate.** `cargo deny check` + `cargo audit` (committed `deny.toml`) run in `cargo xtask ci`, and `cargo audit` is a hard gate before `publish-crates`. The `cargo tree -i` ban-list (D14/D21) enforces architecture, not CVEs — both are needed. (plan → gates/CI)

**D37 — binary provenance is baked** (`GIT_SHA`/`BUILD_DATE`/`TARGET`) into `capabilities.build` and `--version`, so a stale build is detectable without network. (arch §3, contracts §13)

**D38 — confusable pairs get reciprocal did-you-mean.** `monitor` ↔ `websets monitors` (singular/plural) and `--num-results` (search) ↔ `--count` (websets) each redirect both directions (never aliased, per D20's posture). `--dry-run` is **local-only** (≡ `--print-request`, no network); a genuine server-side preview is always a distinct named path (`websets preview`, upstream `dry_run`). (commands §1/§3/§4)

**D39 — `--body` deep-merges over named flags** (fixed precedence `defaults < flags < --body < --set`, inspectable via `--print-request`) — resolving the `commands.md`/`architecture.md` wording split (it does *not* "bypass" flags). `--body -`/`--input -` are TTY-guarded (→ exit 11, never block). (commands §3, arch §4)

---

## Carry-over open validations (unchanged from v1)

These remain runtime-validation items, not blockers — the `raw` + `--body` + `--set` + `schema refresh` escape hatches cover them:

- Canonical spec URL + drift cadence; Websets export endpoints; Research `v1`/`v0` status; OpenAI `/responses` model names; whether 429 returns `Retry-After`; whether key-create returns a one-time secret; admin `rateLimit` semantics.
- **Whether Exa honors a client `Idempotency-Key` header on create-POSTs (D25).** The no-double-bill safety model assumes it does; if a smoke probe shows it doesn't, keyed auto-retry is disabled and `--idempotency-key` is used only for the pending-run recovery record. Resolved in Phase 3 (where create-POSTs are first exercised live), not up front.
