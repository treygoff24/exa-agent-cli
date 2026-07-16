# Model performance journal

## 2026-07-16 - gpt-5.6-sol via Codex - Wave 5 standards review

Command and run: `delegate codex safe --model sol --reasoning-effort xhigh
--prompt-file <standards-review>`; alias/variant/effort: `sol`,
`gpt-5.6-sol`, xhigh; mode/isolation: safe/temporary worktree; run handle:
`codex-66` (`del_20260716T150248Z_b78ee2`).

Task and expectation: Review the diff since `f871b1a` for AGENTS.md,
Ponytail, and code-smell violations without editing.

Outcome and verification: Completed in 110.9 seconds. It found the real
argument-order weakness in the search-specific raw-argv error path and a valid
maintainability concern. Its highest-severity enum-validation finding was
misstated because the operation-aware runtime validator still rejected invalid
search categories; the coordinator traced callers and retained that validation.
The argument-order fix passed targeted CLI and ergonomics tests.

Performance observations: Fast for xhigh and line-specific. It correctly
challenged a workaround introduced after the first full-suite failure, but did
not trace the permissive parser through the downstream validator before calling
it a trust-boundary bypass. The safe snapshot warned that two external skill
symlinks were replaced with placeholders; neither affected the reviewed diff.

Routing assessment: Use again for final standards gates, especially workaround
and abstraction scrutiny, but verify trust-boundary claims end to end. Compare
against a spec-focused lane before accepting severity. Confidence: high.

## 2026-07-16 - grok-4.5-fast-xhigh via Cursor - Wave 5 spec review

Command and run: `delegate cursor safe --prompt-file <spec-review>`;
alias/variant/effort: no alias, `grok-4.5-fast-xhigh`, harness default; mode/
isolation: safe/temporary worktree; run handle: `cursor-33`
(`del_20260716T150440Z_4b355d`).

Task and expectation: Check the final diff since `f871b1a` against the exact
nine-finding remediation list, using read/grep only.

Outcome and verification: Completed in 220.7 seconds. It found the load-bearing
circular enum-parity defect: search-category Clap values were sourced from the
same registry values the test compared. The fix now supplies Clap's independent
`SEARCH_CATEGORY_VALUES` while preserving the operation-aware validator. It
also found a hardcoded text-cap value in error copy; that now formats the
registry-derived maximum. Targeted parity, CLI, and ergonomics tests passed.
The `vendor-spec --check` concern was rejected because the task explicitly
scoped the guard to the non-check curl path.

Performance observations: Thorough and evidence-rich despite the no-shell
constraint. It separated implemented requirements from findings and caught a
semantic false green missed by the initial suite. It over-reported two low-scope
items already present before this round and repeated its report in the raw
assistant text. The safe snapshot had the same harmless skill-symlink warning.

Routing assessment: Use again as the decorrelated spec attacker for registry
and test circularity. Re-rank medium/low findings against exact user wording.
Confidence: high.
