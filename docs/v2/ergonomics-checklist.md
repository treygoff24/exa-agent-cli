# Ergonomics checks

Date: 2026-06-30  
Status: committed checks; no external skill workspace dependency.

The local `agent-ergonomics-and-intuitiveness-maximization-for-cli-tools` skill is useful for ad hoc audits, but v1 release readiness is gated by this repository's own tests and `xtask` commands.

## Verification commands

| Command | Purpose |
| --- | --- |
| `cargo test -j 8 --test ergonomics -- --test-threads=8 --nocapture` | Runs the committed intent, robot-docs, and score-floor corpus |
| `cargo xtask ergonomics` | Convenience wrapper for the ergonomics test binary plus offline self-description smokes |
| `cargo xtask phase-gate 6` | Full workspace tests, ergonomics, self-description smokes, final dry-run smokes |
| `cargo xtask smoke --budget "$EXA_E2E_BUDGET"` | Final low-cost live smoke against a real Exa key; read-only and cost-capped (default $0.05) |

Workers run only targeted checks through `testrun`; the coordinator owns full
workspace and release gates. Set `EXA_AGENT_NO_NETWORK=1` for offline probes.
On the devbox, export `CARGO_TARGET_DIR="$(estate-build-cache path)"` before Cargo
builds. Live smoke requires a real credential and intentional network access.

## Intent-mistake corpus

The release gate pins the predictable mistakes most likely to waste agent loops:

- bare `search` requests query-aware `contents.highlights`; `search --text` maps to `contents.text.maxCharacters=1500`; `search --no-highlights` is metadata-only.
- `search --limit N`, `search --count N`, and `search --all` fail with `invalid_flag_combination` and a paste-ready `--num-results` suggestion.
- `search --filter category=news` fails with a typed `--category news` suggestion.
- Category near-misses such as `companys` fail with `details.didYouMean`.
- `company` and `people` category filters reject unsupported domain/date combinations.
- `people` include-domain filters accept only LinkedIn domains.
- `contents --set contents.text=true` and equivalent nested `--body` shapes are rejected because `/contents` uses top-level `--text` / `--summary-query`.
- `websets create --num-results N` is rejected in favor of `--count N`.

## Robot-docs completeness

`tests/ergonomics/robot_docs.rs` compares the live binary outputs:

- `robot-docs commands --compact` and `capabilities --compact` must publish the same command set.
- `robot-docs errors --compact` and `capabilities --compact` must publish the same error-code set.
- `robot-docs guide --compact` must mention `suggestedCommand`, `--dry-run`, `--print-request`, `--num-results`, and `robot-docs errors`.

## Score floor

The committed Wave 6 floor is **700 minimum per dimension**. The in-repo score monitor tracks:

| Dimension | Floor |
| --- | ---: |
| self_documentation | 700 |
| output_parseability | 700 |
| error_teaching | 700 |
| intent_inference | 700 |
| determinism | 700 |
| dangerous_op_safety | 700 |

The score floor supplements binary behavior and regression tests. It does not
establish live API access or replace the coordinator's integrated gate.
