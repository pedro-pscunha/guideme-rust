# Porting guideme to another language

The Rust crate is the reference. A port is conformant when it satisfies three things.

## 1. Wire fidelity

`spec/schema/request.json` and `spec/schema/response.json` are the exact shapes of
`POST /v1/systemone`. Your request and response types must validate against them.
Probabilities and confidences carry `minimum: 0, maximum: 1` and must be rejected outside
that interval at parse time. Score `legend` and `probabilities` are keyed by level index
serialised as strings (`"0"`, `"1"`, …).

## 2. Policy conformance

Port `guideme::policy::resolve(answer, thresholds) -> outcome` as a pure function. For every
entry in `spec/vectors/policy.json`:

- if the entry has `outcome`, `resolve(entry.answer, entry.thresholds)` must equal it (JSON
  equality; `schema/outcome.json` and `schema/thresholds.json` give the shapes);
- if the entry has `error: "protocol"`, `resolve` must reject the answer.

The rules, in words:

- Noul: `p >= yes_above` → `yes`; else `p <= no_below` → `no`; else `unsure`.
- Choice: `unsure = confidence < min_confidence`; `ranked` is options by descending
  probability, ties by key; the API's `choice` must be present in the distribution.
- Score: `index` is the argmax (ties → lowest); `value` is the API's `score`; `unsure` as
  above; `legend` and `probabilities` must be the same contiguous `0..n` with `2 <= n <= 10`
  and `score` within `0..=n-1`.

Thresholds are valid when every field is in `0..=1` and `no_below <= yes_above`. Policy
patches merge question > guide > defaults `{yes_above: 0.5, no_below: 0.5, min_confidence: 0.0}`.

## 3. Interface shape

Mirror the verbs, not the Rust types:

- constructors `noul(q)`, `choose(Enum, q)`, `score(Enum, q)`, `chooseAmong(q, options)`,
  `scoreLevels(q, levels)`;
- question methods `.with(policy)`, `.yesAbove(p)`, `.noBelow(p)`, `.minConfidence(c)`,
  `.or(value)`, `.detail()`, `.criteria(yes, no)` on noul;
- `guide.ask(shape, state)` where a shape is a question, a tuple/array of shapes, or a map of
  shapes, answered in one request with ids `q0..qN` in encounter order; the result has the
  same shape;
- unsure resolution: `.or()` > enum fallback > error; `.detail()` never fails on unsure;
- one span per `ask` and one event per answer with the fields in `docs/observability.md`.

Runtime option sets should return a caller-chosen type where the language allows it
(TypeScript generics, Python `TypeVar`); Rust's `Key`/`Rank` are the minimal form.
Compile-time `match` exhaustiveness does not port; use a literal union plus a runtime check
in `from_key`.
