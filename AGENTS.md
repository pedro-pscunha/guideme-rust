# guideme working rules

Rules for anyone (or anything) changing this repository. Read fully before editing.

## What this is

A Rust workspace that makes a TypeSafe Jev judgment usable as control flow: a yes/no is an
`if`, a choice is an exhaustive `match`, a score is a comparison. Two crates:

- `guideme`: the product. Public surface is what `guideme/src/lib.rs` re-exports, nothing else.
- `guideme-derive`: `#[derive(Choice)]` and `#[derive(Levels)]`, re-exported by `guideme`.

The live TypeSafe docs are the source of truth for the wire contract:
`https://docs.typesafe.ai/api.md`. Re-read that page before touching `guideme/src/api/`.

## Layout and seams

| Path | Owns | Rule |
|---|---|---|
| `guideme/src/api/mod.rs` | exact wire mirror of `POST /v1/systemone` and `GET /v1/models` | mirrors the docs field for field; no policy here |
| `guideme/src/api/client.rs` | HTTP, retries, status → `Error` | the only file that may name `reqwest` |
| `guideme/src/policy.rs` | `resolve(&Answer, Thresholds) -> Outcome`, `Policy`, `Thresholds` | pure: no I/O, no generics, no Rust enums; its behaviour is the shared contract |
| `guideme/src/question.rs` | question kinds, constructors, `Options`/`Levels` traits, the unsure ladder | every kind stays sealed |
| `guideme/src/ask.rs` | the `Ask` shape trait (question, tuple, `Vec`, `BTreeMap`) | sealed; ids are `q0..qN` in encounter order |
| `guideme/src/guide.rs` | `Guide::ask`, spans and events | one `guideme.ask` span per request, one `guideme.answer` event per question |
| `guideme/src/spec.rs`, `src/bin/spec.rs` | schemas and golden vectors under `spec/` | regenerate with `mise run spec`; the drift test fails otherwise |
| `guideme-derive/src/lib.rs` | the two derives | every misuse is a compile error with a message naming the rule |

`docs/design.md` records the decisions and the sharp edges. Update it when a decision changes.

## Invariants

These hold everywhere in `guideme/src` and `guideme-derive/src`. The lints in the workspace
`Cargo.toml` enforce most of them; the rest are checked in review.

- No `unwrap`, `expect`, `panic!`, `dbg!`, `todo!`, `unimplemented!` in library code. Return a typed `Error`.
- No `_ =>` catch-all arms on this crate's own enums. Add a variant, update every match.
- No `as` numeric casts. Use `From`/`TryFrom`.
- Raw JSON only in `api/`. `serde_json::Value` appears on the public surface only as an input to `State` and `Instructions`.
- Probabilities and confidences are validated once, at the wire, into `Probability`/`Confidence`.
- Fail loudly. Unknown answer kind, option or level not in the rubric, malformed body, bad thresholds, empty batch, duplicate runtime keys: each is a typed error. Never a default, never a log-and-continue.
- The API key is never printed. `ApiKey`'s `Debug` is `ApiKey(***)`; it has no `Display` or `Serialize`.
- State is user data. It is never recorded on a span unless `record_state(true)` was set.
- Every public item has a doc comment (`missing_docs` is denied). Doc comments are what the derive turns into rubrics, so they are part of the contract.
- Dependencies stay minimal. Adding one needs a reason in the commit message. Jitter uses `std::hash::RandomState`, not `rand`, on purpose.

## Policy semantics (do not change casually)

- Noul: `p >= yes_above` is yes, `p <= no_below` is no, strictly between is unsure. Defaults `0.5 / 0.5`.
- Choice and score: `confidence < min_confidence` is unsure. Default `0.0`.
- Precedence: question `.with(..)` and one-liners > guide (`builder().policy`, `with_policy`) > defaults.
- Unsure ladder: `.or(value)` > the enum's `#[guide(fallback)]` > `Error::Unsure`. `.detail()` never fails.
- Score plain output is the argmax level; `.detail()` also exposes the API's expected `value`.
- A batch is atomic.

A change to any of these changes `spec/vectors/policy.json` and therefore every other SDK.
Bump the version, regenerate the spec, and say so in `CHANGELOG.md`.

## Tests

Few tests, high grade. The ceiling is 30 nextest entries. A new test must be one of:

- a property test (`proptest`) over a law of `policy::resolve` or the wire types;
- a wire or contract check through `wiremock`, asserting on received requests and typed results;
- a structural tracing assertion through a capturing `Layer`, on field names and values;
- a compile-time guarantee (`trybuild` case under `guideme/tests/ui/`).

Never assert on log or `Debug` text, except to prove a secret is absent. Never mock `policy`
or `Guide`. Test files start with
`#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::pedantic, missing_docs)]`.

The live tests in `guideme/tests/live.rs` are `#[ignore]` and hit the real API. Run them
only on purpose:

```
TYPESAFE_API_KEY=… cargo nextest run -p guideme --test live --run-ignored ignored-only --no-capture
```

## Commands

```
mise run check      # the gate: fmt-check, clippy -D warnings, nextest, doctests, rustdoc, cargo-deny
mise run test       # nextest + doctests only
mise run lint       # clippy only
mise run spec       # regenerate spec/ after any change to api, policy, or the vector grid
mise run hooks      # install the git hooks (see below)
```

Run everything from the repo root. Capture long output to a file; do not pipe a gate through
`tail`.

## Git

- Branch from `main`, open a PR, squash-merge. `main` is protected by the gate.
- The pre-commit hook runs fmt and clippy; the pre-push hook runs `mise run check`.
  If a global `core.hooksPath` is set, git runs only that directory; `mise run hooks`
  exits non-zero and names any hook that will not fire. Run `mise run check` by hand before
  pushing when that happens.
- Commit messages: imperative subject under 72 characters, body says why. No trailers, no
  tool attributions, no generated-by lines.
- Never commit an API key, a `.env`, or anything under `.tmp/`.

## Changing the contract

1. Read the current TypeSafe API page.
2. Change `api/mod.rs` to mirror it. Add or update the docs-example fixture in `tests/wire.rs`.
3. If the change reaches `policy`, update `resolve`, the property tests, and `docs/contract.md`.
4. `mise run spec`, commit the regenerated `spec/`.
5. `mise run check`.
6. Note it in `CHANGELOG.md` and open an issue in each other SDK repository citing the new spec commit.

## Other SDKs

Each guideme SDK lives in its own repository and is written from scratch in its own language.
This repository publishes the shared contract under `spec/` and states it in
`docs/contract.md`: the wire schemas, the policy vectors, and the interface shape. A change
here that touches the contract is announced to every other SDK repository with the commit
that regenerated `spec/`.
