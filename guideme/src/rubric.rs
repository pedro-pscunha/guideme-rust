//! A rubric as a value: what something means, plus the inputs that belong to it.

use std::fmt;

use crate::Error;

/// A description with examples, for the rubric positions that are not an enum.
///
/// `#[derive(Choice)]` and `#[derive(Levels)]` compose the same thing at compile time from
/// `#[guide(example = "…")]` and `#[guide(counterexample = "…")]`. A noul has no enum to hang
/// them off, so it takes this instead: [`Question::criteria`](crate::Question::criteria) accepts
/// a description or a `Rubric`, through [`IntoRubric`].
///
/// Examples and counterexamples render in the order they were added. A `Rubric` with neither
/// renders to `what` itself, byte for byte. [`Display`](fmt::Display) renders without checking
/// anything, for the rubric positions that take an already-rendered string; the checks below
/// run when a question carrying one is asked.
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
/// The rules a single rubric can see are enforced here, with the same wording the derives use:
/// an empty example or counterexample, a duplicate within either clause, the same string as
/// both an example and a counterexample, and examples attached to a blank description. The
/// contradiction checks that need every option at once stay in `guideme-derive`.
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

    /// Check, then render for the wire.
    ///
    /// A blank description on its own is left alone — it is what 0.1.0 accepted and says
    /// nothing about this feature — so that one fires only where parts were attached to it.
    /// `docs/contract.md` draws the same line for every SDK.
    pub(crate) fn into_wire(self) -> Result<String, Error> {
        if self.what.trim().is_empty()
            && !(self.examples.is_empty() && self.counterexamples.is_empty())
        {
            return Err(Error::Config {
                detail: "examples need a non-empty description to attach to".into(),
            });
        }
        check_clause(&self.examples, "example")?;
        check_clause(&self.counterexamples, "counterexample")?;
        for example in &self.examples {
            if self.counterexamples.contains(example) {
                return Err(Error::Config {
                    detail: format!(
                        "{example:?} is both an example and a counterexample; it cannot be in and out of the same option"
                    ),
                });
            }
        }
        Ok(self.to_string())
    }
}

/// Reject an empty entry or a repeat within one clause. Whitespace-only counts as empty, and
/// duplicate detection is exact: `"a"` and `" a"` are two different examples.
fn check_clause(items: &[String], kind: &str) -> Result<(), Error> {
    for (i, item) in items.iter().enumerate() {
        if item.trim().is_empty() {
            return Err(Error::Config {
                detail: format!("an {kind} must not be empty"),
            });
        }
        if items[..i].contains(item) {
            return Err(Error::Config {
                detail: format!("duplicate {kind} {item:?}"),
            });
        }
    }
    Ok(())
}

/// Compose the parts into the one string the API takes. `docs/contract.md` pins these bytes;
/// `guideme-derive` renders the same way at expansion time, and `guideme/tests/rubric.rs`
/// asserts the two agree.
impl fmt::Display for Rubric {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.what)?;
        if !self.examples.is_empty() {
            write!(f, "\nExamples: {}", self.examples.join("; "))?;
        }
        if !self.counterexamples.is_empty() {
            write!(f, "\nNot this option: {}", self.counterexamples.join("; "))?;
        }
        Ok(())
    }
}

mod sealed {
    pub trait Sealed {}
}

/// What a rubric position accepts: a description — anything `String` converts from, so every
/// call written against `impl Into<String>` still compiles — or a [`Rubric`] carrying examples.
///
/// Sealed. It exists so one position can take either, not as an extension point. `Rubric`
/// itself is deliberately not `Into<String>`: that is what lets the blanket case and the
/// `Rubric` case coexist. Use [`Display`](fmt::Display) to render one by hand.
pub trait IntoRubric: sealed::Sealed {
    /// Convert into a [`Rubric`].
    fn into_rubric(self) -> Rubric;
}

impl<T: Into<String>> sealed::Sealed for T {}

impl<T: Into<String>> IntoRubric for T {
    fn into_rubric(self) -> Rubric {
        Rubric::new(self)
    }
}

impl sealed::Sealed for Rubric {}

impl IntoRubric for Rubric {
    fn into_rubric(self) -> Rubric {
        self
    }
}
