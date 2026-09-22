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
/// cannot see, because they need something outside the rubric, are applied where the question
/// is asked and the whole set is in view: an example shared by two options or two levels, and
/// a counterexample on a level, which only [`score_levels`](crate::score_levels) knows it is
/// building. Both are compile errors under the derives and [`Error::Config`] on the runtime
/// paths, so a declaration is legal on both or on neither.
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

/// Check a noul's pair, then render both.
pub(crate) fn render_pair(yes: &Rubric, no: &Rubric) -> Result<(String, String), Error> {
    check_shared(&[yes, no], "option", |i| {
        if i == 0 { "yes" } else { "no" }.to_owned()
    })?;
    Ok((yes.render()?, no.render()?))
}

/// Check a choice's options against each other, then render each. A key with no rubric stays
/// `None`: the wire distinguishes an option described as nothing from one not described.
pub(crate) fn render_options(
    options: &[(String, Option<Rubric>)],
) -> Result<Vec<(String, Option<String>)>, Error> {
    let described: Vec<(&str, &Rubric)> = options
        .iter()
        .filter_map(|(key, rubric)| rubric.as_ref().map(|r| (key.as_str(), r)))
        .collect();
    let rubrics: Vec<&Rubric> = described.iter().map(|(_, r)| *r).collect();
    check_shared(&rubrics, "option", |i| format!("{:?}", described[i].0))?;
    options
        .iter()
        .map(|(key, rubric)| {
            let rendered = match rubric {
                Some(rubric) => Some(rubric.render()?),
                None => None,
            };
            Ok((key.clone(), rendered))
        })
        .collect()
}

/// Check a score's levels against each other, then render each.
///
/// A level may not carry a counterexample: it is a position on an ordered scale, not an
/// option to rule out. `#[derive(Levels)]` makes that a compile error; this is the same rule
/// where there is no declaration to reject.
pub(crate) fn render_levels(levels: &[Rubric]) -> Result<Vec<String>, Error> {
    for (i, level) in levels.iter().enumerate() {
        if !level.counterexamples.is_empty() {
            return Err(Error::Config {
                detail: format!(
                    "a counterexample is not allowed on level {i}; a level is a position on a scale, not an option to rule out"
                ),
            });
        }
    }
    let rubrics: Vec<&Rubric> = levels.iter().collect();
    check_shared(&rubrics, "level", |i| format!("level {i}"))?;
    levels.iter().map(Rubric::render).collect()
}

/// The rule no single rubric can see. An example asserts that an input belongs here, so the
/// same string under two of them asserts it belongs to each — what the derives check across a
/// `Choice`'s variants at expansion time, applied on the runtime paths that hold every rubric
/// in view at once. `label` names a position the way the caller's own surface does, so the
/// message points at something they wrote.
fn check_shared(
    rubrics: &[&Rubric],
    noun: &str,
    label: impl Fn(usize) -> String,
) -> Result<(), Error> {
    for (i, rubric) in rubrics.iter().enumerate() {
        for example in &rubric.examples {
            for (j, earlier) in rubrics[..i].iter().enumerate() {
                if earlier.examples.contains(example) {
                    return Err(Error::Config {
                        detail: format!(
                            "{example:?} is an example of both {} and {}; an input belongs to one {noun}",
                            label(j),
                            label(i)
                        ),
                    });
                }
            }
        }
    }
    Ok(())
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
