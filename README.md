# guideme

Judgments from [TypeSafe Jev](https://docs.typesafe.ai) that read like Rust control flow.

A yes/no question is an `if`. A choice is an exhaustive `match` over your own enum. A score is
a comparison against your own ordered levels. Thresholds, unsure bands and fallbacks are
explicit and composable. Every request is one tracing span. The decision logic is pure and
its contract is published under `spec/`, so every guideme SDK, in any language, answers the
same way.

```rust
use guideme::{choose, noul, score, Choice, Guide, Levels};

#[derive(Choice, Clone, Copy, PartialEq, Eq, Debug)]
enum Department {
    /// Payments, invoicing, refunds
    Billing,
    /// Bugs, outages, integrations
    Technical,
    /// Pricing, upgrades, new accounts
    #[guide(fallback)]
    Sales,
}

#[derive(Levels, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Frustration {
    /// Calm and polite
    Calm,
    /// Frustrated
    Frustrated,
    /// Very angry
    VeryAngry,
}

let guide = Guide::from_env()?; // reads TYPESAFE_API_KEY

if guide.ask(noul("Should this ticket be escalated?"), ticket).await? {
    escalate();
}

match guide.ask(choose::<Department>("Which team should handle this?"), ticket).await? {
    Department::Billing => route_billing(),
    Department::Technical => route_tech(),
    Department::Sales => route_sales(), // also the answer when confidence is below the floor
}

if guide.ask(score::<Frustration>("How frustrated is the customer?"), ticket).await?
    >= Frustration::Frustrated
{
    prioritise();
}
```

The doc comment on each variant is the rubric the model reads. The variant name in
`snake_case` is the wire key. The compiler enforces that every option is handled.

## Install

The crate is private and unpublished. Depend on it by git:

```toml
[dependencies]
guideme = { git = "https://github.com/pedro-pscunha/guideme-rust" }
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
```

Set `TYPESAFE_API_KEY` in the environment, or pass a key to `Guide::builder().api_key(..)`.

## The three questions

| Constructor | Sends | Plain output | `.detail()` output |
|---|---|---|---|
| `noul("…")` | a yes/no question | `bool` | `Verdict::{Yes, No, Unsure}(p)` |
| `choose::<C>("…")` where `C: Choice` | a choice over `C`'s variants | `C` | `Ranked<C>` with confidence and the full distribution |
| `score::<L>("…")` where `L: Levels` | a score over `L`'s levels, low to high | `L`, the most probable level | `Scored<L>` with the expected `value`, the level, confidence and distribution |
| `choose_among("…", options)` | a choice over runtime `(key, rubric)` pairs | `Key` | `Ranked<Key>` |
| `score_levels("…", levels)` | a score over runtime level descriptions | `Rank` | `Scored<Rank>` |

A noul can carry `.criteria("what yes means", "what no means")`. Instructions accept a string
or a `serde_json::Value`, so a question can reference structured data by field name the way
the TypeSafe docs describe.

The state is anything serialisable: a text literal, a `String`, a `serde_json::Value`, or a
reference to your own struct.

## Policy

Thresholds decide how a probability or a confidence becomes an answer. They form a patch that
merges from the question, over the guide, over the crate defaults.

| Layer | How to set | Wins over |
|---|---|---|
| question | `.yes_above(p)`, `.no_below(p)` on nouls; `.min_confidence(c)` on choice and score; `.with(Policy)` on any | guide |
| guide | `Guide::builder().policy(..)`, or `guide.with_policy(..)?` for a scoped copy | defaults |
| defaults | `yes_above 0.5`, `no_below 0.5`, `min_confidence 0.0` | nothing |

The rules:

- Noul: `p >= yes_above` is yes, `p <= no_below` is no, strictly between is unsure. With the
  defaults there is no unsure band.
- Choice and score: `confidence < min_confidence` is unsure. With the default there is never
  an unsure answer.

When an answer is unsure, resolution goes down a ladder: `.or(value)` on the question, then
the enum's `#[guide(fallback)]` variant, then `Error::Unsure` naming the question and the
boundary it missed. `.detail()` skips the ladder and hands you the reading to decide yourself.

House policies are constants, because the setters are `const fn`:

```rust
const CAUTIOUS: Policy = Policy::new().yes_above(0.7).no_below(0.3);

let guide = Guide::builder().api_key(key).policy(CAUTIOUS).build()?;
let strict = guide.with_policy(Policy::new().min_confidence(0.8))?;

match guide.ask(noul("Is this about billing?").detail(), ticket).await? {
    Verdict::Yes(_) => billing(),
    Verdict::No(_) => other(),
    Verdict::Unsure(p) => review(p),
}
```

## Several judgments, one request

A tuple of questions is a question. So is a `Vec` or a `BTreeMap`, and they nest. The answer
has the same shape, from one request and one span. Each question keeps its own policy.

```rust
let labels = BTreeMap::from([
    ("spam", noul("Is it spam?")),
    ("vip", noul("Is the sender a VIP?")),
]);

let (urgent, dept, mood, flags) = guide.ask((
    noul("Is this urgent?").yes_above(0.7).no_below(0.3).or(false),
    choose::<Department>("Which team?").min_confidence(0.6),
    score::<Frustration>("How frustrated?").detail(),
    labels,                                      // comes back as BTreeMap<&str, bool>
), &ticket).await?;

if urgent || mood.value > 1.5 || flags["vip"] {
    prioritise();
}
```

Question ids are `q0..qN` in encounter order; they appear on the wire, in errors and in
events. A batch is atomic: one answer that cannot be resolved fails the whole call, so put
`.or(..)` or `.detail()` on the questions that may come back unsure.

## Observability

One `tracing` span named `guideme.ask` per request, with the model requested and answered,
question count, token usage, retries, elapsed time, and an `error` field when the ask failed.
One `guideme.answer` event per question with the outcome, the probability or confidence, the
unsure verdict and the settled thresholds that produced it. The state is never recorded unless
you opt in with `record_state(true)`. The API key never appears anywhere.

`docs/observability.md` has the field tables, the `RUST_LOG` targets, and a console and OTLP
setup. `examples/otlp` is a runnable version of both against the live API.

## Errors

One enum, `guideme::Error`, for everything:

| Variant | When |
|---|---|
| `Auth` | 401 |
| `Invalid { detail }` | 422, body included |
| `RateLimited { retry_after }` | 429 after retries, or a `retry-after` too long to wait for |
| `Overloaded` | 529 after retries |
| `Transport(..)` | connection, TLS, timeout |
| `UnexpectedStatus { status, body }` | anything the contract does not define |
| `Protocol { detail }` | the response violates the contract: undecodable body, wrong answer kind, option or level not in the rubric, probability outside 0..1 |
| `Unsure { question, value, threshold }` | the policy said unsure and nothing caught it |
| `Config { detail }` | bad thresholds, missing key, empty batch, unserialisable state, empty or duplicate rubric |

Retries on 429 and 529 use exponential backoff with jitter, capped at 30 s, and honour
`retry-after`.

## Lower layers

- `guideme::api` is the exact wire mirror of `POST /v1/systemone` and `GET /v1/models`, plus
  `Client` for callers who want to build requests themselves.
- `guideme::policy::resolve(&Answer, Thresholds) -> Outcome` is the pure decision function.
  `spec/` holds its JSON Schemas and 42 golden vectors; `docs/contract.md` states what every
  guideme SDK must satisfy. `docs/design.md` records the design and its sharp edges.

## Environment

| Variable | Meaning |
|---|---|
| `TYPESAFE_API_KEY` | required by `Guide::from_env` |
| `TYPESAFE_BASE_URL` | optional API origin override |
| `GUIDEME_MODEL` | optional model or alias; default `jev-latest` |

## Development

Tooling is managed by [mise](https://mise.jdx.dev); `mise install` fetches lefthook,
cargo-nextest and cargo-deny. The toolchain is pinned in `rust-toolchain.toml`.

```
mise run check    # fmt-check, clippy -D warnings, nextest, doctests, rustdoc, cargo-deny
mise run test     # nextest + doctests
mise run spec     # regenerate spec/ after changing api, policy, or the vector grid
mise run hooks    # install the git hooks
```

Library code is held to a strict lint set: pedantic clippy, with `unwrap`, `expect`, `panic`,
`dbg` and `todo` denied. Tests are few and high-grade: property tests for the policy laws, a
local mock server for the wire and retry contract, structural tracing assertions, a
compile-fail suite for the derives, and a drift guard that re-resolves every golden vector.

Two opt-in tests hit the real API and are skipped by default:

```
TYPESAFE_API_KEY=… cargo nextest run -p guideme --test live --run-ignored ignored-only --no-capture
```

Contributor rules live in `AGENTS.md`.
