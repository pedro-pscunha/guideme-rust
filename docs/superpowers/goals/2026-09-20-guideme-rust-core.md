# Goal: `guideme` — a type-safe Rust core for TypeSafe Jev (noul / choice / score) usable inline in `if`, `match` and expressions

## Intent
Make a Jev judgment feel like a language primitive in Rust: `if guide.noul("Should we refund?").on(&ticket).await? { … }`, `match guide.choose::<Department>(…).on(&ticket).await? { … }` with compiler-enforced exhaustiveness over a `#[derive(Choice)]` enum, and `guide.score::<Frustration>(…)` returning a typed level plus the raw expected value. Several judgments over the same state go out as one request through `guide.batch(state, (a, b, c))`. Every judgment resolves through an explicit, configurable **policy** (thresholds, unsure band, fallback) set once on the `Guide` and overridable per call, and every call emits one structured `tracing` span. The crate is a finished product on its own and the reference from which the Python/TypeScript SDKs will be ported: the decision logic is pure (no I/O), and the crate emits a language-agnostic `spec/` (JSON Schemas + golden vectors) that every future SDK must pass.

## Context
Greenfield. `/Users/pedrocunha/repos/guideme` is empty and not yet a git repo. No prior code to reuse; the only source of truth is the live TypeSafe API, read on 2026-09-20 and summarised here so the executor never guesses:

- Endpoint: `POST https://api.typesafe.ai/v1/systemone`, header `Authorization: Bearer <TYPESAFE_API_KEY>`. Also `GET /v1/models` (name, description, release_date).
- Request: `{ state: string | object | array, model: string, questions: { <id>: Question } }`. Default model alias `jev-latest` (→ `jev-1.13.0`); `jev-preview` alias also exists. Question ids are never sent to the model.
- Question (shared `type`, `instructions: string | object | array`):
  - `noul`: optional `criteria: { "true": str, "false": str }`.
  - `choice`: `criteria: { <option>: str | null }`, max 255 options.
  - `score`: `criteria: [str, …]` ordered low→high, 2..=10 levels.
- Response: `{ model: string, answers: { <id>: Answer }, usage: { input_tokens, output_tokens } }`.
  - noul answer: `{ type:"noul", noul: f64 }` — no confidence field.
  - choice answer: `{ type:"choice", choice: str, probabilities: { <option>: f64 }, confidence: f64 }`.
  - score answer: `{ type:"score", score: f64, legend: { "0": str, … }, probabilities: { "0": f64, … }, confidence: f64 }`; `score` is the probability-weighted value and may land between levels.
- Errors: `401` bad key, `422` validation (body names the field), `429` rate limit, `529` overloaded. Official SDKs retry 429/529 with exponential backoff and honour `retry-after`.
- Semantics that shape the policy layer: a noul near 0.5 means "yes and no equally likely", not medium intensity; thresholds live in caller code and scale with risk; confidence is derived from the distribution and is absent on noul; a score's `legend`/`probabilities` are keyed by level index.

## Skills (load the matching one BEFORE working in its area — every job on this matter)
**Review gates (always):** `brooks-review` (the two fable passes Pedro authorised: plan, final implementation) + `brooks-sweep` (opus, discretionary per-wave quality sweeps), `requesting-code-review` (before merge), `receiving-code-review` (triage every finding — reviewer never fixes its own findings). Add `brooks-test` for the test-suite design wave (few tests, property-based, no log-string matching).
**Domain specialists:** `typesafe-ai` (live docs are source of truth; re-read the API page before touching the wire layer), `rust-best-practices` (ch. 2 lint set, ch. 4 `thiserror`, ch. 7 newtypes/enums/typestate, ch. 10 structure, ch. 11 toolchain gates), `rust-performance` (only if a hot path appears; none expected).
**Design:** `codebase-design` (deep modules, seams, deletion test — use its vocabulary in `docs/design.md`), `ponytail` at `full` (fewest files, stdlib first, no speculative abstraction; mark real corners with `// ponytail:`).
**Process/meta:** `writing-plans`, `subagent-driven-development` (opus executors), `test-driven-development`, `systematic-debugging`, `unlazy` (GATES.md ledger; no done-report with unchecked boxes), `superpowers:verification-before-completion`.
**Caveats:** `write-goal`'s Invariant Library is Python/Arche-flavoured (pyright, Pydantic, testcontainers); translate, don't copy — the Rust equivalents are fixed in `## Invariants` below. `rust-best-practices` ch. 11 names a toolchain version as a placeholder; pin the actual stable (1.98.1 installed today).

