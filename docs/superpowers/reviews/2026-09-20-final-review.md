# Merge gate — fable brooks-review of the whole implementation, 2026-09-20

Reviewer counts: **Critical 0 · Major 3 · Minor 16.** Health 69/100 before fixes. The reviewer
independently re-ran the suite, re-resolved all 42 golden vectors, and probed coherence, `Send`,
and the derive diagnostics.

| # | Finding | Decision | Applied as |
|---|---|---|---|
| M1 | `Outcome::Score` cannot deserialise its own JSON (`usize` map keys inside a tagged enum) | accept | `distribution: Vec<Probability>`, `legend: Vec<String>` (positional); `spec/` regenerated; `tests/spec.rs` now re-resolves every vector and compares to the stored `Outcome` |
| M2 | pre-push gate never runs under the global `core.hooksPath` | accept (blast-radius part) | `scripts/install-hooks.sh` exits 1 naming the dead hook when a global hooks path lacks a chaining hook; README states it; the report says pre-push is not live on this machine |
| M3 | "questions are values" without `Clone`/`Debug`/const | accept | `Clone + Debug` on `Noul`/`Choose`/`Score`/`Detailed`, manual impls on `Question<K>`; design doc reworded to "build once, clone per ask" |
| m1 | `#[guide(key)]` on `Levels` silently ignored | accept | compile error naming the rule; `tests/ui/key_on_levels.rs` |
| m2 | threshold one-liners on every kind | accept | sealed markers `Binary` (noul) and `Confident` (choice/score); `.yes_above/.no_below` and `.min_confidence` moved onto them |
| m3 | `.or().detail()` drops the fallback silently | accept | documented on `detail()` |
| m4 | `with_policy` does not validate | accept | returns `Result`, settles immediately |
| m5 | `models` untyped on 429/529 | accept | typed; shared `classify` for 401/422/other |
| m6 | `retry-after` beyond 30 s retried early | accept | fails loudly with `RateLimited { retry_after }` |
| m7 | invalid `State` through `Client` reported as `Transport` | accept | request serialised once in `Client`, `Error::Config` |
| m8 | state serialised twice per ask | ponytail | kept, marked `// ponytail:` in `Guide::ask` |
| m9 | choice distribution length unchecked | accept | length check against the rubric |
| m10 | option/level limits duplicated | accept | `api::MAX_OPTIONS`, `api::MAX_LEVELS`; the derive keeps literals (no cycle) |
| m11 | failed ask leaves no error marker on the span | accept | `error` field recorded; documented |
| m12 | `spec` module public | accept | `#[doc(hidden)]` |
| m13 | `Policy` struct-literal constructible | accept | `#[non_exhaustive]` |
| m14 | tokio `rt`/`macros` in the lib | accept | lib `time` only; dev `rt-multi-thread`, `macros` |
| m15 | GATES.md unchecked | accept | evidence filled from the final gate run before push |
| m16 | doc nits | accept | CHANGELOG date, `Error::Invalid` doc, `Thresholds` doc link |

Loop-break: no Critical/Major open. `mise run check` green after the fixes (24 passed, 1 ignored).
