# Refactor: finish the registry-driven dispatch (the "full vision")

Date: 2026-06-30
Status: **design proposal — not yet approved.** Decides whether, and how, to converge the as-built dispatch onto the generic registry-driven architecture that [`architecture.md`](architecture.md) §1/§5 already describes. Companion to [`architecture.md`](architecture.md); where they disagree, this doc is the newer thinking and supersedes for the dispatch layer only.

---

## 0. TL;DR

The architecture doc's vision — *"dispatch is generic and registry-driven; there is no per-command handler layer; the typed `cli/<group>.rs` structs only collect flags"* (§1, §5) — is **already built for 7 of 68 operations and abandoned for the other 61**, which fell back to hand-written body-builders, validators, and in three cases whole duplicate execution functions. The result is `src/lib.rs` at **7,435 lines**: 104 `dispatch_*` handlers, 24 `build_*_spec`/`*_named_body` functions, 19 `validate_*` functions.

This is not because the Exa surface is irreducibly heterogeneous. The 61 hand-coded commands bypassed the registry for **~3–4 specific, bounded reasons** the `FieldDef` model can't yet express (constant discriminator co-fields, scalar→object array templates, a couple of capability behaviors). Add those to the field/operation metadata and **most of the 61 collapse into data**, the way the 7 already are.

**The proposal:** make the registry the single source of the entire command surface — clap args, body assembly, validation, and special behaviors all derived from one enriched table — so that the whole pipeline runs once for every command, and adding an Exa endpoint is a registry entry, not a handler. Migrate via strangler-fig (never a broken tree). Prove correctness with property tests *over the registry*, not example tests per handler.

**Why it's worth it (the evidence, not the elegance):** every bug we fixed in the last release-hardening pass was the same disease — a per-command path that forgot a cross-cutting concern or drifted from the registry. Response redaction missing on one path; `auth test` firing live under `--dry-run`; the placeholder guard needing manual insertion at every id-taking dispatcher; the registry/exit-code contradiction; `validate-input` disagreeing with live validation. A data-driven pipeline makes that entire bug class *structurally impossible*, for every command including ones Exa hasn't shipped yet. That is the actual product thesis ("complete, faithful, always-current coverage of the full Exa surface") expressed as an architecture.

---

## 0.5 Post-fusion revision (2026-06-30)