## Scope
**In scope**
- A Cargo workspace with two members: `guideme` (lib, the product) and `guideme-derive` (proc-macro crate, re-exported by `guideme` so callers add one dependency). Edition 2024, async (`tokio` + `reqwest` with rustls), errors via `thiserror`, `tracing` spans, `serde`/`serde_json`.
- Layer 0 `guideme::api` — exact typed mirror of the HTTP contract: `Request`, `Question` (3 variants), `Answer` (3 variants), `Usage`, `ModelInfo`; `Client::evaluate` (many questions, one request) and `Client::models`. Retry with exponential backoff + jitter on 429/529 honouring `retry-after`; configurable `max_retries`, timeout.
- Layer 1 `guideme::Guide` — the inline verbs `noul`, `choose::<C>`, `score::<L>`, plus ad-hoc forms with inline criteria (`choose_among`, `score_levels`) for callers who don't want an enum. Each takes any `impl Into<State>` (text, `serde_json::Value`, or any `Serialize`), which is the `guide_me_str` / `guide_me_json` idea expressed as one overloaded entry point.
- Policy layer (pure, no I/O): `Policy` defaults on the `Guide` (global), per-call overrides (local), typed fallbacks; `Verdict` for the three-way noul read. Precedence: call > guide > crate defaults.
- Derive macros `#[derive(Choice)]` and `#[derive(Levels)]` on plain unit-variant enums. The rubric for a variant is its `///` doc comment (override with `#[guide(rubric = "…")]`); the wire key defaults to the variant name in snake_case (override with `#[guide(key = "…")]`); at most one variant may carry `#[guide(fallback)]`. `Levels` uses declaration order as the level order. Misuse (non-unit variant, fewer than two variants, more than ten levels, two fallbacks, duplicate keys) is a compile error with a message naming the rule.
- Batching on the `Guide`: `guide.batch(state, (ask_a, ask_b, ask_c))` sends one request and returns a tuple of typed outputs (tuples of 1..=8, each `Ask` keeping its own local policy); `guide.batch_all(state, Vec<Ask<K>>)` for the "one noul per label" pattern. One tracing span per request, not per question.
- Observability: one `tracing` span per evaluation with structured fields (model requested and answered, question count/kinds, state byte length, input/output tokens, retries, elapsed, per-answer outcome/confidence, policy applied, fallback used). State content is **never** recorded unless `Config::record_state(true)`. API key redacted from every `Debug`.
- `spec/` output: JSON Schemas (`schemars`) for `Request`/`Response`/`Policy` and golden vectors `(answer, policy) → resolution` produced from the property tests, plus `docs/porting.md` explaining how a Python/TS SDK is validated against them.
- Tooling: `mise.toml` (tasks: `fmt`, `lint`, `test`, `doc`, `deny`, `spec`, `check`), `rust-toolchain.toml` pinned, `deny.toml`, `lefthook.yml` (pre-commit: fmt-check + clippy; pre-push: `mise run check`), `.gitignore`, `LICENSE` (none needed for private; skip), README with the three verbs, `docs/design.md`, `docs/observability.md`, `docs/porting.md`, `CHANGELOG.md` (0.1.0).
- Private GitHub repo `pedro-pscunha/guideme`, default branch `main`, pushed with a green gate.
**Out of scope**
- Python / TypeScript / other bindings (PyO3, napi, uniffi, wasm). This goal only guarantees the *foundation* (pure policy module + `spec/`).
- A sync/blocking API, a process-global `Guide` singleton, streaming, caching, metrics exporters (users bridge `tracing` to OpenTelemetry themselves), CLI, crates.io publishing, CI workflows on GitHub Actions.
**Blast radius**
- May create/modify: everything under `/Users/pedrocunha/repos/guideme/**`.
- Must not touch: anything outside that directory, including `~/.agents/skills`, `~/.claude`, other repos, global mise config (`mise use -g` is allowed only if a tool is missing — none are).

## Public interface
Designed up front; the executor may rename nothing here without annotating the deviation. No `reqwest`/`serde_json` internals leak except `serde_json::Value` as an *input* convenience for `State`.

