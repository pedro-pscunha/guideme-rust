# guideme (Rust)

[![crates.io](https://img.shields.io/crates/v/guideme.svg)](https://crates.io/crates/guideme)
[![docs.rs](https://docs.rs/guideme/badge.svg)](https://docs.rs/guideme)
[![license](https://img.shields.io/crates/l/guideme.svg)](#license)

guideme is a Rust library for [TypeSafe Jev](https://docs.typesafe.ai), the TypeSafe model that
gives judgments. You send a question and your state, and you get the answer back as a normal
Rust value. A yes/no question gives a `bool`. A choice gives a variant of your own enum, and a
score gives one of your own ordered levels.

## Install

```sh
cargo add guideme
cargo add tokio --features rt-multi-thread,macros
```

guideme needs Rust 1.98 or newer. The crate includes `#[derive(Choice)]` and
`#[derive(Levels)]`, so do not add `guideme-derive` yourself. Set the API key in the
`TYPESAFE_API_KEY` environment variable, or give it to `Guide::builder().api_key(..)`.

## Quick start

```rust,no_run
use guideme::{choose, noul, Choice, Guide};

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

async fn triage(ticket: &str) -> Result<(), guideme::Error> {
    let guide = Guide::from_env()?; // reads TYPESAFE_API_KEY

    if guide.ask(noul("Should this ticket be escalated?"), ticket).await? {
        // escalate
    }

    match guide.ask(choose::<Department>("Which team should handle this?"), ticket).await? {
        Department::Billing => {}
        Department::Technical => {}
        Department::Sales => {} // also the answer when the model is not sure
    }
    Ok(())
}
```

The doc comment on each variant is its *rubric*: the text that tells the model what the option
means. The variant name in `snake_case` is the key of the option on the wire. The compiler makes
sure that the `match` handles every option.

When your program starts, build one guide. Then share it in the whole program. A `Guide` is
cheap to clone, and the clones share one HTTP client.

## Questions

A yes/no question is what TypeSafe calls a *noul*, and `noul` is its type on the wire. A choice
has one of your options as its answer. A score has one level of your ordered scale as its answer.

| Constructor | Asks | Plain answer | Answer with `.detail()` |
|---|---|---|---|
| `noul("…")` | a yes/no question | `bool` | `Verdict::{Yes, No, Unsure}(p)` |
| `choose::<C>("…")`, `C: Choice` | a choice over the variants of `C` | `C` | `Ranked<C>`: confidence and all probabilities |
| `score::<L>("…")`, `L: Levels` | a score over the levels of `L` | `L`, the most probable level | `Scored<L>`: the expected `value`, the level, confidence and distribution |
| `choose_among("…", options)` | a choice over `(key, rubric)` pairs that you give at run time | `Key`, the option key | `Ranked<Key>` |
| `score_levels("…", levels)` | a score over level rubrics that you give at run time | `Rank`, the level position from 0 | `Scored<Rank>` |

The levels of a score go from low to high in declaration order. Derive `PartialOrd` and `Ord` to
compare them:

```rust,no_run
use guideme::{score, Guide, Levels};

#[derive(Levels, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Frustration {
    /// Calm and polite
    Calm,
    /// Frustrated
    Frustrated,
    /// Very angry
    VeryAngry,
}

async fn is_upset(guide: &Guide, ticket: &str) -> Result<bool, guideme::Error> {
    let mood = guide.ask(score::<Frustration>("How frustrated is the customer?"), ticket).await?;
    Ok(mood >= Frustration::Frustrated)
}
```

A yes/no question can carry `.criteria("what yes means", "what no means")`. The instructions of
a question are a string or a `serde_json::Value`. With a `Value`, a question can name fields of
a structured state, as the TypeSafe docs describe.

The *state* is the data that you send with the question. It can be a text literal, a `String`,
a `serde_json::Value`, or a reference to any type that implements `Serialize`. An owned struct
is not accepted, so pass a reference. If the state does not convert to JSON (a map with keys
that are not strings, for example), `ask` returns `Error::Config`.

`guide.models()` returns the models that your account can use.

## When the model is not sure

*Unsure* means that the answer is not certain enough under the thresholds. The *policy* is the
set of thresholds. A threshold on the question wins over the guide, and the guide wins over the
defaults.

| Layer | How to set it |
|---|---|
| question | `.yes_above(p)` and `.no_below(p)` on a yes/no question, `.min_confidence(c)` on a choice or a score, `.with(Policy)` on any question |
| guide | `Guide::builder().policy(..)`, or `guide.with_policy(..)?` for a copy with a different policy |
| defaults | `yes_above` 0.5, `no_below` 0.5, `min_confidence` 0.0 |

- Yes/no question: `p >= yes_above` is yes, `p <= no_below` is no, and a value between the two
  is unsure. With the defaults, no answer is unsure.
- Choice and score: `confidence < min_confidence` is unsure. With the default, no answer is
  unsure.
- A threshold outside 0..1, or `no_below` above `yes_above`, is `Error::Config`.

An unsure answer goes down the *unsure ladder*. The first step that applies wins:

1. The value from `.or(value)` on the question.
2. The *fallback*: the variant marked `#[guide(fallback)]`. Only a choice can declare one. For
   a score, use `.or(level)`.
3. `Error::Unsure`, with the question id, the value and the threshold that it did not reach.

`.detail()` skips the ladder and gives you the full reading, so it never fails on an unsure
answer. It also removes an `.or(..)` that you set before it. The policy setters are `const fn`,
so a house policy can be a constant:

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

## Examples and counterexamples

A description alone can leave two similar options to chance. An *example* is an input that
belongs to an option. A *counterexample* is an input that does not.

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

The rubric of `Billing` goes on the wire as one string:

```text
Payments, invoicing, refunds
Examples: My card was charged twice; Where is my refund?
Not this option: The dashboard is down
```

- `example` and `counterexample` can repeat. They combine with `rubric`, `key` and `fallback`.
- A rubric with no examples and no counterexamples goes on the wire unchanged, byte for byte.
- A level can have examples, but not a counterexample.
- An input cannot be an example of two options, or an example and a counterexample of one option.
- An example cannot be empty, repeat, contain a line break, or sit on a variant with no rubric.
- An input can be an example of one option and a counterexample of another. This separates two
  options that are easy to confuse.

The derives refuse a broken rule at compile time. When you ask the question, `.criteria`,
`choose_among` and `score_levels` apply the same rules and return `Error::Config`. These three
take a `Rubric` or a plain description (anything that converts to `String`) as a rubric:

```rust,no_run
use guideme::{noul, Guide, Rubric};

async fn urgent(guide: &Guide, ticket: &str) -> Result<bool, guideme::Error> {
    let question = noul("Is this ticket urgent?").criteria(
        Rubric::new("Something is broken now and nobody can work around it")
            .example("the checkout page is down")
            .counterexample("a nightly job failed and we pull the numbers by hand for now"),
        Rubric::new("It can wait for the next working day"),
    );
    guide.ask(question, ticket).await
}
```

One list has one rubric type. If no option in a list has a rubric, name the type once:
`[("a", None::<&str>), ("b", None)]`.

## Several questions in one request

A tuple of questions is also a question. So is a `Vec` or a `BTreeMap` of questions, and they
can nest. The answer has the same shape, and it comes from one request in one span. Each
question keeps its own policy.

```rust,no_run
use std::collections::BTreeMap;

use guideme::{choose_among, noul, score_levels, Guide};

async fn triage(guide: &Guide, ticket: &str) -> Result<(), guideme::Error> {
    let flags = BTreeMap::from([("spam", noul("Is it spam?")), ("vip", noul("Is it a VIP?"))]);
    let desks = [("returns", Some("Returns of items")), ("tracking", Some("Where a parcel is"))];

    let (urgent, desk, mood, flags) = guide.ask((
        noul("Is this urgent?").yes_above(0.7).no_below(0.3).or(false),
        choose_among("Which desk?", desks).min_confidence(0.6),
        score_levels("How frustrated?", ["calm", "annoyed", "angry"]).detail(),
        flags, // comes back as BTreeMap<&str, bool>
    ), ticket).await?;

    if urgent || mood.value > 1.5 || flags["vip"] {
        println!("prioritize, send to {}", desk.0);
    }
    Ok(())
}
```

The question ids are `q0`, `q1` and so on, in the order that you wrote the questions. The ids
appear on the wire, in errors and in events. A batch is atomic. If one answer cannot be
resolved, the whole call fails. Put `.or(..)` or `.detail()` on each question that can be unsure.

## The receipt

`ask_with_receipt` sends the same request and records the same span as `ask`. It returns a
`Receipt`: the answer, the versioned model that answered, and the token usage. If you asked for
an alias, the model is still a version such as `jev-1.13.0`. Input tokens are the billed ones.

```rust,no_run
async fn escalate(guide: &guideme::Guide, ticket: &str) -> Result<bool, guideme::Error> {
    let question = guideme::noul("Should this be escalated?");
    let receipt = guide.ask_with_receipt(question, ticket).await?;
    println!("{} input tokens from {}", receipt.usage.input_tokens, receipt.model.as_str());
    Ok(receipt.answer)
}
```

## Errors

Every function returns `guideme::Error`. `error.kind()` gives the *error kind*, a short stable
name that is also the `error.type` of a failed span. `Error` is `#[non_exhaustive]`, so a
`match` on it needs a `_` arm.

| Variant | Error kind | When |
|---|---|---|
| `Auth` | `auth` | `401`: the API key is missing or not valid |
| `Invalid { detail }` | `invalid` | `422`, with the response body |
| `RateLimited { retry_after }` | `rate_limited` | `429` after the last retry, or a `retry-after` of more than 30 s |
| `Overloaded { retry_after }` | `overloaded` | `529` after the last retry, or a `retry-after` of more than 30 s |
| `Transport(..)` | `transport` | connection, TLS, timeout, or reading the body |
| `UnexpectedStatus { status, body }` | `unexpected_status` | a status that the contract does not define |
| `Protocol { detail }` | `protocol` | the response breaks the contract: a body that does not decode, the wrong answer kind, an option or level not in the rubric, a probability outside 0..1, a missing answer |
| `Unsure { question, value, threshold }` | `unsure` | the answer is unsure and nothing on the unsure ladder caught it |
| `Config { detail }` | `config` | bad thresholds, no API key, an empty batch, a state that does not convert to JSON, a rubric that breaks a rule, a base URL with no host or with credentials, a setting that an injected client already carries |

## Retries and timeouts

guideme retries `429`, `529`, and a request that did not reach a server. A refused or reset
connection and a failed TLS handshake are requests of that type. This applies to both
endpoints, so a `429` on a `models()` call at startup does not stop your program. guideme does
not retry a timeout of any phase, or a failure to read the body. [`docs/contract.md`](docs/contract.md) gives the reason.

The wait before a retry is `backoff × 2^attempt`, plus up to 250 ms of jitter, with a maximum of
30 s. If the API sends a `retry-after` in whole seconds, guideme waits that long instead. If the
`retry-after` is more than 30 s, the call fails at once, and the error carries that duration.
`timeout` is one deadline for each attempt, so the longest call takes about
`(max_retries + 1) × timeout`, plus the waits.

If you give guideme your own `reqwest::Client`, that client classifies its errors. guideme still
retries what `reqwest` reports as a failure to connect. So a `connect_timeout` on your client
makes a connect timeout a retry. guideme itself never sets a `connect_timeout`.

## Testing your code

Point the guide at a mock server. Then your control flow runs without a network or an API key.

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

Add `wiremock = "0.6"` to your `[dev-dependencies]`. guideme does not add it for you. In the
mock body, the answer ids are `q0`, `q1` and so on, in question order. Use
`server.received_requests()` to make sure that guideme sent the correct request.

To replace the *transport*, give guideme your own `reqwest::Client`. Do this for a proxy, a
client certificate, or a connection pool that the rest of your program shares:

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

Set the timeout on the `reqwest` client. `timeout(..)` beside `http(..)` is `Error::Config`. The
`Client` carries the API key, the base URL, the retries, the backoff and the timeout. If you
also set one of these on `Guide::builder()`, `build` returns `Error::Config` with its name. For
this reason, `Guide::builder().from_env()` does not combine with `client(..)`.

## Observability

guideme emits `tracing` spans and events. It installs no subscriber, no file and no exporter, so
you see nothing until you add a subscriber. This is the smallest one:

```rust,no_run
tracing_subscriber::fmt().with_env_filter("warn,guideme=info").init();
```

Each request is one `guideme.ask` span with the OpenTelemetry GenAI fields. Under it, each HTTP
attempt is one span, with a `guideme.retry` warning before each wait. Each question gives one
`guideme.answer` event with the outcome and the thresholds. The state is recorded only with
`record_state(true)`. The API key never appears.
[`docs/observability.md`](docs/observability.md) has the field tables, the `RUST_LOG` matrix,
and console and OTLP setups. `examples/otlp` runs it all against the live API.

## Configuration

Each *setting* is a method on `Guide::builder()`.

| Setting | Default | Meaning |
|---|---|---|
| `api_key(key)` | none | required, unless `from_env` or `client` gives the key |
| `base_url(url)` | `https://api.typesafe.ai` | the API origin |
| `model(Model)` | `jev-latest` | the model or alias |
| `policy(Policy)` | the defaults above | the policy for every question of this guide |
| `max_retries(n)` | 3 | retries for `429`, `529` and a failure to connect |
| `backoff(d)` | 500 ms | the base of the exponential wait |
| `timeout(d)` | 30 s | the deadline for each attempt |
| `record_state(on)` | `false` | record the state on the span |
| `client(Client)` | none | a `guideme::api::Client` with your own transport |

| Environment variable | Meaning |
|---|---|
| `TYPESAFE_API_KEY` | the API key, required by `from_env` |
| `TYPESAFE_BASE_URL` | optional, replaces the API origin |
| `GUIDEME_MODEL` | optional, the model or alias |

`Guide::from_env()?` reads the three variables and builds the guide. `Guide::builder().from_env()?`
reads them onto a builder that you continue to configure:
`Guide::builder().from_env()?.policy(CAUTIOUS).build()?`. `build` validates the policy, so a bad
house policy fails at startup. Build one guide for each process and share it.

## Lower layers

- `guideme::api` is the exact wire mirror of `POST /v1/systemone` and `GET /v1/models`, plus
  `Client` for callers who build requests themselves.
- `guideme::api::reqwest` is the `reqwest` that guideme links. Use it, and you need no
  `reqwest` dependency of your own. Two major versions of `reqwest` give two unrelated `Client`
  types, and the call does not compile. So a major `reqwest` update is a breaking change for guideme.
- `guideme::policy::resolve(&Answer, Thresholds) -> Outcome` is the pure decision function.
- [`docs/design.md`](docs/design.md) records the design decisions, the measurements behind them,
  and the sharp edges.

## Other SDKs

Each guideme SDK is written from scratch in its own language. All three satisfy the contract
that this repository publishes under `spec/` and states in [`docs/contract.md`](docs/contract.md).
The contract covers the wire schemas, the 42 golden policy vectors, the rubric rendering and the
interface shape. So the same response gives the same answer in each SDK. All three use the same
span, event and field names, so one dashboard reads all of them.

| Language | Package | Repository |
|---|---|---|
| Rust | [`guideme`](https://crates.io/crates/guideme) | this repository |
| Python | [`guideme`](https://pypi.org/project/guideme/) | [guideme-python](https://github.com/pedro-pscunha/guideme-python) |
| TypeScript | `@guideme/sdk` (not yet on npm) | [guideme-typescript](https://github.com/pedro-pscunha/guideme-typescript) |

## Development

Tools come from [mise](https://mise.jdx.dev). Run `mise install`, then `mise run hooks` once.
`mise run check` runs the full gate. [`CONTRIBUTING.md`](CONTRIBUTING.md) is the short guide.
[`AGENTS.md`](AGENTS.md) has the full contributor rules, the commands and the live tests. Report a vulnerability privately, as [`SECURITY.md`](SECURITY.md)
describes.

## License

MIT or Apache-2.0, as you choose.