This doc was pressure-tested through a cross-vendor council (Codex/OpenAI, Gemini/Google, Kimi/Moonshot proposing; xAI/grok judging). **Verdict: GO with changes — medium-high confidence. Do NOT downgrade to the pragmatic extraction** (it leaves the confirm-gate/secret-capture bug class in place). The council ratified the core bet (one pipeline for 100% of commands; Option B for clap now; §2.6's pipeline-vs-declarative separation) and corrected the mechanics. The validated revisions, in priority order:

**R1 — Resequence so the bug-class kill comes first, overlay expansion last (highest-value change).** The original §6 buried the cheap wins behind 61 rows of overlay curation. New order:
1. Land `exec::run(Plan)` with **engine-enforced confirm-gate from `op.destructive()`** (closes the live `websets_searches_cancel` latent bug) and the **`SecretCapture` stage** (deletes the 3 duplicate `*_create_live` fns).
2. **Wire the *already-written* `validate_registry_input` into the live path** (it exists; it's only connected to `schema validate-input` today — §2.4). Kills the validator-drift bug class with near-zero new code.
3. **Freeze a pre-migration `--print-request` golden corpus for all 68 ops** (see R2) as the independent oracle.
4. *Then* implement the `request.rs` enrichments (R4) and populate overlay `fields` group-by-group.
   This delivers the documented bug fixes without betting the schedule on hand-curated TOML or the proc-macro story.

**R2 — The truth-oracle needs an *independent* second witness; canonical examples alone are insufficient.** A wrong `body_path` in the overlay plus a matching wrong expected-body in the example corpus (same author, same mistake) passes every self-referential property test → silent staging 400. Fix: (a) freeze the **current** `--print-request` output per op *before* migration (derived from today's live handlers, not the overlay) and diff each migrated group against it; (b) cross-check assembled bodies against the **embedded OpenAPI `requestBody` schema** (already in the binary via `EMBEDDED_SPEC_SHA256`) for a second witness on body *shape*. Overlay-only facts (`dangerous`, `cli_path`, `SecretCapture.required`) still need examples, but body shape gets two independent checks.

**R3 — The `co_fields`/`item_template`/`enum_values`/`range` enrichments are not yet implemented in `request.rs`.** `build_flag_body` today applies only `flag`/`body_path`/`kind`/`required`. Until the request builder grows these, G1/G2 are *not* "data-only" — the first migrated group hits a per-group **behavior** big-bang when empty `fields` suddenly populate. So: **implement the request-builder enrichments and test them before populating any overlay `fields`.** This is the real week-one gate, more than Option A-vs-B.

**R4 — Honest correction on `IntoFlagValues`.** The claim that the proc-macro "also emits enum/range metadata from clap attributes, so the validator gets it without a second copy" does **not** hold: the macro runs on `cli/*.rs` structs; `validate_registry_input` reads `OperationDef.fields` from `REGISTRY` (a `build.rs`/overlay output) — different compilation pipelines, no closed loop. The real options are (a) `enum_values`/`range` live in **both** `#[arg(...)]` and overlay with a flag-name consistency test (note: that test catches name drift, **not** enum-member drift — the exact bug relocated), or (b) the macro emits a side table `operation_id → &[FlagConstraint]` and the validator is rewritten to consult it (a new parallel registry, not "no copy"). Pick (b) and own it as a real artifact, or accept (a)'s residual drift risk explicitly. No magic.

**R5 — The `body_builders ≤ 16` canary starts *at* the ceiling, not below it** (16 G5 handlers exist day one), so as written it freezes architecture instead of giving headroom. Fix: separate **budget** (target ~12, hard max ~20) from **inventory** (count today); grandfather existing IDs on an allowlist; require explicit *decommission* as declarative migration removes builders. Same fix for `validators ≤ 8`.

**R6 — FieldDef scope is three categories, not one** (resolves the §2.1 over-reach): **body-construction** (`co_fields`, `item_template`) lives in `FieldDef` *independent* of the clap choice; **clap-presentation** (`help`, `value_name`, `conflicts_with`, `positional`) stays in `#[arg(...)]` derive attributes under Option B — do **not** duplicate into the registry; **validation** (`enum_values`, `range`) is shared per R4. G5 → a named Rust builder, never a TOML `group_defaults` primitive.

**R7 — Body builders take a uniform input, not `fn(&SearchArgs)`.** There is no single `Args` type (64 distinct structs), so a registry-table builder registered by `BuilderId` must accept the stringly `&[(&str, Option<String>)]` slice (or a `Value`) — not a concrete arg struct. Purity (no I/O/auth/transport/redaction) is a convention + review rule + test hook, not statically enforceable; say so.

**R8 — Day-one cross-cutting gaps to assign explicitly** (each a silent regression if unowned): `raw` must be **carved out** of `exec::run` (no `OperationDef`, no confirm, no placeholder guard — by design); the `Macro` (`ask`/`fetch`) re-entrancy contract must define whether the child plan inherits globals/dry-run/confirm; `QueryFromBody` must run **after** `--body`/`--set` merge; the `FieldKind::Bool` coercion (`request.rs` treats any non-`true/1/yes/on` as `false`) must agree with whatever the declarative validator does with a bad bool; placeholder-guard order (parse-time vs post-build) must be fixed since it affects `IntoFlagValues` ordering; pending-run-on-ambiguous-create must stay on the list when folding `execute_typed_live`; the ~8 bespoke residue commands need a typed dispatch enum so nobody routes them through registry lookup during migration.

**Dropped:** the LRO/multi-request concern (ungrounded for this `ureq`/no-async/stateless-per-call binary; agent drives polling via `agent runs list`).

**Two forks surfaced, with recommendations:**
- *Clap Option A as eventual target vs Option B possibly-permanent.* Ship **B now** (unanimous; unblocks every migration step, deletes the §6 step-5 big-bang). Whether A is ever the target turns on a fact nobody can settle yet — can a parity harness reproduce the golden-pinned agent-facing parse/error contract — so it's a **later gated spike, not a now-decision**. Honest cost of B, conceded: the flag surface stays maintained in two places; the consistency test *detects* drift but does not *eliminate* the tax.
- *FieldDef-shape (`co_fields`/`item_template` in registry vs push G1/G2 to builders).* Recommend **keep them in the registry** (majority + the builder-count math: pushing G1/G2 to builders blows the ≤16 canary) — but per R3, implement them in `request.rs` before claiming the declarative win.

The sections below are the original proposal; where R1–R8 amend them, the revision wins.

---

## 1. Diagnosis: what is actually built

### 1.1 The generic engine already exists

Dispatch already funnels through a shared typed engine:

```
dispatch_typed_command              (lib.rs:4676)
  → dispatch_typed_command_with_options   (4726)
    → dispatch_typed_inner                (4759)   # dry-run preview, cred resolve, timeout
      → execute_typed_live                (5159)   # single request → redact → envelope
         ├ execute_paginated_live         (4866)   # --all cursor loop
         ├ execute_streaming_live         (5280)   # SSE
         └ dispatch_typed_chunks          (5063)   # batched requests
```

`execute_typed_live` already centralizes the cross-cutting concerns the contracts care about: `--dry-run`/`--print-request` short-circuit (4768), credential resolution (4793), idempotency-sensitive redaction (5248), pending-run-on-ambiguous-create (5223), raw passthrough (5233), ndjson vs envelope (5272). For the commands that use it, this is exactly the architecture doc's `exec::run(Plan)`.

### 1.2 …but only 7 of 68 operations actually use the registry to build their body

The generated registry (`$OUT_DIR/registry.rs`) has **68 `OperationDef`s; only 7 carry a non-empty `fields: &[FieldDef]` array.** The other 61 have `fields: &[]`. For those, `request::build_flag_body` (request.rs:99) produces an empty object, and the real flag→body mapping lives in a hand-written builder in `lib.rs`:

- `build_monitor_create_named_body` (1862)
- `build_websets_create_named_body` (2490)
- `build_websets_monitors_create_named_body` (3446)
- `build_websets_webhooks_create_named_body` (3916)
- …24 `build_*` functions total.

So the "generic registry-driven dispatch" is **half-implemented**: the simple core commands (`search`, `answer`, `contents`, `context`, `similar`, `agent runs create`) are registry-field-driven; everything in `monitor`/`websets`/`admin` is hand-coded. The 104 handlers, 24 builders, and 19 validators are the cost of that abandonment.

### 1.3 Why the 61 bypassed the registry — the real `FieldDef` gaps

Reading the hand-written builders, the bypass reasons are specific and bounded, not "the domain is too weird." The classification pass (§7) found **16 handlers** that hand-roll bodies via private `build_*_named_body` helpers. Their reasons:

| # | Gap | Example (lib.rs) | Expressible today? |
|---|-----|------------------|--------------------|
| G1 | **Constant discriminator co-field** | `--schedule X` → `trigger: {type:"interval", period:X}` — must set a *constant* sibling `type` alongside the value (1870) | No |
| G2 | **Scalar→object array template** | `--criteria "d"` (repeatable) → `[{description:"d"}]` — each scalar wraps into a fixed-key object (3477) | No (StrArray gives `["d"]`) |
| G3 | **Discriminated wrapper object** | `--query` → `search:{query:X}`; `--webhook-url` → `webhook:{url:X}` (1867,1876) | **Yes** — nested `body_path` (`search.query`) already does this; these were hand-written needlessly |
| G4 | **Conditional parent emission** | emit `cadence:{...}` only if `cron` or `timezone` present (3454) | **Yes** — nested `body_path` only writes present leaves |
| G5 | **Group-level default co-field** | `behavior.config.type` auto-defaults to `"search"` whenever *any* behavior field is present (3446) | No — the constant is tied to a *group* of fields being present, not one field |

G3 and G4 are *already* expressible — those builders are pure tech debt. **G1, G2, G5 are genuine model gaps.** G1/G2 are small field-level enrichments. **G5 is the dangerous one** — a default tied to "any of a group is present" is the first step toward a conditional-construction mini-DSL in TOML, and it's the canary for §5.1's expressivity-ceiling risk. The classification pass flagged this exact 16-handler cluster as "a real architectural gap, not laziness." It is ~15% of all handlers — right at the go/no-go threshold (§7). **This is the make-or-break of the whole bet, and §2.6 resolves it by *not* requiring 100% declarative coverage.**

### 1.4 The duplication and leak smells (the strongest evidence)

- **Three ~80-line near-duplicate execution functions** exist *only* because secret-capture must intercept the response before redaction: `dispatch_monitor_create_live` (1939), `dispatch_admin_keys_create_live` (958), `dispatch_websets_webhooks_create_live` (3990). Each re-implements credential resolution, transport setup, request execution, pending-run handling, and envelope construction — a verbatim fork of `execute_typed_live` with one secret-extraction line inserted (1980). This is the canonical "a capability couldn't be expressed, so a whole parallel handler got written" failure.
- **A special case leaks *into* the generic engine as a string compare:** `let exit_code = if command == "contents" { contents_mixed_status_exit_code(&data) }` (5253). The engine reaches back out to know about one specific command.
- **Two validators that drift:** `validate_registry_input` (6055, the `schema validate-input` preflight) and the live `validate_*`/`normalize_*` path are separate code; we already shipped one drift bug between them.
- **The bug class:** `auth test` (`dispatch_auth_test`, 5772) is hand-written *outside* the engine, which is exactly why it forgot `--dry-run` until patched. The placeholder guard (`reject_placeholder_value`, 4352) had to be *sprinkled* into every id-taking dispatcher because there's no one chokepoint that sees every command.
- **A latent confirm-gate bug, live in the tree right now.** `OperationDef::destructive()` (registry/mod.rs:88) is a clean boolean, but **nothing in the engine consults it** — each handler must *remember* to call `ensure_destructive_confirmed`/`ensure_confirm_by_id`. `dispatch_websets_searches_cancel` (2960) calls neither. It is safe today *only* because the registry happens to mark that one op `dangerous:false` while its siblings (`websets cancel`, `websets enrichments cancel`) are `dangerous:true`. There is **no test tying `op.destructive()` to an actual gate call** — so the day someone flips that op's `dangerous` in the overlay (or adds a new destructive op) with no code change, it ships unguarded and silent. This is precisely the bug the engine-enforced confirm-gate stage (§2.3) makes impossible, and it's a concrete reason to do this now rather than "someday." Three distinct confirm protocols exist (`--yes`; `--confirm <id>` echo-the-id; `--yes` + `--confirm delete`), so the gate stage must model all three or deliberately unify them.

---

## 2. Target architecture

One principle: **the registry is the single source of the entire command surface.** clap args, body assembly, validation, blast-radius metadata, and special behaviors all derive from one enriched table. Every command flows through one pipeline. The only hand-written Rust is (a) the table's enrichments in `overlay.toml`, (b) a small closed set of capability implementations, and (c) the ~8 commands that aren't Exa API calls at all.

### 2.1 Enriched registry / overlay schema

Extend `FieldDef` and `OperationDef` (and the `overlay.toml` that feeds them) so the 61 hand-coded commands become data. Additions, smallest set that closes the real gaps:

```rust
pub struct FieldDef {
    pub flag: &'static str,
    pub body_path: &'static str,
    pub kind: FieldKind,
    pub required: bool,
    // NEW — declarative enrichments (all default-empty, backward compatible):
    pub co_fields: &'static [(&'static str, ConstValue)], // G1: constant siblings, e.g. ("trigger.type","interval")
    pub item_template: Option<&'static str>,              // G2: StrArray item wrap key, e.g. "description"
    pub enum_values: &'static [&'static str],             // declarative enum membership (today: hand-coded)
    pub range: Option<(f64, f64)>,                        // declarative numeric range (today: hand-coded)
    pub help: &'static str,                               // clap help text (today: in the cli struct)
    pub value_name: &'static str,                         // clap value placeholder
    pub conflicts_with: &'static [&'static str],          // clap arg conflicts (today: #[arg(conflicts_with)])
    pub positional: bool,                                 // positional vs --flag
}

pub struct OperationDef {
    // …existing fields…
    // NEW — typed capabilities (closed enum; each has ONE impl in exec):
    pub capabilities: &'static [Capability],
    pub validators:   &'static [ValidatorId],             // named semantic-validator escape hatch (§2.4)
    pub mixed_status_exit: bool,                          // replaces the `command=="contents"` string compare
}

pub enum Capability {
    SecretCapture { response_field: &'static str, output_flag: &'static str, required: bool }, // kills the 3 dup create-live fns; `required` captures the admin-keys(mandatory) vs monitor/webhook(optional) split (§7.D7)
    Chunked       { input_fields: &'static [&'static str], max: u32 },          // contents batching
    Macro         { expands_to: &'static str },                                 // ask/fetch
    Confirm       { protocol: ConfirmProtocol },                                // Yes | EchoId | YesPlusEcho(&'static str) — the 3 protocols in §7.D6; engine-enforced from op.destructive()
    QueryFromBody { rules: &'static [(&'static str, &'static str)] },           // websets-preview `?search=true` derivation (§7.D8) — small, but real
    // pagination / streaming / idempotency_sensitive stay as today's typed fields
}
```

The overlay already proves this works: it's a committed, reviewed TOML keyed by operationId that today carries `cli_path`, `namespace`, `idempotency_sensitive`, `dangerous`, `streaming`, `cursor`, and `[[operations.X.fields]]` for the 7 working commands. The proposal is to populate `fields` for the other 61 plus the new enrichment keys. `build.rs` already merges and emits a byte-reproducible table; the codegen change is mechanical.

### 2.2 The generated clap surface (THE central design fork)

Today every command has a hand-written clap `Args` struct (`SearchArgs`, `MonitorCreateArgs`, … 64 of them in `cli/mod.rs`), and a hand-written `build_*_spec` that unpacks `args.query`, `args.num_results`, … into the stringly `[(flag, Option<String>)]` array the engine consumes. **The flag set is therefore maintained twice** — once as `FieldDef` (spec/overlay) and once as a clap struct — and the unpacking glue is pure boilerplate. The vision's "structs only collect flags" requires eliminating that second copy. Two ways:

- **Option A — dynamic clap from the registry (builder API).** Build the clap `Command` tree at startup from `REGISTRY` (each `FieldDef` → `clap::Arg`), and read parsed values generically by iterating `op.fields` and pulling each flag as a string. **Zero hand-written arg structs for uniform commands.** Add an endpoint = registry entry, full stop. Cost: lose derive ergonomics; the registry must carry the metadata derive gives clap for free (help, value names, enum members, conflicts); must verify clap's builder API reproduces derive's did-you-mean/help quality.
- **Option B — keep derive, add a `#[derive(IntoFlagValues)]` proc-macro** that generates the unpacking from each struct. Keeps typed structs and derive ergonomics; removes the `build_*_spec` boilerplate but *not* the double-maintenance of the flag set. Smaller blast radius, smaller payoff.

**Recommendation: Option A.** The typed structs buy almost nothing here — every value is stringified at the `build_typed_spec` boundary anyway (request.rs takes `&[(&str, Option<String>)]`), so the compile-time typing is shallow. Option A is the only one that delivers the product thesis ("new endpoint = data"). Option B is the fallback if clap's builder API can't match derive's error quality. **This fork is the #1 thing to pressure-test in review.**

### 2.3 The one pipeline: `exec::run(Plan)`

Formalize the existing engine into one stage list every command flows through, with the *pre-dispatch* concerns folded in so no command can skip them:

```
build clap surface (from registry)
  → parse
  → resolve OperationDef
  → build body            (registry fields + co_fields + item_template; then --body, --set)
  → validate              (declarative: required/enum/range/conflicts  +  named validators §2.4)
  → placeholder-guard     (ONE chokepoint, not sprinkled)
  → confirm-gate          (if dangerous/destructive && !--yes)
  → preview | send        (--dry-run short-circuits here, uniformly)
  → [single | paginate | stream | chunk]   (selected by capability/pagination/streaming data)
  → secret-capture        (if Capability::SecretCapture — intercept before redact)
  → redact → envelope → sink
```

This deletes the three duplicate `*_create_live` functions (secret-capture becomes a pipeline stage), the `command=="contents"` compare (becomes `op.mixed_status_exit`), and the sprinkled placeholder guard (becomes one stage). `auth test`-class bypass bugs become impossible because there is no bypass.

### 2.4 Validation: one path, declarative + a small named-validator hatch

Most validation is declarative and belongs in `FieldDef`: required-field presence, enum membership (`enum_values`), numeric range (`range`), arg conflicts (`conflicts_with`). One function evaluates these against any op's body — and **both live dispatch and `schema validate-input` call it**, so they cannot drift (kills that bug class).

**This is the cheapest win in the whole proposal: the declarative validator is already written.** `validate_registry_input` (6055) + `validate_field_kind` (6145) + `validate_enum_field` (6200) already evaluate `FieldDef.kind`/`.required`/enum against an arbitrary body — but they are wired *only* into the offline `schema validate-input` command (#99 in §7) and **never run on the live dispatch path.** All 36 VALIDATION handlers reimplement required/type/enum by hand. Step one of the validation work is literally "call the function that already exists, from the pipeline." That alone collapses a chunk of the 36.

Genuinely *semantic* checks that aren't declarative get a **small, closed, named escape hatch**: `OperationDef.validators: &[ValidatorId]`, each id mapping to one fn in a validator table. Candidates from the current 19: search category alias-canonicalization (466), admin usage date-window math (1258), linkedin-domain heuristic (619), monitor batch shape (2307), websets import body shape (3277), context query length (814). **The discipline: this table stays ≤ ~8 named validators.** If it grows past that, the declarative model is leaking and we've mis-drawn the line — that's the canary, written down so we notice.

### 2.5 Bespoke residue (stays hand-written, ~8 functions)

Commands that are not uniform Exa API calls keep small explicit handlers, dispatched directly: `capabilities`, `schema`, `robot-docs`, `doctor`, `config`, `auth` (login/logout/status/test), and the `ask`/`fetch` macros (which already just build another command's spec and re-enter the engine with an `expands_to` annotation — 747, 762). These are genuinely irreducible; they are ~8 functions, not 104.

Two members of this set need a deliberate call: **`raw`** (6606) is the intentional escape hatch — `op = None`, no registry presence, no confirm-gate, by design; it stays bespoke and that is correct. **`auth test`** (5772) is the genuine awkward case: it makes a live call but emits a custom `exa.cli.auth_test.v1` envelope outside the engine. Options: fold it into the pipeline as a tiny synthetic op (losing the custom envelope), or carve a narrow `Probe` capability. Lean toward the former — one fewer bespoke path is worth more than a bespoke envelope shape.

### 2.6 The crux: separate "one pipeline" from "100% declarative bodies"

The go/no-go tension (§7) dissolves once you see these are **two independent goals**, not one:

1. **One pipeline for every command** (`exec::run`, §2.3) — every command, including the 16 conditional-body ones, flows through the same stages and therefore gets dry-run, redaction, placeholder-guard, confirm-gate, and pending-run *by construction*. **This is the prize. It is fully achievable and it kills the bug class.** A handler that still hand-builds its body can *still* return a `Plan` and ride the pipeline — the body-builder shrinks to one function whose only job is JSON assembly, with every cross-cutting concern handled upstream.

2. **Declarative body construction from the registry** (§2.1, §2.2) — eliminating hand-written builders by pushing fields into the overlay. **This is a dial, not a switch.** G1/G2/G5 enrichments push it from 7/68 to ~52/68. The remaining ~16 conditional-body commands keep a *thin named builder* (`fn build_websets_monitor_body(args) -> Value`) — declared in the registry as `body_builder: BuilderId` — instead of a baroque TOML encoding. **We stop pushing declarative coverage exactly where the TOML stops being clearer than the Rust.**

So the recommendation is not "make all 68 commands pure data." It is: **route 100% of commands through one pipeline; make ~75% of bodies declarative; let the conditional-body tail keep a thin named builder that still rides the pipeline.** That captures essentially all the bug-class-killing value while refusing the mini-DSL trap. The `validators ≤ 8` and `body_builders ≤ ~16` canaries (named, counted, tested) keep the escape hatches honest.

---

## 3. What this deletes

Concrete, not abstract:

- **3 duplicate execution functions** (~240 lines) → one `SecretCapture` stage.
- **24 `build_*_spec`/`*_named_body` functions** → registry data + the generic builder.
- **~61 empty-`fields` operations** become real registry entries; the half-built model becomes whole.
- **64 hand-written clap arg structs** → generated surface (Option A) or thin unpack macro (Option B).
- **19 `validate_*` functions** → one declarative evaluator + ≤8 named semantic validators.
- **The `command=="contents"` leak** → `op.mixed_status_exit` data.
- **The two-validator drift** → one validation path shared by preflight and live.
- **`src/lib.rs`: 7,435 → an estimated ~1,500 lines**, most of it the bespoke residue + the pipeline.

---

## 4. Testing: properties over the registry

Replace example-per-handler tests with **properties quantified over `REGISTRY`** — they test all ops at once, including future ones, and catch precisely the bug class we hand-found:

- For **every** op: `--dry-run`/`--print-request` performs zero network I/O.
- For **every** `idempotency_sensitive` op: no auto-retry without `--idempotency-key` (today's `no-retry matrix` test, generalized).
- For **every** `SecretCapture` op: the captured secret never appears in stdout/the envelope.
- For **every** `dangerous`/destructive op: dispatch refuses without `--yes`/`--confirm`.
- For **every** op: `cli_path` round-trips — the generated clap surface parses the op's own canonical example, and `capabilities` agrees with dispatch behavior (no registry/behavior drift).
- For **every** op with `enum_values`/`range`: `validate-input` and live dispatch return identical verdicts on the same body.

Keep insta goldens for the frozen envelopes (contracts §14). The point: correctness becomes a property of the table + pipeline, not a thing each of 104 handlers must individually remember.

---

## 5. The hard parts (honest)

1. **Metadata expressivity ceiling.** Every enrichment (G1, G2, co-fields, item templates) is a step toward a mini-DSL in TOML. There is a point where a declarative encoding is *less* readable than a 10-line Rust builder. The design must draw that line explicitly and refuse to cross it — the `validators ≤ 8` canary (§2.4) and a "if a field needs more than co-field + template, it's a named builder" rule. **This is the real risk and the main thing to get a second opinion on.**
2. **clap parity (Option A).** Derive gives did-you-mean, help layout, and value-enum errors for free. The registry-driven builder must reproduce that quality or the agent-ergonomics regress. Needs a spike to confirm `clap::Command` builder + `ArgMatches` generic reads match derive's UX.
3. **Semantic validations don't all reduce to data.** Category aliasing and date-window math are real logic. The named-validator hatch handles them — but only if it stays small. If the inventory shows >~8 genuinely-semantic validators, reconsider.
4. **Error-message quality.** Today's hand-written validators emit beautiful, specific `suggestedCommand`s. The generic path must not regress these to generic messages — the registry needs enough per-field context to keep them sharp.
5. **It's a big change to a shipping, green codebase.** Mitigated entirely by sequencing (§6), but real: this is weeks, not days, if done right.

---

## 6. Migration: strangler-fig, never a broken tree

Do **not** big-bang rewrite. Build the new path alongside the old and migrate one command group at a time:

1. **Land the enriched `FieldDef`/`OperationDef`** (additive, default-empty) + `build.rs` codegen. Gate green; nothing changes behavior yet.
2. **Build `exec::run(Plan)`** as the formalized pipeline, initially wrapping the existing `execute_typed_live`. The 7 already-registry-driven commands move onto it first; prove parity with their existing tests + new property tests.
3. **Per group (search → contents → agent → monitor → websets → admin):** populate the overlay `fields` + capabilities, route the group through `exec::run`, delete its hand-written builder/validator/handler, gate green. Each group is an independent, revertible commit.
4. **Fold in the capabilities:** `SecretCapture` (deletes the 3 dup fns), `Chunked`, `mixed_status_exit`, the placeholder-guard and confirm-gate stages.
5. **Switch the clap surface** to generated (Option A) once all groups are registry-driven — or keep derive + the unpack macro (Option B) if the parity spike fails.
6. **Delete the residue:** the 64 arg structs (Option A), the empty handlers. `lib.rs` collapses.

At every step the tree builds and `cargo xtask ci` is green. The risk I was hedging against earlier — a half-finished rewrite leaving a broken tree — is sequenced away.

---

## 7. Go / no-go gate

The whole bet rests on one empirical question: **does the 104-handler inventory classify cleanly into {declarative data | ≤8 named validators | closed capability set | ~8 bespoke non-API}, with <~10–15% residue that resists?**

**Inventory (full classification of all 104 `dispatch_*` functions).** Base buckets are mutually exclusive and sum to 104; capability tags overlap.

| Base bucket | Count | Capability tag | Count |
|---|---|---|---|
| PURE (14 routers + 4 engine-plumbing + 19 leaf) | 37 | secret-capture | 7 |
| VALIDATION | 36 | pagination | 14 |
| BESPOKE-NON-API | 5 | confirm-gate | 12 |
| capability/residue-only (no P/V/BNA tag) | 26 | streaming | 5 |
| | | macro | 5 |
| **total** | **104** | chunking | 3 |

Of the 18 `validate_*`/`normalize_*` functions: **~10 are cleanly declarative** (range, enum, length, required-combination — e.g. `validate_cursor_pagination` alone covers 13 sites) and **~8 are genuinely semantic** (search category aliasing, admin date-window math, the LinkedIn-domain heuristic, conditional nested-object checks). That puts the named-validator hatch right at its ≤8 budget — tight but viable.

**Verdict: GO, with the §2.6 framing — not the naive "all data" reading.** The residue is real and lands at ~15%:
- **16 conditional-body handlers** (§7.D1) that hand-roll JSON because of gap G5 (group-level defaults). These do *not* collapse to clean data. Under §2.6 they keep a thin named `body_builder` and still ride the one pipeline — so they cost us declarative coverage, not pipeline coverage.
- **2 true outliers** (§7.D3, D4): `auth test` (live call, custom envelope, no registry presence) and `raw` (intentionally registry-less). `raw` stays bespoke by design; `auth test` folds in as a synthetic op.
- **5 cross-cutting inconsistencies** (§7.D2/D5/D6/D7/D8) — the unused live validator, opt-in-not-enforced confirm-gate (with a live latent bug), three confirm protocols, inconsistent secret-capture strictness, body-keyed query derivation. These are *features of the refactor*, not blockers: each one is a thing the convergence fixes by construction.

So: **100% pipeline convergence is achievable; ~75% declarative-body coverage is achievable; the conditional-body tail (~16) stays as thin named builders.** That clears the bar for the bug-class-killing value. If you wanted the stricter "everything is data" target, the answer would be no-go on G5 alone — which is exactly why §2.6 reframes the goal.

If the council disagrees that G5/the 16-handler tail is containable, the fallback remains the pragmatic extraction (move handlers into `src/commands/*.rs`, extract `exec.rs`, stop) — strictly worse on the bug class but lower-risk.

**The residue, itemized** (referenced above as §7.D*):
- **D1 — 16 conditional-body handlers.** Hand-roll JSON via private `build_*_named_body` helpers because of G5 (group-level defaults / conditional nested construction). ~15% of handlers. Resolved by §2.6 (thin named builder, still pipelined).
- **D2 — the unused live validator.** `validate_registry_input` exists and is wired only to `schema validate-input`; the 36 VALIDATION handlers reimplement its checks by hand. Resolved by wiring it into the pipeline.
- **D3 — `auth test`.** Live call, custom `exa.cli.auth_test.v1` envelope, zero registry presence. Folds in as a synthetic op (or a narrow `Probe` capability).
- **D4 — `raw`.** Intentionally registry-less escape hatch; stays bespoke by design.
- **D5 — confirm-gate is opt-in, not engine-enforced** from `op.destructive()`; live latent bug at `websets_searches_cancel` (§1.4). Resolved by the engine confirm stage.
- **D6 — three confirm protocols** (`--yes`; `--confirm <id>`; `--yes`+`--confirm delete`). The `Confirm` capability models all three.
- **D7 — inconsistent secret-capture strictness** (admin-keys mandatory vs monitor/webhook optional). Captured by `SecretCapture { required }`.
- **D8 — body-keyed query derivation** (`websets preview` adds `?search=true` when `search.count` present). Captured by `QueryFromBody`.

---

## 8. Open questions for review (what to pressure-test)

1. **Option A vs B for the clap surface** (§2.2) — is dynamic-clap-from-registry worth losing derive ergonomics? Does the builder API match derive's error UX?
2. **Is the enriched-`FieldDef` model the right abstraction, or a mini-DSL trap?** (§5.1) Where exactly is the line between "declare it" and "write a 10-line builder"?
3. **Capability enum completeness** (§2.1) — is `{SecretCapture, Chunked, Macro}` + the existing typed fields the full closed set, or is there a special behavior it can't express?
4. **Validator-hatch sizing** (§2.4) — is ≤8 realistic given the 19 current validators, or does the semantic tail blow the budget?
5. **Is this worth it vs. the pragmatic extraction?** The honest cost is weeks. The honest benefit is killing a bug class and making "new endpoint = data" true. Which matters more for where this product is going?
