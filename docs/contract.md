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

An option, a level, or either side of a noul's criteria may carry **examples** — inputs that
belong to it — and an option or a noul criterion may carry **counterexamples**, inputs that do
not. All three question kinds take them: a noul's `true`/`false` criteria are exactly as
confusable as a choice's options, and measured the largest swing of the three. They are composed into the rubric string the
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
is added, and `what` is used verbatim: never trimmed, never re-punctuated. Examples and
counterexamples render in **declaration order**, always — two SDKs ordering differently would
produce different bytes for the same declaration, so this is contract, not presentation. The
`Not this option` label is contract for the same reason; it is also the best of the three
labels measured.

**The load-bearing invariant:** with no examples and no counterexamples the output is `what`
itself. A rubric written as a bare string puts the same bytes on the wire as it did in 0.1.0.

`spec/vectors/rubric.json` is the golden set; every case carries `what`, `examples`,
`counterexamples` and the `rendered` result, and every SDK must reproduce each `rendered`
exactly. Every case also carries `kind` — `"choice"`, `"levels"` or `"noul"` — naming the
question kind the rubric sits in. Nothing rendered from a level's declared parts carries a
counterexample clause, because a level is a position on a scale rather than an option to rule
out; that is why a counterexample on a level is refused outright, below — a compile error on
Rust's `Levels` derive, a refusal in Python's `level()`. The guarantee is over declared parts:
neither SDK can stop a caller pre-rendering a rubric that carries a counterexample and handing
the string to a runtime level constructor, which is the trusted-input boundary above. An
example under a level *is* the statement that such an input scores there — no numeric
annotation is added to the text.

Where the rendering happens is each language's business, and the SDKs differ on purpose. Rust
renders inside `#[derive(Choice)]` / `#[derive(Levels)]` at expansion time, because
`Options::RUBRIC` is a `const` and cannot call a function, and offers `Rubric` for the rubric
positions that are not an enum. Rust has no unchecked way to render one: `Rubric::render`
returns `Result`, and there is deliberately no `Display`. Python renders where a rubric becomes
wire text. Both SDKs' **runtime** constructors take a rubric that carries examples —
`Rubric` in Rust's `choose_among()` and `score_levels()`, `option()` and `level()` in
Python's — so a description and a rubric are interchangeable in every rubric position in both.
Equivalent inputs produce identical wire bytes either way; only the moment of the check
differs.

Declaration-time validation is shared. Rejected loudly: an empty or whitespace-only example or
counterexample; a duplicate string within one option's examples or within its counterexamples;
the same string as an example of two different options, or of two different levels, since it
cannot belong to both; and the same string as both an example and a counterexample of the same
option; and a counterexample on a **level**, since an ordered scale has no "not this option".
The same string as an example of one option and a counterexample of **another** is legitimate
and must stay legal — it is exactly the confusable-options pattern this feature exists for.

Python also refuses an **empty clause written out** — `examples=[]`, where the caller wrote the
clause and put nothing in it. Rust's attribute surface has no spelling for that: a variant
either carries `#[guide(example = "…")]` or it does not, so the rule has no Rust counterpart.
It is absent there because it is inexpressible, not because it goes unenforced.

**Validation is over the declared items, not the rendered text.** That one rule explains the
rest: `["a; b"]` renders exactly like `["a", "b"]` and still passes the duplicate and
shared-example checks, `" a"` is not `"a"`, and `"A"` is not `"a"`. No SDK normalises, and none
should start.

What the rules guarantee is **rendering integrity, not input trust**: a string an SDK renders
from declared parts carries exactly the clauses those parts declared. Rubric text itself is
trusted and is not sanitised, and a rubric handed to a runtime constructor as an
already-rendered string is passed through as written.

Three definitions follow, because they are the kind of thing two SDKs drift on silently:

- **Empty or whitespace-only** means the string is empty once characters with the Unicode
  `White_Space` property are removed from both ends. That is exactly Rust's `str::trim`.
  Python's `str.strip()` is a *superset*: it also strips the C0 separators `U+001C`–`U+001F`,
  which `White_Space` does not include, so a rubric of a single `U+001C` is blank to Python and
  not to Rust. The boundary is stated rather than resolved, because the characters involved are
  unreachable from a keyboard. An SDK that wants to match exactly should trim on `White_Space`.
- **Duplicate detection is exact string equality**, with no trimming, case folding or Unicode
  normalisation. `"a"` and `" a"` are two different examples and both are legal. Only the
  emptiness check trims, so the two rules deliberately disagree about what `" a"` is.
