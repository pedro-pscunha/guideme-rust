# guideme

Type-safe inline judgments from [TypeSafe Jev](https://docs.typesafe.ai) for Rust.
A judgment reads like control flow: `if`, `match`, a comparison. Policies (thresholds,
unsure band, fallbacks) are explicit and composable. Every request is one tracing span.

```toml
[dependencies]
guideme = { git = "https://github.com/pedro-pscunha/guideme" }
```

Set `TYPESAFE_API_KEY` in the environment.

```rust
use guideme::{choose, noul, score, Choice, Guide, Levels, Policy};

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

let guide = Guide::from_env()?;

// if — a noul
if guide.ask(noul("Should this ticket be escalated?"), ticket).await? {
    escalate();
}

// match — a choice, exhaustive by construction
match guide.ask(choose::<Department>("Which team should handle this?"), ticket).await? {
    Department::Billing => route_billing(),
    Department::Technical => route_tech(),
    Department::Sales => route_sales(), // also below min_confidence, via #[guide(fallback)]
}

// compare — a score
if guide.ask(score::<Frustration>("How frustrated is the customer?"), ticket).await?
    >= Frustration::Frustrated
{
    prioritise();
}

// three judgments, one request, one span
let (urgent, dept, mood) = guide.ask((
    noul("Is this urgent?").yes_above(0.7).no_below(0.3).or(false),
    choose::<Department>("Which team?").min_confidence(0.6),
    score::<Frustration>("How frustrated?").detail(),
), ticket).await?;
if urgent || mood.value > 1.5 { prioritise(); }
```

## Policy

| Layer | How | Wins over |
|---|---|---|
| question | `.with(Policy)`, `.yes_above(p)`, `.no_below(p)`, `.min_confidence(c)` | guide |
| guide | `Guide::builder().policy(..)`, `guide.with_policy(..)?` (validated on the spot) | defaults |
| defaults | `yes_above 0.5`, `no_below 0.5`, `min_confidence 0.0` | — |

Noul: `p >= yes_above` is yes, `p <= no_below` is no, in between is unsure. Choice and score:
`confidence < min_confidence` is unsure. Unsure resolves by `.or(value)`, then the enum's
`#[guide(fallback)]`, then `Error::Unsure`. `.detail()` returns `Verdict` / `Ranked<C>` /
`Scored<L>` and never fails on unsure.

House policies are constants: `const CAUTIOUS: Policy = Policy::new().yes_above(0.7).no_below(0.3);`

## Shapes

`guide.ask` takes a question, a tuple of up to eight, a `Vec`, or a `BTreeMap`; nesting
works. The answer has the same shape. One request, ids `q0..qN` in encounter order. The call
is atomic: an unresolvable answer fails the whole call.

Runtime rubrics: `choose_among(q, [("key", Some("rubric")), …])` returns `Key`;
`score_levels(q, ["low", "high"])` returns `Rank`.

## Lower layers

- `guideme::api` is the exact wire mirror plus `Client` (retries on `429`/`529`, honours
  `retry-after`, typed errors, `GET /v1/models`).
- `guideme::policy::resolve` is the pure decision function; `spec/` holds its schemas and
  golden vectors. See `docs/porting.md` to build another SDK from this one.

## Environment

| Variable | Meaning |
|---|---|
| `TYPESAFE_API_KEY` | required by `Guide::from_env` |
| `TYPESAFE_BASE_URL` | optional API origin override |
| `GUIDEME_MODEL` | optional model or alias; default `jev-latest` |

## Development

`mise run check` is the gate: fmt, clippy (pedantic, no `unwrap`/`panic` in `src`), nextest,
doctests, rustdoc, cargo-deny. `mise run hooks` installs lefthook stubs into the repo's hooks dir (pre-commit: fmt + clippy;
pre-push: the gate). If a global `core.hooksPath` is set, git runs only that directory: the
pre-commit stub is reached when the global hook chains to it, and pre-push only if the global
directory has a `pre-push` that chains too; otherwise run `mise run check` before pushing. `mise run spec` regenerates `spec/`. The live contract test is opt-in:
`TYPESAFE_API_KEY=… cargo nextest run -p guideme --test live --run-ignored ignored-only`.

Docs: `docs/design.md`, `docs/observability.md`, `docs/porting.md`.
