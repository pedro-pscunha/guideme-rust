# Plan gate — fable brooks-review, 2026-09-20

Reviewer counts: **Critical 0 · Major 5 · Minor 16.** Verdict: design sound; plan not executable verbatim until M1/M5 fixed; four fail-loudly gaps.

| # | Finding | Decision | Applied as |
|---|---|---|---|
| M1 | pedantic + `-D warnings` breaks on plan code | accept | `return_self_not_must_use = "allow"`; test header allows `clippy::pedantic`; `as` casts replaced by `try_from`/`from` |
| M2 | `choose_among` never checks answer ∈ rubric | accept | membership check in `Choose::read_detail` before `from_key` |
| M3 | unsure ladder + runtime verbs untested | accept | `batch.rs`: ladder test + protocol test; `policy.rs`: thresholds monotonicity |
| M4 | malformed 200 body → `Transport` | accept | decode via `serde_json::from_str` → `Error::Protocol`; wire test through `Client` |
| M5 | redaction test six tasks early → hooks off | accept | `redaction.rs` written with the Guide; hook-rejection proof added to final task |
| m1 | porting doc says 2..=10, resolve checks ≥2 | accept | `resolve` enforces `2..=10` |
| m2 | `Thresholds` pub fields unvalidated | accept | private fields, `Thresholds::new(..) -> Result`, `#[serde(try_from)]`, getters |
| m3 | `spec::render` swallows errors | accept | `render() -> Result`, const grid, `?`, vector-count assertion, negative vectors, schema range on Probability/Confidence |
| m4 | duplicate runtime option keys collapse | accept | length check after collecting into `BTreeMap` |
| m5 | `ranked`/`value` overloaded | accept | score `distribution`; event fields `probability` / `confidence` / `value` by kind |
| m6 | Client records parent span field | accept | `pub(crate) evaluate_counted -> (Response, u32)`; `Guide` records `retries` |
| m7 | noul `Unsure.threshold` only `yes_above` | partial | keep `question: String` (same id as wire/span); threshold = nearer boundary |
| m8 | wrong tracing note | accept | note deleted; `usize` records directly |
| m9 | tasks 6/7 reversed | accept | question kinds before derives |
| m10 | corrections as prose | accept | code written final; superseded blocks dropped |
| m11 | `choose::<Key>` fails at runtime | pushback | the two-impl remedy overlaps under coherence (no negative impls on stable); documented on `Key`/`Rank` instead |
| m12 | `Question` name clash with `api::Question` | pushback | user-facing name kept; stated in `docs/design.md` sharp edges |
| m13 | `syn` `full` unneeded | accept | default features; add back only on compile error |
| m14 | inline threshold setters lost | accept | `Question::{yes_above,no_below,min_confidence}` as one-liners over `.with` |
| m15 | policy/fallback not on event | accept (cheap) | settled thresholds on `guideme.answer`; fallback non-observability documented |
| m16 | derive proptests are loops; `take` clones; retry-after uncapped | accept | plain loops; `Reply::get`; clamp to `MAX_BACKOFF` |

Loop-break: no Critical/Major remains open after the amendments. Re-review not required (no structural finding).