```rust
// ── Layer 1: the product ────────────────────────────────────────────────
pub struct Guide { /* Arc<inner> — cheap Clone */ }
impl Guide {
    pub fn from_env() -> Result<Guide, Error>;           // TYPESAFE_API_KEY, optional TYPESAFE_BASE_URL, GUIDEME_MODEL
    pub fn builder() -> GuideBuilder;                    // .api_key(..) .model(..) .policy(Policy) .max_retries(u32) .timeout(Duration) .record_state(bool) .build()
    pub fn noul(&self, instructions: impl Into<Instructions>) -> Ask<Noul>;
    pub fn choose<C: Options>(&self, instructions: impl Into<Instructions>) -> Ask<Choose<C>>;
    pub fn choose_among<'a>(&self, instructions: impl Into<Instructions>, options: impl IntoIterator<Item = (&'a str, Option<&'a str>)>) -> Ask<Choose<String>>;
    pub fn score<L: Levels>(&self, instructions: impl Into<Instructions>) -> Ask<Score<L>>;
    pub fn score_levels<'a>(&self, instructions: impl Into<Instructions>, levels: impl IntoIterator<Item = &'a str>) -> Ask<Score<usize>>;
    pub async fn models(&self) -> Result<Vec<ModelInfo>, Error>;
    /// One request, many judgments. `asks` is a tuple of 1..=8 `Ask`s; the output is the tuple of their `K::Out`s.
    pub async fn batch<T: Batch>(&self, state: impl Into<State>, asks: T) -> Result<T::Out, Error>;
    /// Same-kind batch, e.g. one noul per label.
    pub async fn batch_all<K: Kind>(&self, state: impl Into<State>, asks: Vec<Ask<K>>) -> Result<Vec<K::Out>, Error>;
}

/// A pending question. Local overrides go here; `.on(state)` sends it.
pub struct Ask<K> { /* kind K carries the typed fallback */ }
impl<K> Ask<K> {
    pub fn criteria(self, yes: &str, no: &str) -> Self;          // noul only (method exists on Ask<Noul>)
    pub fn yes_above(self, p: f64) -> Self;                     // noul
    pub fn no_below(self, p: f64) -> Self;                      // noul; no_below <= yes_above else Error::Config at .on()
    pub fn min_confidence(self, c: f64) -> Self;                // choose / score
    pub fn unsure(self, fallback: Fallback<K::Out>) -> Self;    // Fallback::Fail | Fallback::Use(value)
    pub async fn on(self, state: impl Into<State>) -> Result<K::Out, Error>;
}
// K::Out:  Noul → bool   |   Choose<C> → C   |   Score<L> → Scored<L>
impl Ask<Noul>       { pub async fn verdict(self, state: impl Into<State>) -> Result<Verdict, Error>; }
impl<C: Options> Ask<Choose<C>> { pub async fn ranked(self, state: impl Into<State>) -> Result<Ranked<C>, Error>; } // full distribution + confidence

pub enum Verdict { Yes(Probability), No(Probability), Unsure(Probability) }
pub struct Scored<L> { pub value: f64, pub level: L /* argmax */, pub confidence: Confidence, pub probabilities: Vec<(L, Probability)> }
pub struct Ranked<C> { pub choice: C, pub confidence: Confidence, pub probabilities: Vec<(C, Probability)> }
pub enum Fallback<T> { Fail, Use(T) }

#[derive(Clone, Debug, PartialEq)]
pub struct Policy { pub yes_above: f64, pub no_below: f64, pub min_confidence: f64 } // defaults 0.5 / 0.5 / 0.0
pub struct State(/* serde_json::Value */);  // From<&str>, From<String>, From<serde_json::Value>; State::from_serialize(&T)
pub struct Instructions(/* serde_json::Value */); // From<&str>, From<String>, From<serde_json::Value>
pub struct Probability(f64);  // validated 0..=1 at the wire; Deref/Into<f64>
pub struct Confidence(f64);   // validated 0..=1 at the wire
pub struct ApiKey(String);    // Debug prints "ApiKey(***)"
pub struct Model(String);     // Model::LATEST, Model::PREVIEW, Model::new("jev-1.13.0")

/// Implemented by `#[derive(Choice)]`; may be hand-implemented for dynamic option sets.
pub trait Options: Sized + Copy + Eq + 'static { const RUBRIC: &'static [(&'static str, Option<&'static str>)]; fn from_key(key: &str) -> Option<Self>; fn key(self) -> &'static str; fn fallback() -> Option<Self>; }
/// Implemented by `#[derive(Levels)]`; level order is declaration order.
pub trait Levels:  Sized + Copy + Ord + 'static { const LEVELS: &'static [&'static str]; fn from_index(i: usize) -> Option<Self>; fn index(self) -> usize; }
// `choose_among` and `score_levels` use crate-internal dynamic kinds (String keys / usize index) behind the same `Ask`.

/// Tuples of 1..=8 `Ask`s; `Out` is the tuple of each `K::Out`. Implemented internally via macro_rules; sealed.
pub trait Batch: sealed::Sealed { type Out; }
/// The kind of question an `Ask` carries (Noul, Choose<C>, Score<L>); sealed.
pub trait Kind: sealed::Sealed { type Out; }

// Re-exported from `guideme-derive`:
//   #[derive(Choice)]  on a unit-variant enum: `///` doc = rubric, `#[guide(rubric = "…")]`, `#[guide(key = "…")]`, `#[guide(fallback)]`
//   #[derive(Levels)]  on a unit-variant enum: declaration order = level order (2..=10 variants); caller also derives PartialOrd/Ord

#[derive(Debug, thiserror::Error)]
pub enum Error {
    Auth,                                   // 401
    Invalid { detail: String },             // 422, body's message
    RateLimited { retry_after: Option<Duration> }, // 429 after retries exhausted
    Overloaded,                             // 529 after retries exhausted
    Transport(#[source] Box<dyn std::error::Error + Send + Sync>),
    UnexpectedStatus { status: u16, body: String },
    Protocol { detail: String },            // answer type ≠ question type, option/level not in rubric, probability out of range
    Unsure { probability_or_confidence: f64, threshold: f64 }, // policy said unsure and Fallback::Fail
    Config { detail: String },              // bad thresholds, missing key
}

// ── Layer 0: exact wire mirror, for batching and for porting ────────────
pub mod api {
    pub struct Client;  // Client::new(ApiKey), .with_base_url, .with_model, .with_retries
    impl Client {
        pub async fn evaluate(&self, req: &Request) -> Result<Response, Error>;
        pub async fn models(&self) -> Result<Vec<ModelInfo>, Error>;
    }
    pub struct Request { pub state: State, pub model: Model, pub questions: BTreeMap<QuestionId, Question> }
    pub enum Question { Noul { instructions, criteria: Option<NoulCriteria> }, Choice { instructions, criteria: BTreeMap<String, Option<String>> }, Score { instructions, criteria: Vec<String> } }
    pub struct Response { pub model: Model, pub answers: BTreeMap<QuestionId, Answer>, pub usage: Usage }
    pub enum Answer { Noul { noul: Probability }, Choice { choice: String, probabilities: BTreeMap<String, Probability>, confidence: Confidence }, Score { score: f64, legend: BTreeMap<u8, String>, probabilities: BTreeMap<u8, Probability>, confidence: Confidence } }
    pub struct Usage { pub input_tokens: u64, pub output_tokens: u64 }
    pub struct ModelInfo { pub name: String, pub description: String, pub release_date: String }
    pub struct QuestionId(String);
}

// ── Pure policy (no I/O; the thing other SDKs port) ─────────────────────
pub mod policy {
    pub fn resolve_noul(p: Probability, policy: &Policy) -> Verdict;
    pub fn resolve_choice<C: Options>(a: &api::Answer, policy: &Policy, fallback: Fallback<C>) -> Result<Ranked<C>, Error>;
    pub fn resolve_score<L: Levels>(a: &api::Answer, policy: &Policy, fallback: Fallback<L>) -> Result<Scored<L>, Error>;
}
```

Usage the README must show verbatim (and doctests compile with `no_run`):

```rust
use guideme::{Choice, Fallback, Guide, Levels, Verdict};

let guide = Guide::from_env()?;   // TYPESAFE_API_KEY

// if — noul
if guide.noul("Should this ticket be escalated?").on(&ticket).await? {
    escalate(&ticket);
}
// three-way, with a local unsure band
match guide.noul("Is this about billing?").yes_above(0.7).no_below(0.3).verdict(&ticket).await? {
    Verdict::Yes(_) => billing(),
    Verdict::No(_) => other(),
    Verdict::Unsure(p) => review(p),
}

// match — choice, exhaustive by construction; doc comments are the rubric
#[derive(Choice, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Department {
    /// Payments, invoicing, refunds
    Billing,
    /// Bugs, outages, integrations
    Technical,
    /// Pricing, upgrades, new accounts
    #[guide(fallback)]
    Sales,
}
match guide.choose::<Department>("Which team should handle this?").min_confidence(0.6).on(&ticket).await? {
    Department::Billing   => route_billing(),
    Department::Technical => route_tech(),
    Department::Sales     => route_sales(),   // also what you get below 0.6 confidence
}

// score — typed levels plus the raw expected value
#[derive(Levels, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Frustration {
    /// Calm and polite
    Calm,
    /// Frustrated
    Frustrated,
    /// Very angry
    VeryAngry,
}
let f = guide.score::<Frustration>("How frustrated is the customer?").on(&ticket).await?;
if f.level >= Frustration::Frustrated || f.value > 1.5 { prioritise(); }

