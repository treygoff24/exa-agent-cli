# Monthly update facts

Cycle skill: `cli-monthly-update` (skill pool; `claude-skill add
cli-monthly-update` activates it for this repo). This file holds only what
that skill leaves abstract. Networked commands are marked below; publication is a separate authorized step.

## Upstream

- Spec sources: `openapi/exa-openapi.json` (Exa Public API) and
  `openapi/team-management.json` (Team Management API); provenance and
  source hashes in `openapi/provenance.toml`, narrative in
  `openapi/PROVENANCE.md`; typed-surface overrides in `openapi/overlay.toml`.
- Offline identity/provenance check: `cargo xtask vendor-spec --check` checks
  committed specs, hashes, and overlay; it does not fetch or detect upstream
  drift. Audit fresh copies from `https://exa.ai/docs/exa-spec.json` and
  `https://exa.ai/docs/team-management-spec.yaml` before re-vendoring with
  `cargo xtask vendor-spec` (live fetch and tracked writes). Rust-only since
  0.7.0; no Ruby needed.
- Read `exa.ai/docs/changelog`, the API reference, and official SDKs (`exa-js`,
  `exa-py` on GitHub) every cycle. Added or withdrawn routes may precede or be
  absent from the public spec diff; `/context` and retired Websets routes were
  caught this way in 2026-09.
- Papercut search: `papercuts list --all --status open --format md
  --limit 0`, then filter text for `exa-agent`, `exa`, `EXA_API_KEY`;
  tags in use are `exa-agent`, `research`, `research-tools`.

## Build and gate

- Full gate: `EXA_AGENT_NO_NETWORK=1 cargo xtask ci` (fmt, clippy `-D
  warnings`, tests). The coordinator runs it once on the integrated candidate;
  worker lanes stop at targeted checks through `testrun`.
- Before release, also run `cargo +1.85 clippy --locked --all-features
  --all-targets -- -D warnings`. Current stable Clippy does not catch every lint
  enforced by the minimum supported Rust version.
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
- Live smoke with a cost ceiling: `cargo xtask smoke --budget 0.05`. This sends
  requests; unset `EXA_AGENT_NO_NETWORK` only for an intentionally live run.

## Ship

- Version in `Cargo.toml`; `CHANGELOG.md` keeps `## Unreleased` on top and
  dated version headings per release, with Added/Changed/Fixed.
- CI: `.github/workflows/ci.yml`, six jobs (lint, msrv, static-linux,
  release-config, test ubuntu, test macos). Public repo, free minutes.
- GitHub reads below use the network. GitHub pushes, tags, releases, and registry
  publication require explicit authorization; local verification does not publish.
- Finding the run for a pushed commit: `gh run list --commit SHA` lags the
  push by several minutes; `gh run list --branch main --json databaseId,headSha`
  shows it at once. Then `gh run watch ID --exit-status`.
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

## Latest local cycle

- 2026-10-07: 0.8.0 release preparation and local verification. The full gate passed 690 tests,
  `cargo publish --locked --dry-run` passed, and installed provenance and package
  identity were checked. Live search accepted objective, location, and larger
  content caps; contents returned usable text. These checks cost $0.008. No paid
  Ultra run or live Agent/Websets mutation was performed.
- Added Ultra/duration budgets, objective, country/JSON/paired-coordinate location,
  1,000,000-character content caps, and Macrobond. Fixed preset/schema validation,
  safe migration hints, supported Websets metadata-key limits, and Firecrawl
  recovery for government content. SDK-only beta Agent Monitors remain untyped;
  deprecated filters get no new typed flags.
- The earlier validation and fallback papercuts were resolved with current
  evidence. Detailed receipts are retained locally in `work/oct07/RESULT.md`,
  `work/STATE.md`, and the installation/review reports under
  `work/oct07/`. These ignored files are not distributed with the repository.

### 0.8.0 release handoff

Publication is pending. Trey will publish from the Mac, which holds the crates.io
credential. GitHub main received the release preparation; its Rust 1.85 Clippy
check found a boolean simplification missed by stable Clippy. The fix is on
Forgejo main and needs to reach GitHub before tagging.

After preserving any local work, fast-forward the Mac checkout from Forgejo.
Push the updated `main` to GitHub and wait for all six CI jobs to pass. Then:

```sh
git tag -a v0.8.0 -m 'Release 0.8.0'
git push origin refs/tags/v0.8.0
git push github refs/tags/v0.8.0
EXA_AGENT_NO_NETWORK=1 cargo publish --locked
```

Watch the release workflow through Homebrew publication. Verify the six platform
archives and their checksums, the shell installer, and crates.io version 0.8.0.
Run `exa-agent --version` and offline `capabilities --json` from an installed
release artifact. The Exa network guard does not prevent Cargo registry uploads.

## Previous published cycle

- 2026-09-21: released 0.7.0 (tag `v0.7.0`). The subsequent local maintenance
  removed the two retired Websets export commands. The 0.7.0 release record is
  preserved in [CHANGELOG.md](../CHANGELOG.md).
