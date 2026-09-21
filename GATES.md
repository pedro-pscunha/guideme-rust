# Gates: guideme 0.1.0 — Rust core for TypeSafe Jev

Scope: the goal at docs/superpowers/goals/2026-09-20-guideme-rust-core.md, delivered as a private repo pedro-pscunha/guideme on main with a green gate and two fable reviews applied.

- [ ] G1: Plan written and gated by one fable brooks-review, findings triaged via receiving-code-review
  CHECK: ls docs/superpowers/plans/ && ls docs/superpowers/reviews/
  EXPECT: /plan.*\.md/
  EVIDENCE: pending

- [ ] G2: Workspace builds and the full gate is green
  CHECK: zsh -lc 'source ~/.zshrc; cd /Users/pedrocunha/repos/guideme && set -o pipefail; mise run check 2>&1 | tee /tmp/guideme-check.log; echo "EXIT=${pipestatus[1]}" | tee -a /tmp/guideme-check.log' && tail -1 /tmp/guideme-check.log
  EXPECT: EXIT=0
  EVIDENCE: pending

- [ ] G3: Wire fidelity — docs example payloads round-trip through api::Answer, plus serde proptest
  CHECK: zsh -lc 'source ~/.zshrc; cd /Users/pedrocunha/repos/guideme && cargo nextest run -E "test(examples_from_docs_round_trip) | test(api_request_and_response_survive_serde_round_trip)" 2>&1 | grep -E "PASS|FAIL|Summary"'
  EXPECT: /2 tests run: 2 passed/
  EVIDENCE: pending

- [ ] G4: Retry contract against wiremock (429 retry-after honoured, 401 → Auth once, 529 exhausts)
  CHECK: zsh -lc 'source ~/.zshrc; cd /Users/pedrocunha/repos/guideme && cargo nextest run -E "binary(wire)" 2>&1 | grep -E "Summary"'
  EXPECT: /passed/
  EVIDENCE: pending

- [ ] G5: Policy laws hold as proptests (noul monotone/unsure band, choice fallback iff low confidence, score level = argmax, unknown option → Protocol)
  CHECK: zsh -lc 'source ~/.zshrc; cd /Users/pedrocunha/repos/guideme && cargo nextest run -E "binary(policy)" 2>&1 | grep -E "Summary"'
  EXPECT: /passed/
  EVIDENCE: pending

- [ ] G6: Derives round-trip and trybuild rejects misuse; no catch-all arms in crate code
  CHECK: zsh -lc 'source ~/.zshrc; cd /Users/pedrocunha/repos/guideme && cargo nextest run -E "binary(derive)" 2>&1 | grep -E "Summary"; rg -c "_ =>" guideme/src guideme-derive/src || echo NO_CATCH_ALL'
  EXPECT: NO_CATCH_ALL
  EVIDENCE: pending

- [ ] G7: Batch sends one request with N questions and honours per-Ask overrides
  CHECK: zsh -lc 'source ~/.zshrc; cd /Users/pedrocunha/repos/guideme && cargo nextest run -E "binary(batch)" 2>&1 | grep -E "Summary"'
  EXPECT: /passed/
  EVIDENCE: pending

- [ ] G8: Observability is structural (span fields asserted via capturing layer; state absent unless record_state)
  CHECK: zsh -lc 'source ~/.zshrc; cd /Users/pedrocunha/repos/guideme && cargo nextest run -E "binary(tracing)" 2>&1 | grep -E "Summary"'
  EXPECT: /passed/
  EVIDENCE: pending

- [ ] G9: Secrets never leak from Debug
  CHECK: zsh -lc 'source ~/.zshrc; cd /Users/pedrocunha/repos/guideme && cargo nextest run -E "binary(redaction)" 2>&1 | grep -E "Summary"'
  EXPECT: /passed/
  EVIDENCE: pending

- [ ] G10: spec/ regenerates identically (drift guard) and docs/porting.md exists
  CHECK: zsh -lc 'source ~/.zshrc; cd /Users/pedrocunha/repos/guideme && mise run spec >/dev/null && git status --porcelain spec/ | wc -l | tr -d " " && test -f docs/porting.md && echo PORTING_OK'
  EXPECT: PORTING_OK
  EVIDENCE: pending

- [ ] G11: lefthook hooks installed and reject an unformatted commit
  CHECK: ls .git/hooks/pre-commit .git/hooks/pre-push && grep -l lefthook .git/hooks/pre-commit
  EXPECT: pre-commit
  EVIDENCE: pending

- [ ] G12: Live contract test exists, is #[ignore], runs only with TYPESAFE_API_KEY
  CHECK: rg -n "#\[ignore" guideme/tests/live.rs
  EXPECT: ignore
  EVIDENCE: pending

- [ ] G13: Test count ≤ 30
  CHECK: zsh -lc 'source ~/.zshrc; cd /Users/pedrocunha/repos/guideme && cargo nextest list 2>/dev/null | grep -cE "^\s{4}\S"'
  EXPECT: /^([0-9]|[12][0-9]|30)$/
  EVIDENCE: pending

- [ ] G14: Invariant machine checks — no unwrap/expect/dbg/todo/panic in crate src; reqwest/Value confined
  CHECK: zsh -lc 'cd /Users/pedrocunha/repos/guideme && (rg -n "\.unwrap\(|\.expect\(|dbg!|todo!|unimplemented!|panic!" guideme/src guideme-derive/src || echo CLEAN_SRC)'
  EXPECT: CLEAN_SRC
  EVIDENCE: pending

- [ ] G15: Final fable brooks-review on the whole implementation applied via receiving-code-review; opus invariant-compliance wave clean
  EVIDENCE: pending

- [ ] G16: Repo private on GitHub, default branch main, HEAD pushed
  CHECK: zsh -lc 'source ~/.zshrc; cd /Users/pedrocunha/repos/guideme && gh repo view pedro-pscunha/guideme --json visibility,defaultBranchRef --jq ".visibility + \" \" + .defaultBranchRef.name" && test "$(git rev-parse HEAD)" = "$(git ls-remote --heads origin main | cut -f1)" && echo SHA_MATCH'
  EXPECT: SHA_MATCH
  EVIDENCE: pending