// batch — three judgments, one request, one span
let (urgent, dept, f) = guide.batch(&ticket, (
    guide.noul("Is this urgent?"),
    guide.choose::<Department>("Which team should handle this?"),
    guide.score::<Frustration>("How frustrated is the customer?"),
)).await?;

// ad-hoc, no enum
let urgency = guide.score_levels("How urgent?", ["can wait", "this week", "today"]).on("Help! Payouts failing for 3 days.").await?;
```

## Success criteria
Each is a mini-eval whose proof is printed in the transcript.
- [ ] **Wire fidelity.** The three example responses from the API page (noul 0.95; choice billing 0.88/0.12/0.0 conf 0.81; score 1.05 with legend/probabilities conf 0.92) deserialise into `api::Answer` and re-serialise byte-equivalently (modulo key order) — `tests/wire.rs::examples_from_docs_round_trip`, and a `proptest` `api_request_and_response_survive_serde_round_trip`.
- [ ] **Retry contract.** Against a `wiremock` server: `429` with `retry-after: 1` then `200` → `Ok` with exactly 2 requests received and elapsed ≥ 1 s; `401` → `Error::Auth` with 1 request; `529` × (max_retries+1) → `Error::Overloaded` with max_retries+1 requests. One test each, asserting on received-request counts and typed errors, never on log text.
- [ ] **Policy laws (property-based, `proptest`)**: `resolve_noul` is monotone in `p` and in the thresholds and returns `Unsure` iff `no_below < p < yes_above`; `resolve_choice` returns `Fallback::Use(c)` iff `confidence < min_confidence` and `Err(Unsure)` iff `Fallback::Fail` in that case; `resolve_score` `level` always equals the argmax of `probabilities` and `value` always lies within `[0, LEVELS.len()-1]`; an answer whose option/level is not in the rubric yields `Error::Protocol`. Printed `cargo nextest run` output shows these names passing.
- [ ] **Types do the work.** A `#[derive(Choice)]` enum round-trips `key ↔ variant` for every variant and reports its `#[guide(fallback)]`; a `#[derive(Levels)]` enum round-trips `index ↔ variant` with index equal to declaration order (one proptest each over the variant set). A single `trybuild` compile-fail suite (`tests/ui/*.rs`) proves the derive rejects a non-unit variant, fewer than two variants, more than ten levels, two fallbacks, and duplicate keys, each with a message naming the rule. `match` exhaustiveness is enforced by `rustc`; no `_ =>` appears in crate code (`rg '_ =>' guideme/src` prints nothing).
- [ ] **Batch is one request.** Against `wiremock`, `guide.batch(state, (noul, choose, score))` produces exactly one received request whose body has three questions, returns the typed tuple, and a per-`Ask` override (e.g. `min_confidence`) is honoured inside the batch — `tests/batch.rs`, asserting on the received body and the typed outputs.
- [ ] **Observability is structural.** A test installs a capturing `tracing` layer and asserts the evaluation span carries typed fields `model.requested`, `model.answered`, `usage.input_tokens`, `usage.output_tokens`, `retries`, and per-answer `outcome`/`confidence`, and that `state` is absent unless `record_state(true)`. Assertions are on field names and values via the layer, never substring matches on rendered output.
- [ ] **Secrets never leak.** `format!("{:?}", guide)` and `format!("{:?}", ApiKey::from("k"))` contain no key bytes — `tests/redaction.rs`.
- [ ] **Spec is the porting contract.** `mise run spec` regenerates `spec/schema/*.json` and `spec/vectors/*.json`; a test asserts the committed files equal the regenerated output (drift guard). `docs/porting.md` states how a foreign SDK proves conformance.
- [ ] **Gate is green and printed.** `mise run check` = `cargo fmt --check` → `cargo clippy --all-targets --all-features --locked -- -D warnings` (with `unwrap_used`, `expect_used`, `dbg_macro`, `todo`, `unimplemented`, `panic` denied outside tests; `missing_docs` denied) → `cargo nextest run` → `cargo test --doc` → `cargo deny check` → `cargo doc --no-deps -D warnings`; its full output and `EXIT=0` are captured to `/tmp/guideme-check.log` and printed.
- [ ] **Hooks are live.** `lefthook install` ran; `cat .git/hooks/pre-commit .git/hooks/pre-push` shows lefthook stubs; a deliberately unformatted scratch commit is rejected (then discarded) — output printed.
- [ ] **Live contract check exists and is opt-in.** `tests/live.rs` is `#[ignore]`, runs only when `TYPESAFE_API_KEY` is set, sends the three docs example questions and asserts the typed shapes parse; `cargo nextest run --run-ignored ignored-only` output printed if a key is present, otherwise stated as skipped.
- [ ] **Repo is private and pushed.** `gh repo view pedro-pscunha/guideme --json visibility,defaultBranchRef` prints `PRIVATE` / `main`, and `git rev-parse HEAD` equals `git ls-remote --heads origin main`.
- [ ] **Test count stays small.** `cargo nextest list | wc -l` ≤ 30 (the trybuild suite counts as one) and every test is either a property, a wire/contract check, or a compile-time guarantee; printed.

