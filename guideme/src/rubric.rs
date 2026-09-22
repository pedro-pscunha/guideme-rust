//! A rubric as a value: what something means, plus the inputs that belong to it.

use crate::Error;

/// A description with examples, for the rubric positions that are not an enum.
///
/// `#[derive(Choice)]` and `#[derive(Levels)]` compose the same thing at compile time from
/// `#[guide(example = "…")]` and `#[guide(counterexample = "…")]`. A noul has no enum to hang
/// them off, so it takes this instead: [`Question::criteria`](crate::Question::criteria) accepts
/// a description or a `Rubric`, through [`IntoRubric`].
///
/// Examples and counterexamples render in the order they were added. A `Rubric` with neither
/// renders to `what` itself, byte for byte. [`render`](Rubric::render) is the only way to get
/// that string, and it checks before it renders, so there is no unchecked path to the wire.
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
/// [`render`](Rubric::render) enforces every rule one rubric can see, in the derives' wording:
/// an empty example or counterexample, a duplicate within either clause, the same string as
/// both an example and a counterexample, and examples attached to a blank rubric. Two rules it
/// cannot see, because they need something outside the rubric: an example shared by two
/// options, which is checked for a noul's pair when the question is asked but not across the
/// options of [`choose_among`](crate::choose_among), and a counterexample on a level, which
/// only [`score_levels`](crate::score_levels) knows it is building. Both are compile errors
/// under the derives.
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

    /// Check, then compose the parts into the one string the API takes.
    ///
    /// `docs/contract.md` pins these bytes; `guideme-derive` renders the same way at expansion
    /// time, and `guideme/tests/rubric.rs` asserts the two agree. A blank rubric on its own is
    /// left alone — it is what 0.1.0 accepted and says nothing about this feature — so that one
    /// fires only where parts were attached to it.
    ///
    /// # Errors
    /// [`Error::Config`] for any rule this rubric breaks; the type's docs list them.
    pub fn render(&self) -> Result<String, Error> {
        if self.what.trim().is_empty()
            && !(self.examples.is_empty() && self.counterexamples.is_empty())
        {
            return Err(Error::Config {
                detail: "examples need a non-empty rubric to attach to".into(),
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
        let mut out = self.what.clone();
        if !self.examples.is_empty() {
            out.push_str("\nExamples: ");
            out.push_str(&self.examples.join("; "));
        }
        if !self.counterexamples.is_empty() {
            out.push_str("\nNot this option: ");
            out.push_str(&self.counterexamples.join("; "));
        }
        Ok(out)
    }
}

/// Check a noul's pair, then render both. An example asserts that an input belongs to this
/// side, so the same string on both sides asserts it belongs to each — the rule the derive
/// applies across a `Choice`'s options, on the only runtime path that holds both in view.
pub(crate) fn render_pair(yes: &Rubric, no: &Rubric) -> Result<(String, String), Error> {
    for example in &yes.examples {
        if no.examples.contains(example) {
            return Err(Error::Config {
                detail: format!(
                    "{example:?} is an example of both yes and no; an input belongs to one option"
                ),
            });
        }
    }
    Ok((yes.render()?, no.render()?))
}

/// Reject an empty entry, a line break, or a repeat within one clause. Whitespace-only counts
/// as empty; duplicate detection is exact, so `"a"` and `" a"` are two different examples. The
/// line-break check is `contains` over `U+000A` and `U+000D`, never a line-splitting primitive:
/// those cover different sets in different languages and would make two SDKs disagree.
fn check_clause(items: &[String], kind: &str) -> Result<(), Error> {
    for (i, item) in items.iter().enumerate() {
        if item.trim().is_empty() {
            return Err(Error::Config {
                detail: format!("an {kind} must not be empty"),
            });
        }
        if item.contains(['\n', '\r']) {
            return Err(Error::Config {
                detail: format!(
                    "an {kind} may not contain a line break (U+000A or U+000D): {item:?}"
                ),
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

mod sealed {
    pub trait Sealed {}
}

/// What a rubric position accepts: a description — anything `String` converts from, so every
/// call written against `impl Into<String>` still compiles — or a [`Rubric`] carrying examples.
///
/// Sealed. It exists so one position can take either, not as an extension point.
///
/// `Rubric` is **permanently** not `Into<String>`, and cannot become so: the blanket case below
/// covers every `T: Into<String>`, so an `impl From<Rubric> for String` would make the two impls
/// overlap and neither would compile. Adding one is not a widening to weigh at 0.2.0, it is a
/// change that cannot be made while `IntoRubric` accepts both. [`Rubric::render`] is how a
/// `Rubric` becomes a string.
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
