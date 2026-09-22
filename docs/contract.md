# The guideme contract

Every guideme SDK is written from scratch in its own language. What they share is this
contract, published by this repository under `spec/`. An SDK is a guideme SDK when it
satisfies the four parts below.

## 1. Wire fidelity

`spec/schema/request.json` and `spec/schema/response.json` are the exact shapes of
`POST /v1/systemone`. Request and response types must validate against them. Probabilities
and confidences carry `minimum: 0, maximum: 1` and must be rejected outside that interval at
parse time. Score `legend` and `probabilities` are keyed by level index serialised as strings
(`"0"`, `"1"`, and so on).

## 2. Policy conformance

Implement `resolve(answer, thresholds) -> outcome` as a pure function. For every entry in
`spec/vectors/policy.json`:

- if the entry has `outcome`, `resolve(entry.answer, entry.thresholds)` must equal it by JSON
  equality; `schema/outcome.json` and `schema/thresholds.json` give the shapes;
- if the entry has `error: "protocol"`, `resolve` must reject the answer.

The rules, in words:

- Noul: `p >= yes_above` is `yes`; else `p <= no_below` is `no`; else `unsure`.
- Choice: `unsure = confidence < min_confidence`; `ranked` lists options by descending
  probability, ties by key; the API's `choice` must be present in the distribution.
- Score: `index` is the argmax, ties to the lowest index; `value` is the API's `score`;
  `unsure` as above; the answer's `legend` and `probabilities` must be the same contiguous
  `0..n` with `2 <= n <= 10`, and `score` must lie within `0..=n-1`. The outcome's
  `distribution` and `legend` are positional arrays: element `i` is level `i`.

Thresholds are valid when every field is in `0..=1` and `no_below <= yes_above`. Policy
patches merge question over guide over the defaults
`{yes_above: 0.5, no_below: 0.5, min_confidence: 0.0}`.

## 3. Rubric rendering

An option or a level may carry **examples** — inputs that belong to it — and an option may
carry **counterexamples**, inputs that do not. They are composed into the rubric string the
API already takes rather than sent as the API's structured `criteria` objects;
`docs/design.md` records why. The composition is byte-for-byte shared:

```
render(what, examples, counterexamples) -> string:
    if examples is empty and counterexamples is empty:
        return what
    lines = [what]
    if examples:
        lines.append("Examples: " + join(examples, "; "))
    if counterexamples:
        lines.append("Not this option: " + join(counterexamples, "; "))
    return join(lines, "\n")
```

Clauses are joined with a newline, items within a clause with `"; "`. No terminal punctuation
is added, and `what` is used verbatim: never trimmed, never re-punctuated.

**The load-bearing invariant:** with no examples and no counterexamples the output is `what`
itself. A rubric written as a bare string puts the same bytes on the wire as it did in 0.1.0.

`spec/vectors/rubric.json` is the golden set; every case carries `what`, `examples`,
`counterexamples` and the `rendered` result, and every SDK must reproduce each `rendered`
exactly. A level never renders a counterexample clause: a level is a position on a scale, not
an option to rule out, and an example under a level *is* the statement that such an input
scores there — no numeric annotation is added to the text.

Where the rendering happens is each language's business, and the SDKs differ on purpose. Rust
renders inside `#[derive(Choice)]` / `#[derive(Levels)]` at expansion time, so `choose_among`
and `score_levels` take an already-rendered string. Python renders where a rubric becomes wire
text, so `choose_among()` and `score_levels()` accept an `option()` / `level()` value directly.
Equivalent inputs produce identical wire bytes either way; only the convenience differs.

Declaration-time validation is shared: an empty or whitespace-only rubric, an empty example or
counterexample, and a duplicate string within one option's examples or counterexamples are all
rejected loudly. The same string as an example of one option and a counterexample of another is
legitimate, and is exactly the confusable-options pattern this feature exists for.

## 4. Interface shape

Mirror the verbs, in the idiom of the language:

- constructors for a yes/no question, a choice over an enum, a score over an ordered enum,
  a choice over runtime options, and a score over runtime levels;
- a way to attach examples to an option or a level, and counterexamples to an option;
- question methods to patch the policy, set `yes_above` and `no_below` on nouls, set
  `min_confidence` on choice and score, set a fallback value, describe what yes and no mean
  on a noul, and switch to the detailed reading;
- one `ask(shape, state)` entry point where a shape is a question, a tuple or list of shapes,
  or a map of shapes, answered in one request with ids `q0..qN` in encounter order; the result
  has the same shape;
- unsure resolution in this order: the question's fallback value, the enum's fallback
  member, then a typed unsure error; the detailed reading never fails on unsure;
- telemetry with the names in `docs/observability.md`: one `guideme.ask` span per `ask` with
  the `gen_ai.*` attributes, one HTTP client span per attempt, one `guideme.answer` event per
  answer, and `error.type` from the same list of names. The names are the contract so that
  one dashboard reads every SDK; how they are emitted is each language's business.

Runtime option sets should return a caller-chosen type where the language allows it.
Exhaustive matching over the enum is enforced where the language can; elsewhere, a literal
union plus a runtime check on the wire key.