## Invariants
### Universal (translated to Rust)
- **Strict typing at the surface.** Domain scalars are newtypes (`ApiKey`, `Model`, `QuestionId`, `Probability`, `Confidence`); fixed value sets are enums (`Question`, `Answer`, `Verdict`, `Fallback`, `Error`); no `serde_json::Value` or `reqwest` type on the public surface except `State`/`Instructions` inputs; no `String`-keyed maps where an enum fits. Parse, don't validate: `Probability` is validated once at the wire.
- **Never leak internals; interface designed first.** The surface above is the contract; anything else stays `pub(crate)`.
- **Typed core, raw only at the edge.** JSON is touched only in `api::transport` (request encode, response decode). Everything else operates on `api::*` and `policy::*` types.
- **DRY / single owner.** One place resolves each primitive (`policy`), one place talks HTTP, one place builds spans.
- **Deep modules (`codebase-design`).** `Guide` is the external seam; `policy` is a pure internal seam used by tests and `spec`; `api::Client` is the transport seam with one real adapter (reqwest) and no hypothetical port trait.
- **Ponytail full.** No trait with one implementation, no plugin system, no feature flags in v0.1, no sync wrapper, no global singleton, no metrics crate. Expected dependencies: `tokio`, `reqwest` (rustls, json), `serde`, `serde_json`, `thiserror`, `tracing`, `schemars`, `rand` (jitter); `guideme-derive`: `syn`, `quote`, `proc-macro2`, `heck` (snake_case); dev: `proptest`, `wiremock`, `tracing-subscriber`, `trybuild`, `tokio` (macros). This list is a guideline, not a cap: adding a crate is fine with a one-line justification in the plan, never for what a few lines of code do.
- **Fail loudly.** No silent defaults: unknown answer type, option not in rubric, probability outside 0..=1, `no_below > yes_above`, missing key → typed `Error`. No `unwrap`/`expect`/`panic` outside tests (clippy-denied).
- **Test quality over quantity.** Property tests for laws, `wiremock` for the HTTP boundary, structural tracing assertions; never assert on log/format strings; never mock `policy` or `Guide`; no test that re-states the implementation.
- **No legacy paths, no TODOs without an issue link, docs on every public item (`missing_docs` deny).**
- **Review waves are gates:** plan gate = one fable `brooks-review` (authorised by Pedro in his own words for this task, exactly one agent) + `receiving-code-review`; merge gate = one fable `brooks-review` on the whole implementation + `receiving-code-review`. Everything in between runs on opus.
### Context-dependent (decided for this goal)
- Test-data strategy: **wire fixtures + wiremock + property tests**, because the only external boundary is the TypeSafe HTTP API and the docs publish exact example payloads; a `#[ignore]`d live contract test guards against provider drift.
- Stack gates: `mise run check` (fmt, clippy restriction set, nextest, doctests, deny, rustdoc) enforced by lefthook pre-push; fmt + clippy on pre-commit.
- New module: yes — two-crate workspace (`guideme`, `guideme-derive`), public surface enumerated above, `docs/design.md` records the seams and the deletion-test result for each module. The derive crate exposes only the two derives; all trait definitions live in `guideme`.
- Ports & adapters: **no** — one transport adapter; `policy` is pure so no fake is needed.
- Type-model boundary: `api::transport` (serde in/out); `Guide` converts `api::Answer` → `bool`/`C`/`Scored<L>` via `policy`.
- Score `level` = argmax of `probabilities` (mode); `value` = the API's expected-value `score`. Both exposed; documented in `docs/design.md`.
- Foreign-SDK foundation: pure `policy` module + `spec/` (schemas + golden vectors) + `docs/porting.md`. Bindings deferred.

