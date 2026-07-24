# v2 Architecture (Rust)

Date: 2026-06-29

Status: implements [`decisions.md`](decisions.md) (D1–D22) and [`contracts.md`](contracts.md). Where this doc and a decision/contract disagree, the decision/contract wins and the gap is logged in [Open seams](#open-seams). Schema ids, field names, and exit codes below are copied from `contracts.md` verbatim — they are not re-litigated here.

This is the build-shape of `exa-agent`: how a single static binary turns an agent's first-guess command into a request, runs it through one auth/retry/redaction chokepoint, and emits a stable envelope keyed off a registry generated from the embedded Exa OpenAPI snapshot.

---

## 1. Crate / workspace layout

**One package, lib + bin, thin `main`.** The package is `exa-agent-cli` (D2); it produces a single binary named `exa-agent` via `[[bin]] name = "exa-agent"`. `src/main.rs` is a ~5-line shim that calls `exa_agent_cli::run()` and `std::process::exit`s the returned code; all logic lives in the lib target so integration tests (`assert_cmd` against the binary) and unit tests (in-module) both have a real surface to drive.

Not a multi-crate workspace in v1. A `core` lib + `cli` bin split would force two published crates and a version-coupling chore for a v1 that ships one binary; `build.rs` also wants to live in the crate that owns the registry *types*, and a single package keeps codegen in one place. The lib+bin single package gives every benefit of the split (testable library, thin entrypoint) at none of the publish cost. **Upgrade path:** if transport/registry ever need to be consumed as a standalone library, promote `src/registry` + `src/transport` into a `exa-agent-core` crate then; the module boundaries below are drawn so that lift is mechanical.

```text
exa-agent-cli/
├── Cargo.toml
├── build.rs                     # registry codegen (§3)
├── openapi/
│   ├── exa-openapi.json            # committed OpenAPI snapshot (embedded via include_bytes!)
│   └── overlay.toml             # curated CLI metadata keyed by operationId (§3)
├── src/
│   ├── main.rs                  # std::process::exit(exa_agent_cli::run())
│   ├── lib.rs                   # run() -> i32: parse, dispatch, map CliError -> exit code
│   ├── cli/                     # clap derive surface (§6)
│   │   ├── mod.rs               #   Cli (Parser), Command (Subcommand), dispatch table
│   │   ├── globals.rs           #   GlobalArgs: #[command(flatten)], global = true
│   │   ├── value_enums.rs       #   Format, Category, Livecrawl, ... (ValueEnum)
│   │   └── <group>.rs           #   search, contents, answer, agent, websets, admin, ...
│   ├── registry/
│   │   ├── mod.rs               #   Registry, OperationDef, FieldDef; lookup_by_id()
│   │   └── generated.rs         #   include!(concat!(env!("OUT_DIR"), "/registry.rs"))
│   ├── request.rs               # RequestSpec assembly: flags + --body + --set (§4)
│   ├── transport/
│   │   ├── mod.rs               #   Transport::send — the one HTTP chokepoint (§5)
│   │   ├── retry.rs             #   RetryPolicy::classify — encodes D7
│   │   └── stream.rs            #   SSE reader -> Iterator<Event>
│   ├── exec.rs                  # orchestration: single / paginate(--all) / stream (§5,§10)
│   ├── output/
│   │   ├── envelope.rs          #   Response/Error/Event/Capabilities structs (§7)
│   │   ├── format.rs            #   OutputMode resolution, human renderer, ndjson
│   │   └── sink.rs              #   stdout=data / stderr=diagnostics; --output / auto-spill
│   ├── auth.rs                  # credential precedence, keyring scopes (§8)
│   ├── config.rs                # TOML config, profile selection (§8)
│   ├── doctor.rs                # read-only detectors (§9)
│   ├── redaction.rs             # secret scrubbing, shared by transport + output + trace (§10)
│   ├── error.rs                 # CliError (thiserror) -> category -> exit code (§10)
│   └── state.rs                 # requestId (ULID), pending-run JSONL — atomic O_APPEND write (D7, §5)
└── tests/                       # insta golden snapshots + assert_cmd integration
```

Module grouping rationale (lean, not flat): `cli` is the only place clap types live; `registry` is the only place the generated table lives; `transport` is the only place an HTTP call happens; `output` is the only place bytes reach stdout/stderr. Everything else (`auth`, `config`, `doctor`, `redaction`, `error`, `state`, `request`, `exec`) is a single flat file until it earns a directory. Eleven units for the full Exa surface is on the lean side of the line.

**Dispatch is generic and registry-driven — there is no `src/commands/` per-command-handler layer.** `cli/mod.rs::dispatch` maps the parsed `Command` to its `OperationDef` (via `cli_path`), then calls `request::build_request` → `exec::run`; the typed `cli/<group>.rs` structs only *collect* flags (clap types, no logic). The handful of commands that aren't a uniform API call — `capabilities`, `schema`, `robot-docs`, `doctor`, `config`, `auth`, the `ask`/`fetch` macros — are small functions dispatched directly from `cli/mod.rs` (the macros expand to a canonical `Command` and re-enter dispatch). So the SKILL's "`src/commands/`" slot is satisfied by `request.rs` + `exec.rs` + the generic dispatch table rather than N hand-written handlers; "a handler returns a `Plan`" (§5) means *the generic dispatch path* produces a `Plan`, not that per-command handler modules exist.

---

## 2. Dependencies

| Crate | Features | Why |
|---|---|---|
| `clap` | `derive`, `env`, `wrap_help` | Args + subcommands + `ValueEnum`; built-in did-you-mean suggestions satisfy the intent-inference axiom (D13); `env` lets fixed flags fall back to env vars natively. |
| `serde`, `serde_json` | `serde/derive`, `serde_json/preserve_order` | Envelope (de)serialization. `preserve_order` (indexmap-backed) keeps upstream `data` key order stable so identical upstream bytes serialize identically (contracts §12). |
| `ureq` | `tls` (rustls) , `gzip` | Blocking HTTP. **No async runtime at all** — see note below. rustls → static-binary friendly, no OpenSSL. Its `Read`-based response body is exactly what SSE line-parsing wants. |
| `toml` | `serde` | Config file parse/serialize (§8). |
| `directories` | — | XDG/OS config + state dir resolution (config, pending-run JSONL, auto-spill). |
| `keyring` | feature-gated `keyring` | Optional OS keyring (Keychain / secret-service / Win cred mgr). Behind a cargo feature so a minimal musl build can drop it (env-first still works). |
| `thiserror` | — | `CliError` enum → category → exit code (§10). Typed errors, not `anyhow`, on the hot path because exit-code mapping is load-bearing. |
| `ulid` | — | `request.requestId` (`req_local_01J…`, contracts §4); seedable for `SOURCE_DATE_EPOCH` golden determinism. |
| `jiff` | — | HTTP-date `Retry-After` parsing and any CLI-emitted timestamp; `SOURCE_DATE_EPOCH`-aware. |
| `humantime` | — | `--timeout 30s` / `--page-delay 250ms` duration flags. |
| `ctrlc` | — | Installs the SIGINT handler that sets the interrupt flag polled by the SSE read loop (§5/D21) → exit 12. Keeps the no-async-runtime story intact (no `tokio::signal`). |
| `tempfile` | — | Atomic temp-file-then-`persist`(rename) for `--output` / auto-spill payloads under the state dir (§7). The pending-run JSONL itself uses plain `O_APPEND` (§5), not temp+rename. |

Build-dependencies (not in the shipped binary): `serde_json` to parse the committed normalized snapshot `openapi/exa-openapi.json` (the YAML→JSON normalization runs once in the `xtask` build tool via `serde_norway`, never in `build.rs` or the binary, D21), `toml` to parse the overlay, `sha2` to compute the embedded-spec SHA-256 at build time, `anyhow` for build-script ergonomics (exit codes don't matter in `build.rs`).

Dev-dependencies: `insta` (golden snapshots for the surfaces frozen in contracts §14), `assert_cmd` + `predicates` (drive the real binary; assert stdout/stderr discipline and exit codes).

**Deliberately excluded.** `tokio` / `futures` / `async-std` — this is a per-call CLI; every operation is one blocking request (or one blocking SSE read loop), so an async runtime is pure cost (binary size, compile time, a reactor we'd never multiplex). A full OpenAPI client generator (`progenitor`, `openapi-generator`) — D1 rejects codegen; we parse the spec to a *data table*, not a client. `sqlite`/`sled` — D5 forbids a cache; the only persistence is an append-only JSONL. `regex` — redaction is a targeted key-prefix scan + header-name set, not pattern soup.

**Note — `ureq` over `reqwest::blocking` (a justified divergence from the brief's suggested default).** The brief suggested blocking `reqwest`. I chose `ureq` because `reqwest::blocking` is a synchronous *façade over an internal current-thread tokio runtime* — it drags `tokio` + `hyper` into the dependency tree and binary even though we never write `async`. `ureq` has no runtime, the smallest dep tree of the rustls blocking clients, and a plain `Read` body for SSE. The whole client sits behind a one-method `Transport` trait (§5), so this is reversible: if we later need reqwest's richer proxy/multipart handling, we swap one impl and nothing above transport changes. Logged in [Open seams](#open-seams).

`#![forbid(unsafe_code)]` is set at the crate root. It is crate-local — rustls's crypto backend contains `unsafe` in *its* code, which is fine; our code has none.

---

## 3. Build-time registry codegen (D9)

The registry is generated from **committed inputs**: `openapi/exa-openapi.json` (the public Exa OpenAPI, fetched as JSON by `xtask vendor-spec`, D21/D22), `openapi/team-management.json` (the separate admin-keys spec, host `admin-api.exa.ai`), and `openapi/overlay.toml` (CLI-specific metadata vanilla OpenAPI cannot express — `dangerous`, idempotency-sensitivity, cursor field names, the CLI command path — **plus full definitions for real-but-docs-only operations such as `/context`**, D22). `build.rs` merges them.

**Generated const Rust for the hot path; embedded bytes for the cold path.** Two artifacts, by access frequency:

1. **`$OUT_DIR/registry.rs`** — a `static REGISTRY: &[OperationDef]` of plain `const`-friendly types (`&'static str`, small enums, `&'static [FieldDef]`). `include!`'d into `src/registry/generated.rs` so the consts name the local types. This is read on **every** invocation (command routing, `capabilities`, flag→field mapping) and must cost nothing — no startup parse, compile-time-checked, branch-predictable. A parsed-at-startup blob (postcard/rkyv `include_bytes!` + deserialize) was rejected for the hot path: it trades a clean zero-cost table for a startup allocation we don't need at this size (a few dozen operations).

2. **`include_bytes!("../openapi/exa-openapi.json")`** — the normalized JSON snapshot, embedded whole and parsed with `serde_json` on cold paths (no YAML parser in the binary, D21). Used only on **cold** paths: computing/serving the spec for `schema export`, and full JSON-Schema validation of `--body` / `schema validate-input`. The SHA-256 is computed at build time over these exact bytes and emitted as `pub const EMBEDDED_SPEC_SHA256: &str`, alongside `SPEC_VERSION`/`SPEC_TITLE`/`SPEC_URL` pulled from the spec's `info` block + overlay. `capabilities --json` reads the const SHA (contracts §13) — no runtime hashing on the common path. `build.rs` also bakes **binary provenance** — `GIT_SHA`, `BUILD_DATE`, and the `TARGET` triple (from the `TARGET` env Cargo sets) — into consts surfaced as `capabilities.build` and `--version` (and reported by the `binary.version` detector), so an agent or `doctor` can tell a stale build from a release without network. `SOURCE_DATE_EPOCH` is honored for `BUILD_DATE` to keep reproducible builds reproducible.

Each `OperationDef` carries exactly what the contracts need plus the internal metadata commands route on:

```rust
pub struct OperationDef {
    pub cli_path:    &'static [&'static str], // ["agent","runs","create"]  -> capabilities.commands[].path
    pub operation_id:&'static str,            // "search"
    pub method:      Method,                  // GET | POST | PATCH | DELETE
    pub api_path:    &'static str,            // "/search"            -> capabilities.commands[].apiPath
    pub read_only:   bool,                    //                      -> readOnly
    pub streaming:   bool,                    //                      -> streaming
    pub pagination:  Pagination,              // None | Cursor { field: &'static str }  -> pagination
    pub dangerous:   bool,                    //                      -> dangerous  (gates --yes/--confirm)
    pub namespace:   Namespace,               // Api | Service        (selects credential + host, D4)
    pub idempotency_sensitive: bool,          // true => create-POST; D7 no-auto-retry list (contracts §7)
    pub deprecated:  bool,
    pub fields:      &'static [FieldDef],      // named-flag <-> body-field map (§4)
}

pub struct FieldDef {
    pub flag:      &'static str,    // "--num-results"
    pub body_path: &'static str,    // "numResults"   (dotted for nested, e.g. "contents.text")
    pub kind:      FieldKind,       // Str | Int | Bool | Num | StrArray | Enum(&'static [&'static str]) | Json
    pub required:  bool,
}
```

`supportsRawBody` and `supportsPrintRequest` in `capabilities` are universally `true` (every operation accepts `--body`/`--set` and `--print-request`), so they're emitted as constants rather than per-op fields. The per-command **blast-radius triad** in `capabilities --json` (contracts §13) is derived here: `readOnly` = `read_only`, `idempotencySensitive` = `idempotency_sensitive`, `destructive` = `dangerous || method == DELETE`, `requiresConfirm` = `dangerous`. Surfacing `idempotencySensitive` is what lets an agent see *before calling* which creates need `--idempotency-key`.

**Merge precedence & deterministic codegen.** When the overlay and a spec disagree, the rule is fixed: **overlay wins** on the CLI-only metadata it owns (`cli_path`, `dangerous`, `idempotency_sensitive`, cursor field names, `namespace`); the **spec wins** on `method`/`api_path`/field shapes — *unless* the op is overlay-*defined* (e.g. `/context`), in which case the overlay supplies all `OperationDef` fields. `build.rs` emits operations sorted by `(namespace, cli_path)` and fields by declaration order. The fixed-environment registry check compares normalized capabilities output (excluding build provenance) twice; it is the supported reproducibility guarantee for the uncommitted `OUT_DIR` artifact.

**Drift is never on the hot path (D9).** `schema refresh --check` fetches the live spec and diffs it against the embedded snapshot; a CI job runs it. The running binary trusts its embedded snapshot and surfaces the SHA so an agent can detect staleness itself. A `tests/` consistency test asserts three invariants: (a) every `OperationDef.cli_path` resolves to a real clap subcommand and vice versa; (b) the set of `idempotency_sensitive` operations equals the create-POST list in contracts §7 exactly; (c) every `dangerous` op's handler requires `--yes`/`--confirm`. These keep the hand-written surface and the generated table from drifting apart.

---

## 4. Request builder

One function turns a parsed command into an upstream request body, regardless of which command called it. The merge has a fixed precedence, last-writer-wins per JSON path:

```
registry field defaults  <  named flags  <  --body JSON|@file|-  <  --set path=value (repeatable)
```

Named flags map onto body fields through `OperationDef.fields`: each `Some(flag value)` is written to its `body_path` with the declared `kind`. `--body` supplies a whole JSON object (parsed, then deep-merged over the flag-built object). Each `--set users.0.name=value` applies a single dotted-path assignment over the result (JSON-Pointer-style traversal, creating intermediate objects/arrays). This ordering means an agent can take a fully flag-driven command and surgically override one nested field with `--set` without restating the rest — and `--body` is the escape hatch for fields no flag models yet (D1's raw-passthrough guarantee, at the field level).

```rust
pub struct RequestSpec {
    pub op: &'static OperationDef,
    pub body: serde_json::Value,     // the merged object (empty for GETs that take only query params)
    pub query: Vec<(String, String)>,// pagination cursor/limit etc.
    pub extra_headers: Vec<(String, String)>, // from --header (never overrides managed Authorization, §8)
}

fn build_request(op, flags, body_opt, sets) -> Result<RequestSpec, CliError> { /* merge in the order above */ }
```

**Stdin inputs are read through one guarded helper.** `--body -`, `--body @file`, and `--input -`/`@file` all go through a shared `read_input` that **rejects `-` when `stdin().is_terminal()`** (returning `CliError::NoInput` → exit 11 with a `suggestedCommand`) instead of blocking on an empty TTY — the agent-hang footgun the SKILL's non-interactivity rule forbids. A missing `@file` is likewise `NoInput`, not a panic. This mirrors the meticulous stdout TTY-detection (§7) on the stdin side.

`--print-request` and `--dry-run` short-circuit here, *before* transport: serialize the `RequestSpec` (method, resolved URL, headers with secrets redacted, body) to the chosen format and return exit 0 without sending — no network, no quota. Local validation runs at the end of this stage: required-field presence and `Enum` membership come from `FieldDef` (cheap, every call); a full JSON-Schema check of a hand-supplied `--body` (cold path) parses the component schema out of the embedded spec bytes. A validation failure is a `CliError::Usage` → exit 1 with a `suggestedCommand`.

---

## 5. Transport layer

One chokepoint. Nothing else in the codebase opens a socket. `Transport` is a trait with a single blocking method so the HTTP client is swappable (§2) and so tests inject a fake:

```rust
pub trait Transport {
    fn send(&self, req: PreparedRequest, ctx: &SendCtx) -> Result<RawResponse, CliError>;
}
```

`exec.rs` sits above it and owns orchestration so command handlers stay thin: a handler returns a `Plan { op, request_spec, output_intent }`; `exec::run(plan)` selects single-shot, paginated (`--all`, §10), or streamed (§8) and feeds results to `output`. Everything below happens inside `send`/`exec`:

- **Auth injection.** The resolved credential (§8) becomes the `Authorization` header here and *only* here. `OperationDef.namespace` selects which credential (api vs service key) and which base URL (default vs `EXA_ADMIN_BASE_URL`, D4). A user `--header` can add headers but cannot override the managed `Authorization` — attempting to is a usage error, so an agent can't accidentally (or a prompt-injected agent maliciously) swap the key.
- **Idempotency-key forwarding (the other half of D7).** For an `idempotency_sensitive` op, a supplied `--idempotency-key KEY` is injected upstream **here** as an `Idempotency-Key: KEY` header (same chokepoint as `Authorization`). This — not the local flag alone — is what makes a keyed auto-retry non-double-billing: the local flag flips `keyed` in `classify` below, and the *same value* rides the wire so Exa dedups the retry server-side. (Whether Exa honors this header is a carry-over validation, contracts §7; if it does not, keyed auto-retry is disabled and the key is used only for the pending-run record.) The key value is **not** secret and is not redacted, but it is never logged with the body.
- **Wall-clock `--timeout`.** Applied here via the `ureq` agent's call timeout; it bounds **each attempt**, and the `--retry` budget multiplies it. Exhaustion maps to `CliError::Network` (exit 4). It composes with the ≤250 ms SSE read-timeout (the read-timeout governs interrupt latency within a stream; `--timeout` governs total per-attempt wall-clock).
- **Retry classification (encodes D7).** `RetryPolicy::classify(op, outcome, has_idempotency_key)` is the single authority on whether to retry:

  ```rust
  fn classify(op: &OperationDef, outcome: &Outcome, keyed: bool) -> Decision {
      // create-POSTs never auto-retry unless the caller supplied --idempotency-key
      if op.idempotency_sensitive && !keyed {
          if outcome.is_ambiguous() {           // request sent, no confirmed response
              return Decision::FailAmbiguous;   // -> write pending-run record, exit non-zero
          }
          return Decision::Fail;
      }
      match outcome {
          Outcome::Network(_)              => Decision::Retry,     // exit-4 class
          Outcome::Http(429)               => Decision::RetryAfter,// honor Retry-After header
          Outcome::Http(s) if s >= 500     => Decision::Retry,
          Outcome::Http(_)                 => Decision::Fail,
          Outcome::Ok                      => Decision::Done,
      }
  }
  ```

  Idempotent GETs, network failures, 429, and 5xx retry up to `--retry N` (default 2). A create-POST without `--idempotency-key` is excluded by the first branch — the rule cannot be bypassed by a command because no command sees this decision. On `FailAmbiguous`, `exec` writes a pending-run record and sets the error's `suggestedCommand` to the exact recovery (`agent runs list --since …` or re-issue with `--idempotency-key`), per contracts §7.

  **Pending-run write contract (`state.rs`).** This breadcrumb is written at the worst possible moment — an ambiguous *billable* create — so the write is crash-safe by construction: one newline-terminated `exa.cli.pending_run.v1` JSON object appended with a single `O_APPEND` write (atomic for a sub-`PIPE_BUF` line on a local fs, so concurrent agents firing creates can't interleave torn lines), `flush`ed before the process exits. The CLI **never reads the file back** — recovery is delegated to `agent runs list` — so there is no torn-line parse risk on our side; the file is an append-only audit log bounded/rotated at N records (oldest dropped) so it can't grow unbounded. Full-file rewrites are never needed; if one were ever added it would use temp+rename+fsync (`tempfile`) per the state-and-persistence rules.
- **429 / `Retry-After`.** Parsed as integer seconds or HTTP-date (`jiff`); `--retry-after` (default on) honors it, capped by remaining `--retry` budget. Exhaustion → exit 6 `rate_limit`.
- **Redaction.** The trace writer and any error built here are wrapped by `redaction` (§10) so a leaked key is structurally impossible: `--trace FILE` records the request/response with the `Authorization` value and known secret headers/fields scrubbed to `last4`/fingerprint.
- **SSE streaming** (`stream.rs`). For `--stream` on a streaming op, the response body `Read` is parsed line-by-line into events. The `Read` uses a short read timeout (≤250 ms) so the `ctrlc`-set interrupt flag is polled promptly — SIGINT exits 12 within ~1s even when the upstream stalls mid-stream, preserving the last `eventId`. `exec` maps them to the format table in contracts §8 (`--raw` → exact SSE bytes; `--ndjson` → one `exa.cli.event.v1` per line then a terminal `exa.cli.response.v1`; `--format json` → accumulate, emit only the terminal envelope; human/TTY → progressive render to stdout, diagnostics to stderr). A broken stream → exit 12 + error envelope carrying the last observed `eventId`; `--last-event-id` resumes.
- **Outcome → envelope / error → exit.** A 2xx becomes a success `Response` envelope; a non-2xx or transport failure becomes a `CliError` whose `category()` is the exit code (§10). HTTP status lives in `error.httpStatus`; the exit code is the CLI *category*, never the raw HTTP code (contracts §6).

---

## 6. clap surface (D13)

Derive throughout. The top-level `Cli` flattens one shared `GlobalArgs` struct marked `global = true` so every subcommand inherits the universal flags without re-declaring them:

```rust
#[derive(Parser)]
#[command(name = "exa-agent", version, disable_help_subcommand = true)]
struct Cli {
    #[command(flatten)]
    globals: GlobalArgs,
    #[command(subcommand)]
    command: Command,
}

#[derive(Args)]
struct GlobalArgs {
    #[arg(long, global = true, value_enum)] format: Option<Format>,   // human|json|ndjson
    #[arg(long, global = true)]             json: bool,               // alias: --format json
    #[arg(long, global = true)]             ndjson: bool,             // alias: --format ndjson
    #[arg(long, global = true)]             raw: bool,                // bypass envelope (contracts §2)
    #[arg(long, global = true, conflicts_with = "compact")] pretty: bool,
    #[arg(long, global = true)]             compact: bool,
    #[arg(long, global = true, env = "EXA_API_KEY")] api_key: Option<String>, // one-shot, never persisted
    #[arg(long, global = true, env = "EXA_PROFILE")] profile: Option<String>,
    #[arg(long, global = true)]             base_url: Option<String>,
    #[arg(long = "header", global = true)]  headers: Vec<String>,
    #[arg(long, global = true)]             timeout: Option<humantime::Duration>,
    #[arg(long, global = true, default_value_t = 2)] retry: u32,
    #[arg(long, global = true)]             output: Option<PathBuf>,  // -o/--output (§7, D10)
    #[arg(long, global = true)]             trace: Option<PathBuf>,
    #[arg(long, global = true)]             no_color: bool,
    #[arg(long, global = true)]             yes: bool,
    #[arg(long, global = true)]             dry_run: bool,
    #[arg(long, global = true)]             print_request: bool,
    // ... cursor/limit/all/max-pages/page-delay, idempotency-key, last-event-id
}

#[derive(Subcommand)]
enum Command {
    Search(SearchArgs),
    Contents(ContentsArgs),
    Answer(AnswerArgs),
    Agent { #[command(subcommand)] sub: AgentCmd },     // nested groups
    Websets { #[command(subcommand)] sub: WebsetsCmd },
    Admin { #[command(subcommand)] sub: AdminCmd },      // gated namespace, D4
    Capabilities(CapabilitiesArgs),
    Schema { #[command(subcommand)] sub: SchemaCmd },
    Doctor(DoctorArgs),
    Raw(RawArgs),
    // ...
}
```

- **`ValueEnum` for every fixed-set flag** (`--format`, `--type`, `--data-source`, `--livecrawl`, `--effort`, `--input-format`, …), each with **`ignore_case = true`** so `--type Fast`/`--format JSON`/`--effort Medium` resolve to the canonical lowercase spelling. This makes an invalid choice self-documenting — clap's error lists the valid values — and gives the registry consistency test a typed surface to check enum membership against `FieldDef::Enum`.
- **Input forgiveness lives in the parser/normalization layer** (design-principle "Input forgiveness"), so the rest of the program sees only canonical values where the API defines them: bare `--text` normalizes to `true`, while `--text N` and `--text full` normalize to `text.maxCharacters` or uncapped text; `--text false`, `--text true`, and `--text 0` reject instead of acting like booleans; `--highlights[=N]` normalizes to a `contents.highlights` options object; fixed enums use `ValueEnum` with `ignore_case = true`; `--category` canonicalizes known suggestions case-insensitively but preserves arbitrary non-empty custom hints; retired `research paper` spellings reject with `didYouMean=publication`; a custom positional parser flags placeholder literals (`<id>`, `$VAR`, `YOUR_*`, `…`) as `CliError::Usage`/`placeholder_argument` (exit 1) naming the discovery step rather than forwarding the literal upstream. Opaque Exa ids get no prefix-coercion (documented non-feature). Coercion is total and deterministic — unambiguous inputs normalize, genuinely ambiguous ones reject with options.
- **Arg-conflict rules are local validation** (exit 1, no network). `conflicts_with` / `conflicts_with_all` / `required_unless_present` encode the static rules (positional URLS vs `--ids`, `--pretty` vs `--compact`); they fail before any request, which is the cheapest possible feedback loop. The dynamic ones that depend on the registry (`--all` on a non-cursor op, contracts §10) are checked in `exec` because clap can't see the operation's pagination style.
- **The suggestion engine *is* the intent-inference axiom — but its output is captured, not leaked.** clap's built-in did-you-mean (on by default) handles `exa-agent serch` → "did you mean `search`?" and `--num-reslts` → "did you mean `--num-results`?" with zero code. We do not hand-roll a suggestion table. But clap prints that text itself and exits **2** by default — which would violate both the stdout/stderr-JSON discipline and the exit dictionary (2 = auth). So `lib.rs::run` parses with **`Cli::try_parse_from`** and intercepts the `clap::Error`: `DisplayHelp`/`DisplayVersion` pass through to stdout at exit 0; every other `ErrorKind` is converted to a `CliError::Usage` (exit **1**), rendered as `exa.cli.error.v1` on stderr, with clap's `error.kind()` → our `error.code` (`UnknownArgument`→`unknown_flag`, `InvalidValue`→`invalid_value`, `MissingRequiredArgument`→`missing_required_argument`, etc.) and clap's nearest-candidate suggestion mirrored into `error.details.didYouMean`. clap's native exit 2 and raw text never reach the surface. Our `suggestedCommand` is then reserved for *semantic* corrections clap can't know (the dynamic ones, e.g. "use `--num-results 100`, not `--all`"). Pinned by `golden_parse_error_envelope`.
- The `--json`/`--ndjson`/`--raw` booleans plus `--format` collapse to one `OutputMode` in §7; clap only collects them.

---

## 7. Output / envelope module

**Format resolution** implements the precedence in contracts §2: explicit `--format`/`--json`/`--ndjson`/`--raw` > `EXA_OUTPUT` > auto. Auto uses `std::io::IsTerminal` on stdout (Rust std, no crate): TTY → human, non-TTY → json (D3). `--pretty`/`--compact` only toggle whitespace; default pretty in a TTY, compact when piped.

```rust
enum OutputMode { Human, Json, Ndjson, Raw }
fn resolve_mode(g: &GlobalArgs, env_output: Option<&str>, stdout_is_tty: bool) -> OutputMode { /* §2 precedence */ }
```

**The envelope serializer guarantees stable field order** by being plain `#[derive(Serialize)]` structs whose field declaration order *is* the wire order — no map, no sorting pass. The structs mirror contracts §4/§5/§8/§13 field-for-field:

```rust
#[derive(Serialize)]
struct Response {
    schema: &'static str,        // "exa.cli.response.v1"
    ok: bool,                    // true
    command: String,
    operation: OperationRef,     // method, path, operationId, source, sourceVersion
    request: RequestRef,         // requestId, upstreamRequestId, correlationId, profile, redacted
    count: Option<u64>,          // item count; populated even when data is spilled (contracts §4)
    data: serde_json::Value,     // upstream payload, unwrapped (preserve_order keeps key order)
    data_hash: Option<String>,   // #[serde(rename="dataHash")]; sha256 over `data`, change-fingerprint
    pagination: Option<Pagination>, // incl. total: Option<u64>
    cost_dollars: CostDollars,   // #[serde(rename="costDollars")]; always present, {total:0.0} default
    next_actions: Vec<NextAction>, // #[serde(rename="nextActions")]; paste-ready follow-ups (creates/lists)
    warnings: Vec<Warning>,
    diagnostics: Diagnostics,    // durationMs, retries
    data_truncated: bool,        // #[serde(rename="dataTruncated")]
    data_path: Option<String>,
    bytes: Option<u64>,
}
```

`data` holds the upstream payload *unwrapped* from any upstream envelope (e.g. `data.results`, `data.answer`), per contracts §4; `--raw` is the only mode that bypasses these structs entirely and writes upstream bytes.

**The `Sink` enforces the stdout/stderr discipline** (contracts §1) so no caller can violate it: success envelopes, NDJSON, raw bytes, and human tables go to a stdout handle; progress, warnings, retry notices, doctor prose, and error envelopes go to a stderr handle. Color/spinners live on the stderr handle and auto-disable on non-TTY, `NO_COLOR`, `CI`, `TERM=dumb`, `--no-color`, or any non-human format. There is no API to print prose to stdout; warnings are structured into `warnings[]`.

**`--output` / `--max-output-bytes` / auto-spill** (D10, contracts §9): with `-o FILE`, the full envelope (or `--raw` bytes) is written to `FILE` (atomic temp-then-`persist`, `tempfile`) and stdout gets a small confirmation envelope with `dataPath` set, `data` elided. The **default-on `--max-output-bytes` ceiling** (48 KiB) catches an unguarded `contents --text` even without `-o`: over-ceiling `data` spills to a pretty-printed JSON file in the state dir and the envelope carries `dataTruncated: true` + `dataPath`/`bytes` + a `warnings[]` note naming the ways forward. **`count` and `dataHash` are computed before the spill and survive it**, so the agent can size and verify the file without reading it. Standalone auto-spill (threshold-gated, independent of `--output`) ships conservative. Conservative content defaults (search highlights; 1500-char bare search/similar text caps) reduce how often any of it fires.

---

## 8. Auth & config

**Credential precedence (D11), resolved in `auth.rs`:** `--api-key`/`--api-key-stdin` (one-shot, never written anywhere) > `EXA_API_KEY` env > OS keyring (`exa-agent:api:<profile>`) > config metadata. The service key is the parallel chain — `--service-key` > `EXA_SERVICE_KEY` > keyring `exa-agent:service:<profile>` — and the `admin` namespace (D4) reads *only* that chain. `OperationDef.namespace` decides which chain transport pulls from, so the everyday search key can never be sent to the admin host and vice versa. The CLI refuses to use an api key where a service key is required (and the reverse) with an actionable error (`key_scope_mismatch`, exit 2), keyed off a cheap shape check on the credential.

**The two auth failures are distinct codes (contracts §5.1).** When the ladder resolves *nothing*, that is a local `not_authenticated` (exit 2) whose `details.checked` lists every rung that was tried (`["--api-key", "EXA_API_KEY", "keyring:exa-agent:api:default", "config"]`) and whose `suggestedCommand` names the exact fix (`export EXA_API_KEY=…` or `exa-agent auth login`). When a credential *was* sent but Exa rejected it (401/403 — revoked/expired), that is `reauth_required` (exit 2) — a different code so an agent branches "set a key" vs "rotate the key" instead of guessing. Both are golden-pinned.

```rust
fn resolve_credential(ns: Namespace, g: &GlobalArgs, cfg: &Config) -> Result<Secret, CliError> {
    // ns selects EXA_API_KEY / EXA_SERVICE_KEY env, keyring scope, and refusal rules
}
```

`Secret` is a newtype whose `Debug`/`Display` print only `last4`/fingerprint, so it cannot be logged in full even by accident — redaction's last line of defense. Keyring access goes through a `Keyring` trait (the same seam pattern as `Transport`), so tests inject an in-memory fake and `cargo test` never touches a real Keychain/secret-service daemon; the musl build compiles a stub impl (env-first auth only, D15). **Keyring reads are best-effort and must not block:** a read that would raise an OS prompt (e.g. macOS Keychain's allow-access dialog on first use by a new binary) fails fast and falls through to the next ladder rung — ultimately to the structured `not_authenticated` error — rather than hanging a per-call agent. This is what keeps the env-first ladder genuinely headless even on a keyring-backed profile.

**Config file (minimal per D12).** TOML at project `.exa-agent-cli.toml` then user `$XDG_CONFIG_HOME/exa-agent-cli/config.toml`. It stores base-url, default format, timeout, retry, and profile *metadata and env-var names* — **never plaintext keys** (D11). `--profile X` selects a `[profiles.X]` block; `EXA_PROFILE` is the env default.

```toml
base_url = "https://api.exa.ai"
format   = "json"
timeout  = "30s"
retry    = 2

[profiles.work]
base_url    = "https://api.exa.ai"
api_key_env = "EXA_API_KEY_WORK"   # name of an env var, not a key
```

The full preset/macro registry (`preset show`, presets-in-TOML) is deferred (D12); v1 ships only the two thin macros `ask` and `fetch`, which expand to canonical commands and return `expands_to` in their JSON so an agent learns the underlying command.

---

## 9. Doctor (D8)

Read-only, offline by default; network checks behind `--online`. **No `--fix`, no `undo`, no backups, no `mutate()` chokepoint** — this is a near-stateless API client whose broken states are diagnose-and-suggest, not mutate-and-undo. Each detector returns a `Finding` whose `suggestedCommand` mirrors the error envelope's, so the fix for a diagnosis is always one copy-pasteable line.

```rust
trait Detector { fn check(&self, ctx: &DoctorCtx) -> Finding; }   // never mutates anything
struct Finding { id: &'static str, status: Status /* Ok|Warn|Fail */, message: String, suggested_command: Option<String> }
```

Detector list (all read-only):

| id | checks | example suggestedCommand |
|---|---|---|
| `config.parse` | config TOML parses; profile exists | `exa-agent config path` |
| `key.present` | api key resolvable via precedence (presence, not validity) | `export EXA_API_KEY=…` |
| `service-key.scope` | service key, if configured, isn't an api key (shape) | `export EXA_SERVICE_KEY=…` (must be a service key, not an `EXA_API_KEY`) |
| `base-url` | base-url is a well-formed absolute https URL | `exa-agent config set base_url …` |
| `spec.hash` | embedded-spec SHA matches the committed snapshot | `exa-agent schema refresh --check` |
| `binary.version` | reports version + embedded spec version | — |
| `tty.discipline` | self-check: non-TTY run emits clean JSON, no ANSI on stdout | — |
| `auth.online` | *(`--online` only)* a billing-free auth probe succeeds — `POST /search` with an empty body (auth is validated before the body, so a good key returns 400 `INVALID_REQUEST_BODY` and a bad one 401/403; 5xx/429 → inconclusive/warn). Not a GET to `/v0/teams/me`, which upstream does not serve (see D22). | `exa-agent auth test` |
| `connectivity` | *(`--online` only)* base-url reachable, TLS valid | `exa-agent doctor --online` |

`doctor --json` emits an `exa.cli.doctor.v1` report (contracts §15): the findings array in stable order, each finding keeping its own `category` for granularity. The **exit code is doctor-local, not the §6 categories** — `0` healthy (no `Fail`), `1` findings present, `4` refused-unsafe (a detector that couldn't safely complete) — so a `doctor` exit can never be mistaken for a real `auth`(2)/`config`(3)/`network`(4) failure from another command (the SKILL's documented doctor exception). The detector ids and this exit dictionary are published in `capabilities.doctor` (contracts §13), so an agent can discover what `doctor` checks without running it. Every `Fail`/`Warn` finding MUST set `suggested_command` (the `service-key.scope` detector's fix is `export EXA_SERVICE_KEY=…` / the cross-use error pointer — no finding is left with a bare `—`). **Upgrade-path boundary (written down so it isn't over-built):** the *only* legitimate future `--fix` target is config-file rewrites — and adopting `--fix` means adopting the full chokepoint + backup + `undo` discipline at that point, not before.

---

## 10. Error model

One `thiserror` enum is the single source of both the exit code and the error envelope. Each variant maps to exactly one category in the contracts §6 dictionary; `category()` returns the exit code, `into_error_envelope()` produces `exa.cli.error.v1`.

```rust
#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error("{0}")] Usage(Diag),       // 1
    #[error("{0}")] Auth(Diag),        // 2
    #[error("{0}")] Config(Diag),      // 3
    #[error("{0}")] Network(Diag),     // 4
    #[error("{0}")] Upstream(Diag),    // 5
    #[error("{0}")] RateLimit(Diag),   // 6
    #[error("{0}")] NotFound(Diag),    // 7
    #[error("{0}")] Conflict(Diag),    // 8
    #[error("{0}")] Safety(Diag),      // 9
    #[error("{0}")] Partial(Diag),     // 10
    #[error("{0}")] NoInput(Diag),     // 11
    #[error("{0}")] Interrupted(Diag), // 12
}

impl CliError {
    fn category(&self) -> u8 { /* 1..=12 per contracts §6 */ }
}
```

`Diag` carries the contract fields: `code` (a stable machine string from the published §5.1 dictionary, not free-form), `message` (one line, names what failed and where), `details` (incl. the optional `didYouMean`/`checked`/`retryAfterMs`), `see`, `http_status`, `upstream_request_id`, `retryable`, and `suggested_command`. `retryable` is computed from the same `RetryPolicy` logic transport uses (so the envelope's fine signal and transport's behavior never disagree) and is `false` for every un-keyed create failure (D7). `lib.rs::run()` is the single funnel: it (1) intercepts `clap::Error` from `try_parse_from` — `DisplayHelp`/`DisplayVersion` → stdout, exit 0; any other kind → `CliError::Usage` (exit 1) so clap's default exit 2 never leaks (§6) — and (2) is the only place that converts a `CliError` into bytes, serializing the envelope to **stderr** (stdout stays empty, contracts §5) and returning `category()` as the process exit code. A static test asserts every `error.code` the binary can emit is a member of the §5.1 dictionary.

**Redaction sits at the serialization boundary, not at each call site**, which is what makes "no error can leak a key" structural rather than a discipline we hope holds. Every `Diag` field — including `suggested_command` and `details` — passes through `redaction::scrub` on its way into the envelope, and the `Secret` newtype (§8) can't render in full anywhere. The scrub is a targeted pass: replace any value matching the Exa key prefix shape with `last4`, and blank known secret headers (`Authorization`, webhook-signing headers) by name. Because it runs at one boundary, adding a new error variant can't reintroduce a leak.

---

## Open seams

**Resolved by the coordinator (2026-06-29) — see `decisions.md` Addenda (D14–D21):** (1) `ureq` confirmed (D14). (2) `overlay.toml` accepted (D17). (3) `preserve_order` reading confirmed — "stable" = deterministic insertion order, not sorted (contracts §12). (4) `diagnostics.cache` removed from the schema entirely (contracts §4). (5) "stateless" clarified — config/keyring/pending-run/trace/spill writes are sanctioned, none is a cache (D19). (6) `--header` cannot override managed auth headers — confirmed (D18). (7) spec vendored as normalized JSON (D21). Originals retained for traceability:

Flagged for the coordinator — none silently resolved:

1. **HTTP client divergence from the brief.** The brief suggested blocking `reqwest`; I chose `ureq` (§2) because `reqwest::blocking` embeds an internal tokio runtime, contradicting the spirit of "no tokio." This is a defensible call the brief invited ("unless you justify otherwise") and is isolated behind the `Transport` trait, but it *is* a divergence from the brief's stated default — confirm or override.
2. **Registry needs a curated overlay, not just OpenAPI.** D9 says the registry is "generated from the embedded Exa OpenAPI snapshot," and the literal reading is OpenAPI-only. But `dangerous`, `idempotency_sensitive`, cursor field names, and the CLI command path are not expressible in vanilla OpenAPI. §3 introduces `openapi/overlay.toml` as a second committed build input. This doesn't contradict D9's *intent* (the registry still carries that metadata) but it does add an input D9 doesn't name — confirm the overlay is acceptable.
3. **`serde_json` `preserve_order` (indexmap) vs "stable ordering."** Contracts §12 wants stable key ordering for the CLI's own output. I read "stable" as "deterministic given identical input," achieved by preserving upstream insertion order (not alphabetizing). If §12 intended *sorted* keys, that's a one-line feature change — confirm the reading.
4. **`diagnostics.cache` is vestigial.** The contracts §4 envelope includes `diagnostics.cache`, but D5 forbids a cache, so it's always `null`. Harmless and forward-compatible, but worth noting the field exists only to satisfy the frozen schema.
5. **Disk touches under a "stateless" client.** D5 says no hidden local state, but pending-run JSONL (D7), the config file, the OS keyring, auto-spill temp files, and `--trace` all touch disk. These are all explicitly sanctioned by other decisions; calling it out so "stateless" isn't read as "never writes disk." None is a cache.
6. **`--header` vs managed `Authorization`.** §5 forbids `--header` from overriding the managed `Authorization` (a prompt-injection / footgun guard). No decision speaks to this directly; I made the safe call. Confirm it's the desired posture (vs. letting power users override).
