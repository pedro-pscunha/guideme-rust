# guideme

[![crates.io](https://img.shields.io/crates/v/guideme.svg)](https://crates.io/crates/guideme)
[![docs.rs](https://docs.rs/guideme/badge.svg)](https://docs.rs/guideme)
[![license](https://img.shields.io/crates/l/guideme.svg)](#license)

Judgments from [TypeSafe Jev](https://docs.typesafe.ai) that read like Rust control flow.

A yes/no question is an `if`. A choice is an exhaustive `match` over your own enum. A score is
a comparison against your own ordered levels. Thresholds, unsure bands and fallbacks are
explicit and composable. Every request is one tracing span. The decision logic is pure and
its contract is published under `spec/`, so every guideme SDK, in any language, answers the
same way.

```rust,no_run
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

async fn triage(ticket: &str) -> Result<(), guideme::Error> {
    let guide = Guide::from_env()?; // reads TYPESAFE_API_KEY

    if guide.ask(noul("Should this ticket be escalated?"), ticket).await? {
        // escalate
    }

    match guide.ask(choose::<Department>("Which team should handle this?"), ticket).await? {
        Department::Billing => {}
        Department::Technical => {}
        Department::Sales => {} // also the answer when confidence is below the floor
    }

    if guide.ask(score::<Frustration>("How frustrated is the customer?"), ticket).await?
        >= Frustration::Frustrated
    {
        // prioritise
    }
    Ok(())
}
```

The doc comment on each variant is the rubric the model reads. The variant name in
`snake_case` is the wire key. The compiler enforces that every option is handled.

## Examples in a rubric

A description alone leaves confusable options to a coin flip. Name the inputs that belong to
an option, and the ones that do not:

```rust
use guideme::Choice;

#[derive(Choice, Clone, Copy, PartialEq, Eq, Debug)]
enum Department {
    /// Payments, invoicing, refunds
    #[guide(example = "My card was charged twice", example = "Where is my refund?")]
    #[guide(counterexample = "The dashboard is down")]
    Billing,
    /// Bugs, outages, integrations
    #[guide(example = "502 on every request")]
    Technical,
    /// Pricing, upgrades, new accounts
    #[guide(fallback, example = "Do you have a team plan?")]
    Sales,
}
```

`Billing` reaches the wire as one string:

```text
Payments, invoicing, refunds
Examples: My card was charged twice; Where is my refund?
Not this option: The dashboard is down
```

Both keys are repeatable and compose with `rubric`, `key` and `fallback`. A variant with
neither renders to its rubric unchanged, byte for byte, so nothing you wrote before moves.

`example` works on `#[derive(Levels)]` too, where an example *is* the statement that such an
input scores at that level — its position on the scale carries the number, so nothing is added
to the text. `counterexample` is a choice key only: a level is a position on a scale, not an
option to rule out, and asking for one is a compile error. So are an empty or duplicated
example, an example on a variant with no rubric to attach it to, and an example that claims an
input belongs to two options at once. The same string as an example of one option and a
counterexample of another is exactly the point, and stays legal.

A noul has no enum to hang attributes off, so it takes `Rubric`, which composes the same way:

```rust,no_run
use guideme::{noul, Guide, Rubric};

async fn urgent(guide: &Guide, ticket: &str) -> Result<bool, guideme::Error> {
    guide.ask(
        noul("Is this ticket urgent?").criteria(
            Rubric::new("Something is broken now and nobody can work around it")
                .example("the checkout page is down")
                .counterexample("a nightly job failed and we pull the numbers by hand for now"),
            Rubric::new("It can wait for the next working day"),
        ),
        ticket,
    ).await
}
```

This is where examples earn the most: on that ticket, plain criteria answer 0.75 and these
answer 0.25 — and 0.25 is right. `criteria` accepts a description or a `Rubric`, and a
description is anything `String` converts from, so a plain pair of strings keeps working
unchanged.

## Install

```sh
cargo add guideme
cargo add tokio --features rt-multi-thread,macros
```

or in `Cargo.toml`:

```toml
[dependencies]
guideme = "0.2"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
```

Rust 1.98 or newer. `#[derive(Choice)]` and `#[derive(Levels)]` come with the crate; you
never depend on `guideme-derive` yourself.

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

The runtime pair takes a description or a `Rubric` in every rubric position, so options and
levels that come from a database carry examples the same way a derived enum does:

```rust,no_run
use guideme::{choose_among, Guide, Key, Rubric};

async fn desk(guide: &Guide, ticket: &str) -> Result<Key, guideme::Error> {
    guide.ask(choose_among("Which desk?", [
        ("returns", Some(Rubric::new("Whether an item can be returned")
            .example("Can I return these?")
            .counterexample("Has my return arrived yet?"))),
        ("tracking", Some(Rubric::new("Progress of a return already sent")
            .example("Has my return arrived yet?"))),
    ]), ticket).await
}
```

One list has one rubric type, so a list where every option is bare needs it named once:
`[("a", None::<&str>), ("b", None)]`. The rules are checked when the question is asked, and
they are the derives' rules: a shared example, an empty or duplicated one, a counterexample on
a level. A declaration the derive accepts is accepted here, and one it rejects is rejected
here.

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

```rust,no_run
use guideme::{noul, ApiKey, Guide, Policy, Verdict};

const CAUTIOUS: Policy = Policy::new().yes_above(0.7).no_below(0.3);

async fn route(key: ApiKey, ticket: &str) -> Result<(), guideme::Error> {
    let guide = Guide::builder().api_key(key).policy(CAUTIOUS).build()?;
    let strict = guide.with_policy(Policy::new().min_confidence(0.8))?;

    match strict.ask(noul("Is this about billing?").detail(), ticket).await? {
        Verdict::Yes(_) => {}                                 // billing
        Verdict::No(_) => {}                                  // everything else
        Verdict::Unsure(p) => println!("a human decides: {}", p.get()),
    }
    Ok(())
}
```

## Several judgments, one request

A tuple of questions is a question. So is a `Vec` or a `BTreeMap`, and they nest. The answer
has the same shape, from one request and one span. Each question keeps its own policy.

```rust,no_run
# use guideme::{Choice, Levels};
# #[derive(Choice, Clone, Copy, PartialEq, Eq, Debug)]
# enum Department { /** Payments */ Billing, /** Bugs */ Technical }
# #[derive(Levels, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
# enum Frustration { /** Calm */ Calm, /** Angry */ Angry }
use std::collections::BTreeMap;

use guideme::{choose, noul, score, Guide};

async fn triage(guide: &Guide, ticket: &str) -> Result<(), guideme::Error> {
    let labels = BTreeMap::from([
        ("spam", noul("Is it spam?")),
        ("vip", noul("Is the sender a VIP?")),
    ]);

    let (urgent, dept, mood, flags) = guide.ask((
        noul("Is this urgent?").yes_above(0.7).no_below(0.3).or(false),
        choose::<Department>("Which team?").min_confidence(0.6),
        score::<Frustration>("How frustrated?").detail(),
        labels,                                      // comes back as BTreeMap<&str, bool>
    ), ticket).await?;

    if urgent || mood.value > 1.5 || flags["vip"] {
        // prioritise
    }
    Ok(())
}
```

Question ids are `q0..qN` in encounter order; they appear on the wire, in errors and in
events. A batch is atomic: one answer that cannot be resolved fails the whole call, so put
`.or(..)` or `.detail()` on the questions that may come back unsure.

## Observability

guideme emits `tracing` spans and events and installs nothing: no subscriber, no file, no
exporter. Add a subscriber and it appears. The smallest one:

```rust,no_run
tracing_subscriber::fmt().with_env_filter("warn,guideme=info").init();
```

One `tracing` span named `guideme.ask` per request, shaped by the OpenTelemetry GenAI
conventions: `gen_ai.request.model`, `gen_ai.response.model`, `gen_ai.usage.*`, and on
failure `error.type` with an error status. Under it, one HTTP client span per attempt with
`http.response.status_code`, so a retry is visible as sibling spans, plus a `WARN` event when
an attempt is throttled. One `guideme.answer` event per question with the outcome, the
probability or confidence, the unsure verdict and the settled thresholds that produced it.
The state is never recorded unless you opt in with `record_state(true)`. The API key never
appears anywhere.

Because the shapes are standard, any OTLP backend reads them as is, and events exported as
OTLP log records carry the trace and span id of the ask they belong to. `docs/observability.md`
has the field tables, the `RUST_LOG` matrix, the environment variables that point the
exporter anywhere, and console and OTLP setups. `examples/otlp` runs all of it against the
live API with a collector that prints what arrives.

## Errors

One enum, `guideme::Error`, for everything:

| Variant | When |
|---|---|
| `Auth` | 401 |
| `Invalid { detail }` | 422, body included |
| `RateLimited { retry_after }` | 429 after retries, or a `retry-after` too long to wait for |
| `Overloaded { retry_after }` | 529, same |
| `Transport(..)` | connection, TLS, timeout |
| `UnexpectedStatus { status, body }` | anything the contract does not define |
| `Protocol { detail }` | the response violates the contract: undecodable body, wrong answer kind, option or level not in the rubric, probability outside 0..1 |
| `Unsure { question, value, threshold }` | the policy said unsure and nothing caught it |
| `Config { detail }` | bad thresholds, missing key, empty batch, unserialisable state, empty or duplicate rubric, a setting that an injected client already carries |

Retries use exponential backoff with jitter, capped at 30 s, and honour `retry-after`. What is
retried: 429, 529, and a request that never reached a server — a refused or reset connection, a
TLS handshake failure. Both endpoints, so a throttle on a startup `models()` call does not fail
the boot. What is not: **a timeout of any phase**, or a body failure. `timeout` is one deadline
over the whole attempt, so a connect-phase timeout cannot be told apart from a read timeout,
and retrying either would multiply the wall time that setting promises.

Hand in your own client with `http(..)` and the classification is that client's: guideme still
retries what `reqwest` reports as a connection failure, so a `connect_timeout` set on your
client makes connect timeouts retryable. guideme never sets one.

## The receipt

`ask` returns the answer. `ask_with_receipt` returns the same answer plus what the response
said about itself: the versioned model that produced it, and the tokens it cost.

```rust,no_run
# use guideme::Choice;
# #[derive(Choice, Clone, Copy, PartialEq, Eq, Debug)]
# enum Department { /** Payments */ Billing, /** Bugs */ Technical }
use guideme::{choose, Guide};

async fn cost(guide: &Guide, ticket: &str) -> Result<Department, guideme::Error> {
    let receipt = guide.ask_with_receipt(choose::<Department>("Which team?"), ticket).await?;
    // input tokens are the billed ones; the model is the version that answered
    println!("{} tokens from {}", receipt.usage.input_tokens, receipt.model.as_str());
    Ok(receipt.answer)
}
```

Same request, same span, same fields. `ask` is this with everything but the answer dropped.

## Testing your code

Point the guide at a mock server and your control flow runs without a network or an API key.

```rust
use guideme::{noul, Guide};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const URGENT: &str = r#"{
  "model": "jev-1.13.0",
  "answers": { "q0": { "type": "noul", "noul": 0.95 } },
  "usage": { "input_tokens": 307, "output_tokens": 20 }
}"#;

#[tokio::test]
async fn an_urgent_ticket_is_escalated() -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_string(URGENT))
        .mount(&server)
        .await;

    let guide = Guide::builder()
        .api_key("test-key".into())
        .base_url(server.uri())
        .build()?;

    assert!(guide.ask(noul("Should this be escalated?"), "payouts down").await?);
    Ok(())
}
```

`wiremock = "0.6"` goes in your `[dev-dependencies]`; guideme does not pull it in for you.

Question ids are `q0..qN` in encounter order, so a batch answers `q0`, `q1` and so on in the
order you wrote it. `server.received_requests()` is how you assert on what was sent.

For a proxy, a client certificate, or a transport shared with the rest of the application,
hand in the client instead:

```rust
use std::time::Duration;

use guideme::api::{reqwest, Client};
use guideme::Guide;

fn configured() -> Result<Guide, Box<dyn std::error::Error>> {
    let http = reqwest::Client::builder().timeout(Duration::from_secs(10)).build()?;
    let client = Client::builder("key".into())
        .base_url("https://api.typesafe.ai")
        .http(http)
        .build()?;
    Ok(Guide::builder().client(client).build()?)
}
```

`guideme::api::reqwest` is the `reqwest` guideme links, re-exported so you do not add a
dependency of your own and do not have to keep a version in step: two `reqwest` majors in one
tree are two unrelated `Client` types and the call would not compile. The flip side is that a
`reqwest` major bump is a breaking change for guideme.

Everything the client carries — the key, the base URL, the retry budget, the backoff, the
timeout — is refused by name if you also set it on the guide builder, so a setting never
quietly does nothing.

## Lower layers

- `guideme::api` is the exact wire mirror of `POST /v1/systemone` and `GET /v1/models`, plus
  `Client` for callers who want to build requests themselves.
- `guideme::policy::resolve(&Answer, Thresholds) -> Outcome` is the pure decision function.
  `spec/` holds its JSON Schemas, 42 golden policy vectors and the rubric rendering cases;
  `docs/contract.md` states what every guideme SDK must satisfy. `docs/design.md` records the
  design and its sharp edges.

## Other SDKs

Every guideme SDK is written from scratch in its own language and answers the same way,
because they all satisfy the contract this repository publishes under `spec/` and states in
[`docs/contract.md`](docs/contract.md): the wire schemas, the 42 golden policy vectors, the
rubric rendering, and the interface shape.

| Language | Package | Repository |
|---|---|---|
| Rust | [`guideme`](https://crates.io/crates/guideme) | this repository |
| Python | [`guideme`](https://pypi.org/project/guideme/) | [guideme-python](https://github.com/pedro-pscunha/guideme-python) |

The Python SDK mirrors the verbs in Python's idiom: `Choice` and `Levels` are `enum.Enum`
bases whose members carry the rubric, `.or(value)` is `.otherwise(value)` because `or` is a
keyword, and `Guide` and `AsyncGuide` share one surface. It emits the same span, event and
attribute names, so one dashboard reads both.

## Environment

| Variable | Meaning |
|---|---|
| `TYPESAFE_API_KEY` | required by `Guide::from_env` |
| `TYPESAFE_BASE_URL` | optional API origin override |
| `GUIDEME_MODEL` | optional model or alias; default `jev-latest` |

`Guide::from_env()?` is the one-liner. `Guide::builder().from_env()?` reads the same three
variables onto a builder you are still configuring, so a house policy and an environment key
compose: `Guide::builder().from_env()?.policy(CAUTIOUS).build()?`.

The rest of the builder: `model`, `policy`, `max_retries` (default 3), `backoff` (default
500 ms, the base of the exponential), `timeout` (default 30 s, per attempt), `record_state`,
and `client` for an injected transport.

## Development

Tooling is managed by [mise](https://mise.jdx.dev); `mise install` fetches gitleaks,
cargo-nextest and cargo-deny. The toolchain is pinned in `rust-toolchain.toml`.

```sh
mise run check    # fmt-check, clippy -D warnings, nextest, doctests, rustdoc, cargo-deny
mise run test     # nextest + doctests
mise run spec     # regenerate spec/ after changing api, policy, or the vector grid
mise run hooks    # point core.hooksPath at the tracked hooks in .githooks
```

The hooks are tracked, not generated: `mise run hooks` sets this repository's
`core.hooksPath` to `.githooks` and verifies it took effect. `AGENTS.md` says what each stage
runs.

Library code is held to a strict lint set: pedantic clippy, with `unwrap`, `expect`, `panic`,
`dbg` and `todo` denied. Tests are few and high-grade: property tests for the policy laws, a
local mock server for the wire and retry contract, structural tracing assertions, a
compile-fail suite for the derives, and a drift guard that re-resolves every golden vector.

Two opt-in tests hit the real API and are skipped by default:

```sh
TYPESAFE_API_KEY=… cargo nextest run -p guideme --test live --run-ignored ignored-only --no-capture
```

Contributor rules live in `AGENTS.md`. Report a vulnerability privately, as `SECURITY.md`
describes, never in a public issue.

## License

MIT or Apache-2.0, at your option.
