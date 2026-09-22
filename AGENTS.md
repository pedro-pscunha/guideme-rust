# guideme working rules

Rules for anyone (or anything) changing this repository. Read fully before editing.

## What this is

A Rust workspace that makes a TypeSafe Jev judgment usable as control flow: a yes/no is an
`if`, a choice is an exhaustive `match`, a score is a comparison. Two crates, both published
to crates.io under `MIT OR Apache-2.0`:

- `guideme`: the product. Public surface is what `guideme/src/lib.rs` re-exports, nothing else.
- `guideme-derive`: `#[derive(Choice)]` and `#[derive(Levels)]`, re-exported by `guideme`.

The public surface is a published API. Anything removed or renamed in it is a breaking change
for people who do not work here, so it needs a major bump and a `CHANGELOG.md` entry.

The live TypeSafe docs are the source of truth for the wire contract, over two pages:
`https://docs.typesafe.ai/api.md` covers `POST /v1/systemone` and
`https://docs.typesafe.ai/models.md` covers `GET /v1/models`. Re-read both before touching
`guideme/src/api/`.

## Layout and seams

| Path | Owns | Rule |
|---|---|---|
| `guideme/src/api/mod.rs` | exact wire mirror of `POST /v1/systemone` and `GET /v1/models` | mirrors the docs field for field; no policy here |
| `guideme/src/api/client.rs` | HTTP, retries, status → `Error`, one HTTP client span per attempt and the `guideme.retry` event | the only file that may name `reqwest`; span fields follow the OpenTelemetry HTTP client conventions |
| `guideme/src/policy.rs` | `resolve(&Answer, Thresholds) -> Outcome`, `Policy`, `Thresholds` | pure: no I/O, no generics, no Rust enums; its behaviour is the shared contract |
| `guideme/src/question.rs` | question kinds, constructors, `Options`/`Levels` traits, the unsure ladder | every kind stays sealed |
| `guideme/src/rubric.rs` | `Rubric`, the runtime half of the rubric renderer | renders identically to `guideme-derive`; a test pins the two |
| `guideme/src/ask.rs` | the `Ask` shape trait (question, tuple, `Vec`, `BTreeMap`) | sealed; ids are `q0..qN` in encounter order |
| `guideme/src/guide.rs` | `Guide::ask`, spans and events | one `guideme.ask` span per request, one `guideme.answer` event per question; span fields follow the OpenTelemetry GenAI conventions, anything else is namespaced `guideme.` |
| `guideme/src/spec.rs`, `src/bin/spec.rs` | schemas and golden vectors under `spec/` | regenerate with `mise run spec`; the drift test fails otherwise |
| `guideme-derive/src/lib.rs` | the two derives and the rubric rendering | every misuse is a compile error with a message naming the rule; the rendered rubric is a cross-SDK contract item |

`docs/design.md` records the decisions and the sharp edges. Update it when a decision changes.

Telemetry is `tracing` only; the crate installs no subscriber. The shape: one `guideme.ask`
span per `Guide::ask` (target `guideme`), one HTTP client span per attempt beneath it
(`POST /v1/systemone`, target `guideme::api`) with a `guideme.retry` warning when throttled,
and one `guideme.answer` event per question. `docs/observability.md` records every field; a
change to any of them must land there in the same commit, and the names are part of the
cross-SDK contract (see below).

`examples/` holds runnable programs. Each is its own workspace root with its own lock file, so
the gate formats but does not build them, and their dependencies stay out of the library's
tree. Build one by running cargo inside its directory. CI does build `examples/otlp` with `--locked`, and its
lock file pins `guideme` through a path dependency, so a change to the library's dependency
set **or its version** must refresh `examples/otlp/Cargo.lock` in the same pull request:
`cargo build` inside `examples/otlp`, without `--locked`, rewrites it.

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
- State is user data. Its content is never recorded on a span unless `record_state(true)` was set; its length, `guideme.state.bytes`, always is.
- Telemetry field names come from the OpenTelemetry semantic conventions when one exists (`gen_ai.*`, `http.*`, `server.*`, `url.*`, `error.type`) and are namespaced `guideme.` otherwise. Numbers are `i64`. Failures mark the span (`error.type`, `otel.status_code`, `otel.status_description`) and are returned; no `ERROR` event is ever emitted.
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

