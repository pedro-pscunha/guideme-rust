//! A rubric as a value: what something means, plus the inputs that belong to it.

use std::borrow::Cow;

use crate::Error;

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
/// A plain `&str` or `String` converts into one, so anything that took a description before
/// still does. Examples attached to a blank description are rejected when the question is
/// asked, the same line the derives draw at compile time; the contradiction checks need a
/// whole declaration to see and stay in `guideme-derive`.
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

    /// Render for the wire, rejecting examples attached to nothing.
    ///
    /// A blank description on its own is left alone — it is what `guideme` 0.1.0 accepted and
    /// says nothing about this feature — so the error fires only where parts were attached to
    /// it. `docs/contract.md` draws the same line for every SDK.
    pub(crate) fn into_wire(self) -> Result<String, Error> {
        if self.what.trim().is_empty()
            && !(self.examples.is_empty() && self.counterexamples.is_empty())
        {
            return Err(Error::Config {
                detail: "a rubric with examples needs a description to attach them to".into(),
            });
        }
        Ok(self.into())
    }
}

/// Everything `String` converts from, so a description that was accepted where this type is
/// now taken is still accepted. `guideme/tests/rubric.rs` compiles one of each.
macro_rules! from_description {
    ($($t:ty),*) => {
        $(impl From<$t> for Rubric {
            fn from(what: $t) -> Self {
                Self::new(String::from(what))
            }
        })*
    };
}

from_description!(&str, String, &String, Cow<'_, str>, Box<str>, char);

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
