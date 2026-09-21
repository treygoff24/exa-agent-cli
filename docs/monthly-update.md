# Monthly update facts

Cycle skill: `cli-monthly-update` (skill pool; `claude-skill add
cli-monthly-update` activates it for this repo). This file holds only what
that skill leaves abstract. Every command below is offline unless it says
live.

## Upstream

- Spec sources: `openapi/exa-openapi.json` (Exa Public API) and
  `openapi/team-management.json` (Team Management API); provenance and
  source hashes in `openapi/provenance.toml`, narrative in
  `openapi/PROVENANCE.md`; typed-surface overrides in `openapi/overlay.toml`.
- Re-vendor: `cargo xtask vendor-spec --check` (compare only), then
  `cargo xtask vendor-spec` (write). Rust-only since 0.7.0; no Ruby needed.
- Withdrawn or added routes show up in the Exa docs (docs.exa.ai changelog
  and API reference) and the official SDKs (`exa-js`, `exa-py` on GitHub),
  not in the spec diff. Read those every cycle; `/context` and the retired
  Websets routes were caught this way in 2026-09.
- Papercut search: `papercuts list --all --status open --format md
  --limit 300`, then filter text for `exa-agent`, `exa`, `EXA_API_KEY`;
  tags in use are `exa-agent`, `research`, `research-tools`.

## Build and gate

- Full gate: `EXA_AGENT_NO_NETWORK=1 cargo xtask ci` (fmt, clippy `-D
  warnings`, tests; 672 tests across 29 suites after `exa-wl1`).
- On the devbox cell, build output must live in this repo's managed cache
  directory: `export CARGO_TARGET_DIR="$(estate-build-cache path)"` (a
  per-repo path under `~/.cache/cargo-targets/managed-v1/`; the `rustc-gate`
  hook rejects other locations, including the cache root itself and `/tmp`).
  Lane briefs must carry this line verbatim.
- Offline switch: `EXA_AGENT_NO_NETWORK=1` refuses every live path;
  dry-run and self-description keep working.
- Generated artifacts: the command registry
  (`cargo xtask generate-registry --check`) and the agent skill at
  `skills/exa-agent-cli/SKILL.md` (`cargo xtask generate-skill`, `--check` in
  CI; source is `GUIDE_SECTIONS` in `src/lib.rs`).
- Emitted-command parse invariant: `tests/next_actions.rs` parses every
  `nextAction` and warning-derived command through `Cli::try_parse_from`.
  Extend it when a new command family emits hints.
- Live smoke with a cost ceiling: `cargo xtask smoke --budget 0.05`.

## Ship

- Version in `Cargo.toml`; `CHANGELOG.md` keeps `## Unreleased` on top and
  `## X.Y.Z — YYYY-MM-DD` per release, with Added/Changed/Fixed.
- CI: `.github/workflows/ci.yml`, six jobs (lint, msrv, static-linux,
  release-config, test ubuntu, test macos). Public repo, free minutes.
- Release: push tag `vX.Y.Z`; `.github/workflows/release.yml` (cargo-dist)
  builds six archives (x86_64/aarch64 × linux-gnu, linux-musl, apple-darwin),
  installer script, checksums, and publishes the Homebrew formula
  (`HOMEBREW_TAP_TOKEN`). Windows is a non-goal.
- Local install: `cargo build --release --bin exa-agent`, then
  `install -m 0755 "$CARGO_TARGET_DIR/release/exa-agent" ~/.local/bin/exa-agent`.
  On the estate the shim at `~/.local/bin/estate-shims/exa-agent` wraps it.
- Pool skill to refresh when the playbook changes: `exa-agent-cli` in the
  skill library (it points at `exa-agent robot-docs guide`, so it only needs
  edits when the estate auth ritual or the "rules that bite" change).

## Last cycle

- 2026-09-21: released 0.7.0 (`2e6038e`, tag `v0.7.0`). Resolved
  `exa-wl1` by removing the two retired Websets commands in this commit.
  Carried forward: bead `exa-7fc` (`--dry-run` skips registry range validation;
  non-string `type` bypasses the Snapshot conflict check), papercut
  `pc2_5d400e3f9250ca3b` (503 fallback guidance, research-skill matter).