- **An example or counterexample may not contain `U+000A` or `U+000D`.** Items are joined onto
  one line with `"; "`, so a line break in one would render as a clause boundary the
  declaration never wrote. The check is `contains` over exactly those two code points — never a
  language's line-splitting primitive (`str::lines`, `str.splitlines`) and never a
  control-character class, because those cover different sets and would leave two SDKs
  disagreeing about `U+2028`, `U+0085` and the rest. Those are deliberately **not** refused:
  they are `White_Space`, so they are already blank on their own, and embedded they are
  harmless. A `what` may contain anything, line breaks included; only items are constrained.
  Where a rubric breaks this rule and the blank-rubric rule at once, the blank one is reported.

Items are inserted verbatim: nothing is escaped, and the rendering is not required to be
reversible — a reader cannot in general recover the item list from the rendered string, and no
SDK should try. An item containing `"; "`, or one whose text is literally
`"Not this option: x"`, is documented rather than refused. Both change what a reader sees
*inside* a clause, while a line break changes *which clause* they are in, and only the second
can forge a `Not this option:` the declaration never wrote. That is the whole of the
refused-versus-documented line; it is not two arbitrary decisions.

An empty or whitespace-only rubric is rejected **only where examples were attached to it** —
you described nothing. A rubric that carries neither keeps whatever an SDK did before this
feature existed, because rejecting it would be a new error for a declaration that has nothing
to do with examples.

**Every rule above holds on every path**, declaration and runtime, in both SDKs. That includes
the two that need more than one rubric in view — an example shared by two options or two
levels, and a counterexample on a level — because every runtime constructor holds the whole
set at once. Only the moment differs: Python refuses when the question is built
(`ConfigError`), Rust when the question is asked (`Error::Config`). A declaration is legal in
both SDKs or in neither. The must-allow overlap is never rejected anywhere.

## 4. Interface shape

Mirror the verbs, in the idiom of the language:

- constructors for a yes/no question, a choice over an enum, a score over an ordered enum,
  a choice over runtime options, and a score over runtime levels;
- a way to attach examples to an option, a level and a noul criterion, and counterexamples to
  an option and a noul criterion;
- question methods to patch the policy, set `yes_above` and `no_below` on nouls, set
  `min_confidence` on choice and score, set a fallback value, describe what yes and no mean
  on a noul, and switch to the detailed reading;
- one `ask(shape, state)` entry point where a shape is a question, a tuple or list of shapes,
  or a map of shapes, answered in one request with ids `q0..qN` in encounter order; the result
  has the same shape;
- unsure resolution in this order: the question's fallback value, the enum's fallback
  member, then a typed unsure error; the detailed reading never fails on unsure;
- a way to read, alongside the answer, the response's `model` (the versioned id that answered)
  and its `usage` (`input_tokens`, `output_tokens`): Rust's `Guide::ask_with_receipt` returning
  `Receipt<T> { answer, model, usage }`, Python's `ask_with_receipt` returning `Receipt[T]`
  with the same three fields;
- a retry policy: `429` and `529` are retried with exponential backoff honouring an integer
  `retry-after`, on both `POST /v1/systemone` and `GET /v1/models`; a connection failure — the
  request never reached a server, so a refused or reset connection or a TLS handshake failure —
  is retried in the same budget; **a timeout of any phase** (connect, read, write) and a body
  failure are not. The reason is stated rather than left to each SDK: Rust sets one overall
  deadline per attempt, under which a connect-phase timeout is indistinguishable from a read
  timeout — `reqwest` classifies it as `is_timeout()`, not `is_connect()` — and a retried
  timeout multiplies the wall time the builder promises; Python matches by not retrying
  `httpx.ConnectTimeout` either. After the last retry, `429` is a rate-limited error and `529`
  an overloaded error, each carrying the `retry-after` the API last sent;
- telemetry with the names in `docs/observability.md`: one `guideme.ask` span per `ask` with
  the `gen_ai.*` attributes, one HTTP client span per attempt, one `guideme.answer` event per
  answer, and `error.type` from the same list of names. The names are the contract so that
  one dashboard reads every SDK; how they are emitted is each language's business.

Runtime option sets should return a caller-chosen type where the language allows it.
Exhaustive matching over the enum is enforced where the language can; elsewhere, a literal
union plus a runtime check on the wire key.
