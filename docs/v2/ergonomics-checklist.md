# Ergonomics checklist (Wave 6)

Date: 2026-06-30  
Status: committed in-repo release gate; no external skill workspace dependency.

The local `agent-ergonomics-and-intuitiveness-maximization-for-cli-tools` skill is useful for ad hoc audits, but v1 release readiness is gated by this repository's own tests and `xtask` commands.

## Required commands

| Command | Purpose |
| --- | --- |
| `cargo test --test ergonomics -- --nocapture` | Runs the committed intent, robot-docs, and score-floor corpus |
| `cargo xtask ergonomics` | Convenience wrapper for the ergonomics test binary plus offline self-description smokes |
| `cargo xtask phase-gate 6` | Full workspace tests, ergonomics, self-description smokes, final dry-run smokes |
| `cargo xtask smoke --budget "$EXA_E2E_BUDGET"` | Final low-cost live smoke against a real Exa key; read-only and cost-capped (default $0.05) |

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

The goal is not to turn scoring into theater; the real gate is binary behavior plus regression tests. The score monitor is a compact release-readiness tripwire.