## Execution interaction policy
Questions during execution: **not allowed** once Pedro approves this goal. The executor annotates every assumption inline (`// ASSUMPTION:` in code, an `## Assumptions` list in `PLAN.md`) and surfaces them in the final report. It never blocks mid-run.

## Process
Initialise the repo in place (`git init -b main` in `/Users/pedrocunha/repos/guideme`); no worktree is needed because the repo is new and single-branch — worktrees start with the first feature branch after 0.1.0. Atomic commits; push once the gate is green.

**The plan is written by the agent that executes this goal**, not beforehand: Explore (done in this goal) → Plan (`writing-plans`, with a `codebase-design` design-it-twice pass on the `Ask`/`Policy`/`Batch` interface and the derive attribute grammar) → Plan gate: ONE fable `brooks-review` subagent → `receiving-code-review` → Execute (`subagent-driven-development`, opus executors, `unlazy` GATES.md ledger, `ponytail` full) → discretionary opus `brooks-sweep` on the policy, derive and batch modules → Merge gate: ONE fable `brooks-review` on the whole implementation → `receiving-code-review` → invariant-compliance wave (opus) → push.

- **Invariant-compliance wave (opus, always, before push).** Grades the tree against `## Invariants` item by item with machine evidence: clippy restriction lints in the workspace `[workspace.lints]`, `rg '_ =>' guideme/src guideme-derive/src`, `rg 'unwrap\(|expect\(|dbg!|todo!|unimplemented!' guideme/src guideme-derive/src`, `rg 'serde_json::Value|reqwest' guideme/src/lib.rs` (only `State`/`Instructions` hits allowed), `cargo tree -e normal --depth 1` against the expected dependency list with each extra justified, `cargo nextest list | wc -l`. Any miss is Critical and re-opens the loop.
- Loop-break: both the fable review and the invariant wave report no Critical/Major. Report counts each wave.
- Fable budget: exactly two single-agent passes (plan, final). No fable fan-out. Everything else opus.

