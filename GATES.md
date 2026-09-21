# Gates: guideme 0.1.0 — Rust core for TypeSafe Jev

Scope: the goal at docs/superpowers/goals/2026-09-20-guideme-rust-core.md, delivered as a private repo pedro-pscunha/guideme on main with a green gate and two fable reviews applied.

- [x] G1: Plan written and gated by one fable brooks-review, findings triaged via receiving-code-review
  CHECK: ls docs/superpowers/plans/ && ls docs/superpowers/reviews/
  EXPECT: /plan.*\.md/
  EVIDENCE: docs/superpowers/plans/2026-09-20-guideme-rust-core.md; docs/superpowers/reviews/2026-09-20-plan-review.md (0 Critical / 5 Major / 16 Minor, every row triaged)

- [x] G2: Workspace builds and the full gate is green
  CHECK: zsh -lc 'source ~/.zshrc; cd /Users/pedrocunha/repos/guideme && set -o pipefail; mise run check 2>&1 | tee /tmp/guideme-check.log; echo "EXIT=${pipestatus[1]}" | tee -a /tmp/guideme-check.log' && tail -1 /tmp/guideme-check.log
  EXPECT: EXIT=0
  EVIDENCE: /tmp/guideme-check.log: "Summary [1.049s] 24 tests run: 24 passed, 1 skipped", doctests "1 passed", "advisories ok, bans ok, licenses ok, sources ok", EXIT=0

- [x] G3: Wire fidelity — docs example payloads round-trip through api::Answer, plus serde proptest
  CHECK: zsh -lc 'source ~/.zshrc; cd /Users/pedrocunha/repos/guideme && cargo nextest run -E "test(examples_from_docs_round_trip) | test(api_request_and_response_survive_serde_round_trip)" 2>&1 | grep -E "PASS|FAIL|Summary"'
  EXPECT: /2 tests run: 2 passed/
  EVIDENCE: both PASS in /tmp/guideme-check.log (guideme::wire examples_from_docs_round_trip, api_request_and_response_survive_serde_round_trip)

- [x] G4: Retry contract against wiremock (429 retry-after honoured, 401 → Auth once, 529 exhausts)
  CHECK: zsh -lc 'source ~/.zshrc; cd /Users/pedrocunha/repos/guideme && cargo nextest run -E "binary(wire)" 2>&1 | grep -E "Summary"'
  EXPECT: /passed/
  EVIDENCE: guideme::wire 7 tests PASS (rate_limit_is_retried_after_the_advertised_delay 1.021s, unauthorized_is_not_retried, overload_exhausts_retries_then_fails, a_body_that_violates_the_contract_is_a_protocol_error)

- [x] G5: Policy laws hold as proptests (noul monotone/unsure band, choice fallback iff low confidence, score level = argmax, unknown option → Protocol)
  CHECK: zsh -lc 'source ~/.zshrc; cd /Users/pedrocunha/repos/guideme && cargo nextest run -E "binary(policy)" 2>&1 | grep -E "Summary"'
  EXPECT: /passed/
  EVIDENCE: guideme::policy 6 tests PASS incl. noul_verdict_is_monotone_in_probability_and_thresholds, choice_is_unsure_iff_confidence_below_threshold, score_index_is_the_argmax_and_value_stays_in_range, malformed_answers_are_protocol_errors

- [x] G6: Derives round-trip and trybuild rejects misuse; no catch-all arms in crate code
  CHECK: zsh -lc 'source ~/.zshrc; cd /Users/pedrocunha/repos/guideme && cargo nextest run -E "binary(derive)" 2>&1 | grep -E "Summary"; rg -c "_ =>" guideme/src guideme-derive/src || echo NO_CATCH_ALL'
  EXPECT: NO_CATCH_ALL
  EVIDENCE: guideme::derive 4 tests PASS; 7 ui cases with committed .stderr (tuple_variant, one_variant, eleven_levels, two_fallbacks, duplicate_keys, missing_level_rubric, key_on_levels); rg "_ =>" → NO_CATCH_ALL

- [x] G7: Batch sends one request with N questions and honours per-Ask overrides
  CHECK: zsh -lc 'source ~/.zshrc; cd /Users/pedrocunha/repos/guideme && cargo nextest run -E "binary(batch)" 2>&1 | grep -E "Summary"'
  EXPECT: /passed/
  EVIDENCE: guideme::batch 4 tests PASS (one request with ids q0..q4, per-question min_confidence + .or honoured, unsure ladder, out-of-rubric → Protocol)

