# Autonomous Implementation Plan

Date: 2026-06-29
Status: execution overlay for a long `/goal` run. It does not replace
[`implementation-plan.md`](implementation-plan.md); it defines how Codex should
orchestrate that plan with delegated implementation and review lanes.

## Source of truth

Read, in order:

1. [`decisions.md`](decisions.md) — canonical on product and architecture calls.
2. [`contracts.md`](contracts.md) — canonical on schemas, exit codes, redaction,
   streaming, retry/idempotency, and stdout/stderr behavior.
3. [`commands.md`](commands.md) — canonical command surface.
4. [`architecture.md`](architecture.md) — canonical module/build shape.
5. [`implementation-plan.md`](implementation-plan.md) — canonical phase gates.
6. This file — canonical execution protocol for the autonomous run.

If this file conflicts with the first five, fix this file.

## Operating contract

Parent Codex is the orchestrator, reviewer, and final integrator. It owns scope,
file ownership, validation, merge decisions, and closeout. It should not be the
default feature implementer.

Implementation lanes:

- **Native Codex subagents** for bounded implementation tasks.
- **Delegate Cursor Composer** for bounded implementation tasks:

  ```bash
  delegate cursor work --isolation worktree --forbid-commit "<bounded prompt>"
  ```

Review lanes at the end of every wave:

- **Native Codex code-review subagent**.
- **Delegate GLM review**:

  ```bash
  delegate droid glm safe "<review prompt>"
  ```

Do not use `delegate codex`; use native subagents for Codex-on-Codex work.

## Non-negotiable guardrails

- One file has one implementation owner per wave.
- Workers get exact owned paths and forbidden actions.
- Workers do not commit, push, remove Delegate worktrees, run destructive cleanup,
  or touch unrelated files.
- Workers may run targeted checks. Parent Codex runs heavy gates once after
  integration.
- Parent reviews every worktree/diff before accepting it.
- Parent makes regular **local commits only** after coherent, reviewed,
  passing checkpoints. Do not push; no remote is assumed or required.
- Secrets never appear in prompts, logs, traces, or final output. Use redacted
  examples only.
- A phase/wave is not complete until both review lanes are clean or every
  finding is fixed or explicitly rejected with rationale in the run ledger.

## Start sequence for the `/goal` run

1. Inspect git state:

   ```bash
   git status --short
   ```

   Current observed state on 2026-06-29: the scaffold is untracked. Before
   implementation fan-out, make an explicit checkpoint decision: either commit
   the current scaffold or confirm that the dirty tree is the intended baseline.

2. Confirm tool lanes:

   ```bash
   delegate --json describe
   delegate --json models
   ```

3. Establish a run ledger in [`../../work/autonomous-run.md`](../../work/autonomous-run.md).
4. Run the narrow baseline:

   ```bash
   cargo test --workspace --locked
   ```

5. Only then begin Wave 0.

## Wave lifecycle

Every wave follows the same loop:

1. **Open the wave.** Record objective, owned files, invariants, and target gate
   in the run ledger.
2. **Build context packs.** Each worker prompt names the docs and files it must
   read; no worker receives vague repo-wide ownership.
3. **Spawn implementation lanes.** Use native subagents and/or Cursor Composer
   worktrees. Keep file ownership disjoint.
4. **Integrate serially.** Parent Codex inspects diffs, rejects off-scope work,
   and applies or merges the minimum acceptable change.
5. **Run targeted checks.** Run the smallest checks that prove the wave's logic.
6. **Native review.** Ask a native review subagent for findings only.
7. **GLM review.** Ask Delegate GLM for findings only.
8. **Fix pass.** Use native subagents or Cursor Composer to fix accepted
   findings. Parent may do tiny integration-only fixes directly.
9. **Gate.** Parent runs the wave's gate once.
10. **Commit checkpoint.** If the wave produced a coherent passing diff, parent
    creates a local commit. For large waves, commit smaller reviewed batches
    whenever they pass their targeted checks. Workers never commit.
11. **Record closeout.** Ledger records commands, results, remaining risks, and
    rejected findings.

## Local commit cadence

Use local commits as restore points, not ceremony:

- Commit after every completed wave.
- Commit mid-wave when a reviewed batch is coherent and its targeted checks
  pass.
- Do not commit failing, unreviewed, or off-scope work.
- Do not push or create PRs unless Trey explicitly asks later.
- Prefer boring messages like `phase1: wire error envelopes` or
  `phase2: add contents request builder tests`.

## Required review prompts

Native review prompt shape:

```text
Review the current diff for Wave <N>: <name>.

Source docs: docs/v2/decisions.md, contracts.md, commands.md,
architecture.md, implementation-plan.md, autonomous-implementation-plan.md.

Scope: <files or diff summary>.
Find bugs, contract violations, missing tests, safety gaps, and maintainability
problems. Do not implement. Output findings ordered by severity with file,
line, evidence, and suggested fix. If none, say "No blocking findings."
```

