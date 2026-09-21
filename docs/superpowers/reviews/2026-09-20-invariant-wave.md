# Invariant-compliance wave — opus, 2026-09-20

Graded the tree against the goal's `## Invariants` and `## Success criteria` with machine evidence.
Reviewer counts as delivered: **Critical 5 · Major 3** (all outside crate source). Verdicts and triage:

| Item | Wave verdict | Triage |
|---|---|---|
| Strict typing, no internals leaked, typed core / raw at edge, DRY, deep modules, ponytail, fail loudly, test quality, no legacy/TODOs | PASS | — |
| Review gates evidenced in tree | NOT-EVIDENCED | ordering: `2026-09-20-final-review.md` is written when the merge-gate review lands |
| Pre-push gate reachable | FAIL | environment: Pedro's global `core.hooksPath` has no `pre-push`, so git never reaches the repo stub. Outside the blast radius. README states the limitation; `mise run check` is the manual gate. Open item for Pedro: add a chaining `~/.git-hooks/pre-push`. |
| Hooks proven live (unformatted commit rejected) | FAIL at read time | done after the wave's read: `/tmp/guideme-hook-reject.log` shows fmt and clippy failing, `REJECT_EXIT=1`, scratch discarded |
| Per-answer `confidence` on the event untested | NOT-EVIDENCED | fixed: tracing test now asks `(noul, choose_among)` and asserts `confidence` on the choice event |
| Repo pushed | FAIL at read time | ordering: pushed after the merge gate |
| GATES.md evidence pending | Major | filled at the end with the final gate run |
| Tree in flux during audit | Major | the hook-rejection proof was running; committed tree never contained the scratch line |
| Out-of-range score untested | Major | fixed: `malformed_answers_are_protocol_errors` now covers `score = 5.0` on three levels |
| Minor: `::guideme` path in derive; unreachable post-loop `Err`; state JSON allocated for `state.bytes`; README pre-push claim | Minor | README fixed; the other three are standard shapes and stay |