Few tests, high grade. The ceiling is 30 entries in the run the gate performs — what
`cargo nextest run --workspace` reports, which is 29 today. The `#[ignore]`d live tests are
not in it: they never execute in the gate, so they are not what the ceiling protects. A new
test must be one of:

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
mise run hooks      # activate the tracked git hooks in .githooks (see below)
```

Run everything from the repo root. Capture long output to a file; do not pipe a gate through
`tail`.

## Git

- Branch from `main`, open a PR, squash-merge. `main` takes pull requests only: a GitHub
  ruleset requires every CI check below to pass before a merge, and refuses direct pushes.
- CI (`.github/workflows/ci.yml`) runs the same gate on every pull request and on `main`,
  scans the whole history with `gitleaks`, builds and lints `examples/otlp`, and proves the
  crate builds on the MSRV. A weekly run re-checks the advisory database against an unchanged
  lock file. CI holds no secrets and never runs the live tests. `.github/dependabot.yml` is
  what moves the SHA-pinned actions forward.
- The hooks are tracked in `.githooks/` and do nothing until you run `mise run hooks`, which
  points this repository's `core.hooksPath` at that directory and fails, leaving nothing
  changed, if the result is not active. Git reads one hooks directory and a repo-local
  `core.hooksPath` outranks a global one, so these fire even where you have a global hooks
  directory — which also means a global secret-scanning hook stops running here, and is why
  these hooks scan for secrets themselves.
- `pre-commit`: `gitleaks` on the staged change, then `mise run fmt-check` and `mise run lint`.
  `pre-push`: `gitleaks` over every range git reports as being pushed, so a branch other than
  the checked-out one is scanned too, then `mise run check` when a branch with content is
  pushed. A delete or a tag-only push runs no gate.
- Every tool a hook needs is resolved loudly. A missing `gitleaks` or `mise` refuses the
  commit or the push; no hook ever skips a check because a binary was not on `PATH`. A
  scanner that fails to run is reported as that, not as a finding.
- `--no-verify` skips a hook. Two known limits are not escape hatches but read like them: the
  gate inspects the working tree, not the index or the pushed commit, so a partial `git add -p`
  is checked against the files on disk; and a checkout of a commit older than `.githooks/` has
  no hooks at all, global ones included. CI on every pull request is the backstop for both.
- Commit messages: imperative subject under 72 characters, body says why. No trailers, no
  tool attributions, no generated-by lines.
- Never commit an API key, a `.env`, or anything under `.tmp/`.

## Changing the contract

1. Read the current TypeSafe API and models pages.
2. Change `api/mod.rs` to mirror them. Add or update the docs-example fixture in `tests/wire.rs`.
3. If the change reaches `policy`, update `resolve`, the property tests, and `docs/contract.md`.
4. `mise run spec`, commit the regenerated `spec/`.
5. `mise run check`.
6. Note it in `CHANGELOG.md` and open an issue in each other SDK repository citing the new spec commit.

Changing how a rubric renders is a contract change too: the composition is shared with every
other SDK, so it goes through `docs/contract.md`, `spec/vectors/rubric.json` and
`CHANGELOG.md`. The clause labels and the declaration ordering are part of it. A rubric with no
examples and no counterexamples must keep rendering to itself, byte for byte; that invariant is
what makes every declaration written before the feature put the same bytes on the wire. The
renderer lives twice, in `guideme-derive` and in `guideme/src/rubric.rs`, because
`Options::RUBRIC` is a `const`; change both, and the pin in `guideme/tests/rubric.rs` is what
catches you if you do not.

Renaming, adding or removing a span or event field is also a contract change: it goes through
`docs/observability.md`, `docs/contract.md` and `CHANGELOG.md`, and is announced the same way.
`spec/` is unaffected.

## Releasing

Both crates are on crates.io and share the workspace `version`. `guideme` depends on
`guideme-derive` by version, so the two are always published together, derive first. Cargo
orders them itself.

**A published version is permanent.** It can be yanked, which stops new dependents resolving
to it and leaves existing lock files alone, but it can never be deleted or replaced. Get the
gate green before uploading, not after.

1. Bump `version` in the root `Cargo.toml`; both crates inherit it. Then `cargo build` inside
   `examples/otlp` without `--locked`, and commit its refreshed `Cargo.lock` with the bump:
   the example pins the library's version, and CI builds it `--locked`.
2. Move the `Unreleased` notes in `CHANGELOG.md` under the new version with today's date.
3. `mise run check`, then commit and push.
4. `git tag -a vX.Y.Z` and push the tag.
5. Publish with the crates.io token from Infisical (`CRATES_IO_TOKEN`, `prod`, path `/`):

```
export CARGO_REGISTRY_TOKEN=…
cargo publish --workspace --locked
```

6. `gh release create vX.Y.Z --notes-file …`, attaching the archives from `target/package/`.
7. Check docs.rs built: `https://docs.rs/guideme/X.Y.Z`.

Breaking the observability field names or the policy semantics is a contract change; see
above and below before bumping.

## Other SDKs

Each guideme SDK lives in its own repository and is written from scratch in its own language.
This repository publishes the shared contract under `spec/` and states it in
`docs/contract.md`: the wire schemas, the policy vectors, and the interface shape. A change
here that touches the contract is announced to every other SDK repository with the commit
that regenerated `spec/`.

The SDKs, so the announcement has addresses:

| Language | Repository |
|---|---|
| Rust | this repository |
| Python | [`pedro-pscunha/guideme-python`](https://github.com/pedro-pscunha/guideme-python) |

That repository vendors `spec/` and records the commit it came from in its `spec/SOURCE`; a
job there fails when its copy drifts from this repository's `main`. So a contract change
lands here first, and the drift job is what tells the other SDK to catch up.