GLM review prompt shape:

```text
You are in code review mode. Do not update the plan. Do not enter plan mode.
Do not say the plan is up to date.

Review the current diff for Wave <N>: <name>.
Output findings only: bugs, contract violations, missing tests, safety gaps,
and maintainability problems. Include file, line, evidence, severity, and
suggested fix. If none, output exactly: "No blocking findings."
```

Implementation prompt shape:

```text
Implement Wave <N>: <name>.

Read first: <docs/files>.
Owned paths: <exact paths>.
Do not edit outside owned paths. Do not commit, push, delete worktrees, run
full gates, or touch secrets.
Success criteria: <specific behavior/tests>.
Run only targeted checks: <commands>.
Report changed files, checks run, risks, and any follow-up needed.
```

## Wave map

| Wave | Scope | Preferred lanes | Gate |
|---|---|---|---|
| 0 | Baseline, git checkpoint, spec/overlay audit, lane readiness | parent only, review lanes if anomalies appear | `cargo test --workspace --locked`; `cargo xtask vendor-spec --check` once implemented |
| 1A | Registry, parser, envelope, error-code dictionaries | native subagent for contracts/tests; Cursor for mechanical CLI wiring | targeted tests, then `cargo xtask phase-gate 1` when ready |
| 1B | Request body building, `--body`/`--set`, redaction, print-request/dry-run | Cursor for mechanical request plumbing; native for contract tests | targeted request-builder/redaction tests |
| 1C | Auth/config/keyring/doctor offline surface | native for auth boundary; Cursor for CLI subcommands | targeted auth/config/doctor tests |
| 1D | `raw`, typed `search` dry-run, goldens, Phase-1 closeout | native for spine tests; Cursor for command completion | `cargo xtask phase-gate 1` |
| 2A | `search` and `contents` typed APIs | Cursor for flag/request breadth; native for partial-status logic | `cargo xtask phase-gate 2` subset |
| 2B | `answer`, `context`, `similar`, streaming path | native for streaming/contracts; Cursor for command wiring | targeted stream and request goldens |
| 2C | `team info`, `ask`, `fetch`, deprecation warnings | Cursor for mechanical surface; native for warnings/contracts | `cargo xtask phase-gate 2` |
| 3A | Agent/research create/list/get/cancel/delete | native for idempotency/pending-run; Cursor for command breadth | targeted async lifecycle tests |
| 3B | SSE events, SIGINT, resume, pagination | native only unless work is purely mechanical | `cargo xtask phase-gate 3` |
| 4A | `monitor` family | Cursor for command breadth; native for confirmation safety | targeted safety/pagination tests |
| 4B | `websets` core/items/searches/enrichments/imports | Cursor for breadth; native for conflict/idempotency tests | targeted websets tests |
| 4C | websets monitors/events/webhooks and Phase-4 closeout | native for webhook/security docs; Cursor for command wiring | `cargo xtask phase-gate 4` |
| 5 | `admin keys` and service-key separation | native first; Cursor only for mechanical subcommands | `cargo xtask phase-gate 5` |
| 6 | Ergonomics harness, robot-docs completeness, live smoke, release readiness | native reviews plus Cursor for fixture/golden churn | `cargo xtask phase-gate 6`; `cargo xtask smoke --budget "$EXA_E2E_BUDGET"` |

## Completion criteria

The autonomous goal can be marked complete only when all are true:

- Every v1 namespace in [`commands.md`](commands.md) is implemented or explicitly
  deferred by an existing decision.
- `cargo xtask ci` passes locally.
- `cargo xtask phase-gate 1` through `cargo xtask phase-gate 6` pass locally.
- Live non-paid smoke passes with a real `EXA_API_KEY`:

  ```bash
  cargo xtask smoke --budget "$EXA_E2E_BUDGET"
  ```

- The installed/local binary works for at least:

  ```bash
  cargo run -- capabilities --json
  cargo run -- search "test query" --dry-run --print-request --json
  cargo run -- raw GET /v0/teams/me --dry-run --print-request --json
  ```

- Both review lanes are clean on the final diff, or every remaining finding is
  explicitly rejected with rationale.
- No Delegate worktree contains unreviewed edits.
- The final git tree is clean except for intentionally untracked artifacts
  documented in the ledger.

If a real API key is unavailable, the run cannot be marked complete; mark it
blocked after all offline gates pass.

## Blocked conditions

Stop and ask Trey before continuing if:

- git status contains user-owned changes that conflict with planned edits;
- Cursor/Delegate/GLM auth is unavailable after one retry;
- upstream Exa specs or docs contradict the vendored specs materially;
- a phase gate fails after two focused fix passes;
- live smoke needs credentials or spend approval not available in the session.
