//! A rubric as a value: what something means, plus the inputs that belong to it.

/// A description with examples, for the rubric positions that are not an enum.
///
/// `#[derive(Choice)]` and `#[derive(Levels)]` compose the same thing at compile time from
/// `#[guide(example = "…")]` and `#[guide(counterexample = "…")]`. A noul has no enum to hang
/// them off, so it takes this instead: [`Question::criteria`](crate::Question::criteria) accepts
/// anything that is `Into<String>`, and a `Rubric` is.
///
/// Examples and counterexamples render in the order they were added. A `Rubric` with neither
/// renders to `what` itself, byte for byte.
///
/// ```
/// use guideme::{noul, Rubric};
///
/// let question = noul("Is this ticket urgent?").criteria(
///     Rubric::new("Something is broken right now and nobody can work around it")
///         .example("the checkout page is down")
///         .counterexample("a nightly job failed and the numbers are pulled by hand for now"),
///     Rubric::new("It can wait for the next working day"),
/// );
/// # let _ = question;
/// ```
///
/// Unlike the derives, this is a runtime path and carries no declaration-time checks: the
/// compile errors that reject an empty rubric or a contradictory example live in
/// `guideme-derive`, where there is a declaration to reject.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rubric {
    what: String,
    examples: Vec<String>,
    counterexamples: Vec<String>,
}

impl Rubric {
    /// Start from what this option or criterion means.
    pub fn new(what: impl Into<String>) -> Self {
        Self {
            what: what.into(),
            examples: Vec::new(),
            counterexamples: Vec::new(),
        }
    }

    /// Add an input that belongs here. Repeatable.
    pub fn example(mut self, example: impl Into<String>) -> Self {
        self.examples.push(example.into());
        self
    }

    /// Add an input that does not belong here. Repeatable.
    pub fn counterexample(mut self, counterexample: impl Into<String>) -> Self {
        self.counterexamples.push(counterexample.into());
        self
    }
}

/// Compose the parts into the one string the API takes. `docs/contract.md` pins these bytes;
/// `guideme-derive` renders the same way at expansion time, and `guideme/tests/rubric.rs`
/// asserts the two agree.
impl From<Rubric> for String {
    fn from(rubric: Rubric) -> Self {
        if rubric.examples.is_empty() && rubric.counterexamples.is_empty() {
            return rubric.what;
        }
        let mut out = rubric.what;
        if !rubric.examples.is_empty() {
            out.push_str("\nExamples: ");
            out.push_str(&rubric.examples.join("; "));
        }
        if !rubric.counterexamples.is_empty() {
            out.push_str("\nNot this option: ");
            out.push_str(&rubric.counterexamples.join("; "));
        }
        out
    }
}