- [x] G8: Observability is structural (span fields asserted via capturing layer; state absent unless record_state)
  CHECK: zsh -lc 'source ~/.zshrc; cd /Users/pedrocunha/repos/guideme && cargo nextest run -E "binary(tracing)" 2>&1 | grep -E "Summary"'
  EXPECT: /passed/
  EVIDENCE: guideme::tracing 2 tests PASS; typed span fields model.*, questions, usage.*, retries, elapsed_ms asserted; event fields probability (noul) and confidence (choice) asserted; state absent by default, present with record_state(true)

- [x] G9: Secrets never leak from Debug
  CHECK: zsh -lc 'source ~/.zshrc; cd /Users/pedrocunha/repos/guideme && cargo nextest run -E "binary(redaction)" 2>&1 | grep -E "Summary"'
  EXPECT: /passed/
  EVIDENCE: guideme::redaction debug_output_never_contains_the_api_key PASS

- [x] G10: spec/ regenerates identically (drift guard) and docs/porting.md exists
  CHECK: zsh -lc 'source ~/.zshrc; cd /Users/pedrocunha/repos/guideme && mise run spec >/dev/null && git status --porcelain spec/ | wc -l | tr -d " " && test -f docs/porting.md && echo PORTING_OK'
  EXPECT: PORTING_OK
  EVIDENCE: guideme::spec committed_spec_matches_render PASS (5 files equal; 42 vectors, 3 negative; every vector re-resolves to its stored Outcome); docs/porting.md present

- [x] G11: lefthook hooks installed and reject an unformatted commit
  CHECK: ls .git/hooks/pre-commit .git/hooks/pre-push && grep -l lefthook .git/hooks/pre-commit
  EXPECT: pre-commit
  EVIDENCE: stubs present; /tmp/guideme-hook-reject.log: unformatted in-tree change → "🥊 fmt", "🥊 clippy", REJECT_EXIT=1, scratch discarded. Pre-push stub is unreachable on this machine (global core.hooksPath has no pre-push); scripts/install-hooks.sh now exits 1 naming it.

- [x] G12: Live contract test exists, is #[ignore], runs only with TYPESAFE_API_KEY
  CHECK: rg -n "#\[ignore" guideme/tests/live.rs
  EXPECT: ignore
  EVIDENCE: guideme/tests/live.rs `#[ignore = "hits the live TypeSafe API; needs TYPESAFE_API_KEY"]`; the invariant wave ran it with a key present: 1 passed

- [x] G13: Test count ≤ 30
  CHECK: zsh -lc 'source ~/.zshrc; cd /Users/pedrocunha/repos/guideme && cargo nextest list --workspace 2>/dev/null | grep -vE "^\s*$|:$" | wc -l | tr -d " "'
  EXPECT: /^([0-9]|[12][0-9]|30)$/
  EVIDENCE: 24 listed (+1 ignored live) = 25; trybuild suite counts as one

- [x] G14: Invariant machine checks — no unwrap/expect/dbg/todo/panic in crate src; reqwest/Value confined
  CHECK: zsh -lc 'cd /Users/pedrocunha/repos/guideme && (rg -n "\.unwrap\(|\.expect\(|dbg!|todo!|unimplemented!|panic!" guideme/src guideme-derive/src || echo CLEAN_SRC)'
  EXPECT: CLEAN_SRC
  EVIDENCE: CLEAN_SRC; NO_AS_CASTS; NONE_IN_LIB (lib.rs has no serde_json::Value/reqwest); `rg -ln "reqwest::" guideme/src` → guideme/src/api/client.rs only

- [x] G15: Final fable brooks-review on the whole implementation applied via receiving-code-review; opus invariant-compliance wave clean
  EVIDENCE: docs/superpowers/reviews/2026-09-20-final-review.md (0 Critical / 3 Major / 16 Minor, all landed except m8 kept as a marked ponytail corner); docs/superpowers/reviews/2026-09-20-invariant-wave.md (5 Critical / 3 Major at read time: all ordering, environment, or since-fixed; per-invariant table PASS)

- [x] G16: Repo private on GitHub, default branch main, HEAD pushed
  CHECK: zsh -lc 'source ~/.zshrc; cd /Users/pedrocunha/repos/guideme && gh repo view pedro-pscunha/guideme --json visibility,defaultBranchRef --jq ".visibility + \" \" + .defaultBranchRef.name" && test "$(git rev-parse HEAD)" = "$(git ls-remote --heads origin main | cut -f1)" && echo SHA_MATCH'
  EXPECT: SHA_MATCH
  EVIDENCE: "PRIVATE main"; HEAD e867296279da81c6a4a2a647566cbbd4deb6c9d9 == origin/main at first push; re-verified after the ledger commit (see final report)