## Definition of done
- [ ] Every success criterion proven with printed evidence.
- [ ] Tree confined to `/Users/pedrocunha/repos/guideme`.
- [ ] `mise run check` green, output captured and printed.
- [ ] Both fable gates run; findings triaged via `receiving-code-review`; deviations from this goal listed.
- [ ] Invariant-compliance wave clean with machine-check output attached.
- [ ] Repo pushed to private `pedro-pscunha/guideme`, HEAD SHA matches remote `main`.
- [ ] Final pass: re-read this goal end to end; the plan written at the start of execution (`docs/superpowers/plans/…`) is fulfilled task by task, or a task is explicitly descoped in the report.

## /goal completion condition
```text
/goal Satisfy every success criterion and Definition-of-done item in docs/superpowers/goals/2026-09-20-guideme-rust-core.md. Write the plan first (writing-plans), gate it with one fable brooks-review, then execute. As the final step, print the full proof state in one block: the captured `mise run check` log with EXIT=0, `cargo nextest list | wc -l`, the `rg` invariant checks, `gh repo view pedro-pscunha/guideme --json visibility,defaultBranchRef`, `git rev-parse HEAD` vs `git ls-remote --heads origin main`, and the invariant-compliance wave's per-invariant verdict plus Critical/Major counts from both fable reviews. Only touch /Users/pedrocunha/repos/guideme/**. Stop when all pass or after 40 turns; if blocked, stop and report blockers and open questions — do not ask mid-run.
```

## Deliverable
Private repo `pedro-pscunha/guideme` on `main`, gate green, both fable reviews applied, `spec/` and docs committed.
