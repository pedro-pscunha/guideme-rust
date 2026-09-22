//! Questions as values: build them anywhere, ask them through a [`crate::Guide`].

use std::collections::BTreeMap;
use std::fmt;
use std::marker::PhantomData;

use crate::api::{self, MAX_LEVELS, MAX_OPTIONS, NoulCriteria};
use crate::policy::{Outcome, Thresholds, Verdict};
use crate::rubric::{IntoRubric, Rubric};
use crate::{Confidence, Error, Instructions, Policy, Probability};

pub(crate) mod sealed {
    pub trait Sealed {}
}

/// Implemented by `#[derive(Choice)]`. Hand-implement for dynamic option sets.
pub trait Options: Clone + Eq + Send + Sync + 'static {
    /// `(key, rubric)` pairs in declaration order.
    const RUBRIC: &'static [(&'static str, Option<&'static str>)];
    /// Map a wire key back to a variant.
    fn from_key(key: &str) -> Option<Self>;
    /// The `#[guide(fallback)]` variant, if any.
    fn fallback() -> Option<Self> {
        None
    }
}

/// Implemented by `#[derive(Levels)]`. Level order is declaration order.
pub trait Levels: Clone + Ord + Send + Sync + 'static {
    /// Level descriptions, low to high.
    const LEVELS: &'static [&'static str];
    /// Map a level index back to a variant.
    fn from_index(index: usize) -> Option<Self>;
    /// This variant's index.
    fn index(&self) -> usize;
}

/// A runtime option key. Only meaningful through [`choose_among`]: its `RUBRIC` is empty,
/// so `choose::<Key>(..)` is rejected with [`Error::Config`] when asked.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Key(pub String);

impl Options for Key {
    const RUBRIC: &'static [(&'static str, Option<&'static str>)] = &[];
    fn from_key(key: &str) -> Option<Self> {
        Some(Self(key.to_owned()))
    }
}

/// A runtime level index. Only meaningful through [`score_levels`]: its `LEVELS` is empty,
/// so `score::<Rank>(..)` is rejected with [`Error::Config`] when asked.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Rank(pub usize);

impl Levels for Rank {
    const LEVELS: &'static [&'static str] = &[];
    fn from_index(index: usize) -> Option<Self> {
        Some(Self(index))
    }
    fn index(&self) -> usize {
        self.0
    }
}

/// A choice answer in full: the pick, its confidence, and the whole distribution.
#[derive(Clone, Debug, PartialEq)]
pub struct Ranked<C> {
    /// The chosen option.
    pub choice: C,
    /// Reported confidence.
    pub confidence: Confidence,
    /// `confidence < min_confidence`.
    pub unsure: bool,
    /// Options by descending probability.
    pub probabilities: Vec<(C, Probability)>,
}

/// A score answer in full: expected value, argmax level, confidence, distribution.
#[derive(Clone, Debug, PartialEq)]
pub struct Scored<L> {
    /// The API's probability-weighted value; may lie between levels.
    pub value: f64,
    /// Argmax of the distribution.
    pub level: L,
    /// Reported confidence.
    pub confidence: Confidence,
    /// `confidence < min_confidence`.
    pub unsure: bool,
    /// Probabilities in level order.
    pub distribution: Vec<(L, Probability)>,
}

/// A question kind: how to put it on the wire and how to read the outcome back.
pub trait Kind: sealed::Sealed + Send + Sync + 'static {
    /// The plain output: `bool`, `C`, `L`, or a detail type.
    type Out: Send + 'static;
    #[doc(hidden)]
    fn wire(&self, instructions: Instructions) -> Result<api::Question, Error>;
    #[doc(hidden)]
    fn read(
        &self,
        id: &str,
        outcome: Outcome,
        t: Thresholds,
        or: Option<Self::Out>,
    ) -> Result<Self::Out, Error>;
}

/// Kinds whose plain output can fail with [`Error::Unsure`] and therefore accept `.or()` / `.detail()`.
pub trait Fallible: Kind {
    /// The detail type `.detail()` switches to.
    type Detail: Send + 'static;
    #[doc(hidden)]
    fn read_detail(&self, id: &str, outcome: Outcome, t: Thresholds)
    -> Result<Self::Detail, Error>;
}

/// Kinds judged by a yes/no probability: `yes_above` / `no_below` apply.
pub trait Binary: Kind {}

/// Kinds judged by a reported confidence: `min_confidence` applies.
pub trait Confident: Kind {}

/// Yes/no.
#[derive(Clone, Debug)]
pub struct Noul {
    criteria: Option<(Rubric, Rubric)>,
}

/// One of `C`'s options.
#[derive(Clone, Debug)]
pub struct Choose<C: Options> {
    rubric: Vec<(String, Option<String>)>,
    _c: PhantomData<fn() -> C>,
}

/// A level of `L`.
#[derive(Clone, Debug)]
pub struct Score<L: Levels> {
    levels: Vec<String>,
    _l: PhantomData<fn() -> L>,
}

/// `K` with the detailed output. Never fails on unsure.
#[derive(Clone, Debug)]
pub struct Detailed<K: Fallible>(K);

impl sealed::Sealed for Noul {}
impl<C: Options> sealed::Sealed for Choose<C> {}
impl<L: Levels> sealed::Sealed for Score<L> {}
impl<K: Fallible> sealed::Sealed for Detailed<K> {}

impl Binary for Noul {}
impl Binary for Detailed<Noul> {}
impl<C: Options> Confident for Choose<C> {}
impl<L: Levels> Confident for Score<L> {}
impl<K: Fallible + Confident> Confident for Detailed<K> {}

fn mismatch(id: &str, want: &str, got: &Outcome) -> Error {
    let got = match got {
        Outcome::Noul(_) => "noul",
        Outcome::Choice { .. } => "choice",
        Outcome::Score { .. } => "score",
    };
    Error::Protocol {
        detail: format!("question {id} asked for {want}, answer is {got}"),
    }
}

fn unsure<T>(id: &str, value: f64, threshold: f64, or: Option<T>) -> Result<T, Error> {
    or.ok_or_else(|| Error::Unsure {
        question: id.to_owned(),
        value,
        threshold,
    })
}

impl Kind for Noul {
    type Out = bool;
    fn wire(&self, instructions: Instructions) -> Result<api::Question, Error> {
        let criteria = match self.criteria.clone() {
            Some((yes, no)) => Some(NoulCriteria {
                yes: yes.into_wire()?,
                no: no.into_wire()?,
            }),
            None => None,
        };
        Ok(api::Question::Noul {
            instructions,
            criteria,
        })
    }
    fn read(
        &self,
        id: &str,
        outcome: Outcome,
        t: Thresholds,
        or: Option<bool>,
    ) -> Result<bool, Error> {
        match self.read_detail(id, outcome, t)? {
            Verdict::Yes(_) => Ok(true),
            Verdict::No(_) => Ok(false),
            Verdict::Unsure(p) => {
                let p = p.get();
                let nearer = if t.yes_above() - p <= p - t.no_below() {
                    t.yes_above()
                } else {
                    t.no_below()
                };
                unsure(id, p, nearer, or)
            }
        }
    }
}

impl Fallible for Noul {
    type Detail = Verdict;
    fn read_detail(&self, id: &str, outcome: Outcome, _t: Thresholds) -> Result<Verdict, Error> {
        match outcome {
            Outcome::Noul(v) => Ok(v),
            other @ (Outcome::Choice { .. } | Outcome::Score { .. }) => {
                Err(mismatch(id, "noul", &other))
            }
        }
    }
}

impl<C: Options> Kind for Choose<C> {
    type Out = C;
    fn wire(&self, instructions: Instructions) -> Result<api::Question, Error> {
        if self.rubric.is_empty() {
            return Err(Error::Config {
                detail: "a choice needs at least one option".into(),
            });
        }
        if self.rubric.len() > MAX_OPTIONS {
            return Err(Error::Config {
                detail: format!("a choice may have at most {MAX_OPTIONS} options"),
            });
        }
        let criteria: BTreeMap<String, Option<String>> = self.rubric.iter().cloned().collect();
        if criteria.len() != self.rubric.len() {
            return Err(Error::Config {
                detail: "duplicate option keys".into(),
            });
        }
        Ok(api::Question::Choice {
            instructions,
            criteria,
        })
    }
    fn read(&self, id: &str, outcome: Outcome, t: Thresholds, or: Option<C>) -> Result<C, Error> {
        let r = self.read_detail(id, outcome, t)?;
        if r.unsure {
            unsure(
                id,
                r.confidence.get(),
                t.min_confidence(),
                or.or_else(C::fallback),
            )
        } else {
            Ok(r.choice)
        }
    }
}

impl<C: Options> Fallible for Choose<C> {
    type Detail = Ranked<C>;
    fn read_detail(&self, id: &str, outcome: Outcome, _t: Thresholds) -> Result<Ranked<C>, Error> {
        let Outcome::Choice {
            key,
            confidence,
            unsure,
            ranked,
        } = outcome
        else {
            return Err(mismatch(id, "choice", &outcome));
        };
        if ranked.len() != self.rubric.len() {
            return Err(Error::Protocol {
                detail: format!(
                    "question {id}: answer has {} options, rubric has {}",
                    ranked.len(),
                    self.rubric.len()
                ),
            });
        }
        let map = |k: &str| {
            if !self.rubric.iter().any(|(r, _)| r == k) {
                return Err(Error::Protocol {
                    detail: format!("question {id}: option {k:?} is not in the rubric"),
                });
            }
            C::from_key(k).ok_or_else(|| Error::Protocol {
                detail: format!("question {id}: option {k:?} has no variant"),
            })
        };
        Ok(Ranked {
            choice: map(&key)?,
            confidence,
            unsure,
            probabilities: ranked
                .iter()
                .map(|(k, p)| Ok((map(k)?, *p)))
                .collect::<Result<_, Error>>()?,
        })
    }
}

impl<L: Levels> Kind for Score<L> {
    type Out = L;
    fn wire(&self, instructions: Instructions) -> Result<api::Question, Error> {
        if !(2..=MAX_LEVELS).contains(&self.levels.len()) {
            return Err(Error::Config {
                detail: format!(
                    "a score needs 2..={MAX_LEVELS} levels, got {}",
                    self.levels.len()
                ),
            });
        }
        Ok(api::Question::Score {
            instructions,
            criteria: self.levels.clone(),
        })
    }
    fn read(&self, id: &str, outcome: Outcome, t: Thresholds, or: Option<L>) -> Result<L, Error> {
        let s = self.read_detail(id, outcome, t)?;
        if s.unsure {
            unsure(id, s.confidence.get(), t.min_confidence(), or)
        } else {
            Ok(s.level)
        }
    }
}

impl<L: Levels> Fallible for Score<L> {
    type Detail = Scored<L>;
    fn read_detail(&self, id: &str, outcome: Outcome, _t: Thresholds) -> Result<Scored<L>, Error> {
        let Outcome::Score {
            index,
            value,
            confidence,
            unsure,
            distribution,
            ..
        } = outcome
        else {
            return Err(mismatch(id, "score", &outcome));
        };
        if distribution.len() != self.levels.len() {
            return Err(Error::Protocol {
                detail: format!(
                    "question {id}: answer has {} levels, question has {}",
                    distribution.len(),
                    self.levels.len()
                ),
            });
        }
        let map = |i: usize| {
            L::from_index(i).ok_or_else(|| Error::Protocol {
                detail: format!("question {id}: level {i} is not in the rubric"),
            })
        };
        Ok(Scored {
            value,
            level: map(index)?,
            confidence,
            unsure,
            distribution: distribution
                .iter()
                .enumerate()
                .map(|(i, p)| Ok((map(i)?, *p)))
                .collect::<Result<_, Error>>()?,
        })
    }
}

impl<K: Fallible> Kind for Detailed<K> {
    type Out = K::Detail;
    fn wire(&self, instructions: Instructions) -> Result<api::Question, Error> {
        self.0.wire(instructions)
    }
    fn read(
        &self,
        id: &str,
        outcome: Outcome,
        t: Thresholds,
        _or: Option<K::Detail>,
    ) -> Result<K::Detail, Error> {
        self.0.read_detail(id, outcome, t)
    }
}

/// A question plus its local policy patch. Inert until a [`crate::Guide`] asks it; build it
/// once and clone it per ask.
///
/// Not to be confused with [`crate::api::Question`], the wire enum this turns into.
pub struct Question<K: Kind> {
    pub(crate) instructions: Instructions,
    pub(crate) kind: K,
    pub(crate) policy: Policy,
    pub(crate) or: Option<K::Out>,
}

impl<K: Kind + Clone> Clone for Question<K>
where
    K::Out: Clone,
{
    fn clone(&self) -> Self {
        Self {
            instructions: self.instructions.clone(),
            kind: self.kind.clone(),
            policy: self.policy,
            or: self.or.clone(),
        }
    }
}

impl<K: Kind + fmt::Debug> fmt::Debug for Question<K>
where
    K::Out: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Question")
            .field("instructions", &self.instructions)
            .field("kind", &self.kind)
            .field("policy", &self.policy)
            .field("or", &self.or)
            .finish()
    }
}

impl<K: Kind> Question<K> {
    /// Merge a policy patch over this question's. Precedence: question > guide > crate defaults.
    pub fn with(mut self, policy: Policy) -> Self {
        self.policy = policy.over(self.policy);
        self
    }
}

impl<K: Binary> Question<K> {
    /// `p >= yes_above` is yes.
    pub fn yes_above(self, p: f64) -> Self {
        self.with(Policy::new().yes_above(p))
    }
    /// `p <= no_below` is no.
    pub fn no_below(self, p: f64) -> Self {
        self.with(Policy::new().no_below(p))
    }
}

impl<K: Confident> Question<K> {
    /// `confidence < min_confidence` is unsure.
    pub fn min_confidence(self, c: f64) -> Self {
        self.with(Policy::new().min_confidence(c))
    }
}

impl<K: Fallible> Question<K> {
    /// Value to use when the policy says unsure. Beats `#[guide(fallback)]`.
    pub fn or(mut self, value: K::Out) -> Self {
        self.or = Some(value);
        self
    }
    /// Ask for the full reading (`Verdict` / `Ranked<C>` / `Scored<L>`); never fails on unsure.
    /// Any `.or(..)` set before this call is dropped: the detailed reading never needs it.
    pub fn detail(self) -> Question<Detailed<K>> {
        Question {
            instructions: self.instructions,
            kind: Detailed(self.kind),
            policy: self.policy,
            or: None,
        }
    }
}

impl Question<Noul> {
    /// Describe what a yes and a no mean. Takes a description, or a [`Rubric`] carrying
    /// examples.
    pub fn criteria(mut self, yes: impl IntoRubric, no: impl IntoRubric) -> Self {
        self.kind.criteria = Some((yes.into_rubric(), no.into_rubric()));
        self
    }
}

fn question<K: Kind>(instructions: impl Into<Instructions>, kind: K) -> Question<K> {
    Question {
        instructions: instructions.into(),
        kind,
        policy: Policy::new(),
        or: None,
    }
}

/// A yes/no question. Plain output: `bool`.
pub fn noul(instructions: impl Into<Instructions>) -> Question<Noul> {
    question(instructions, Noul { criteria: None })
}

/// Pick one of `C`'s options. Plain output: `C`.
pub fn choose<C: Options>(instructions: impl Into<Instructions>) -> Question<Choose<C>> {
    let rubric = C::RUBRIC
        .iter()
        .map(|(k, r)| ((*k).to_owned(), r.map(str::to_owned)))
        .collect();
    question(
        instructions,
        Choose {
            rubric,
            _c: PhantomData,
        },
    )
}

/// Pick one of runtime options `(key, rubric)`. Plain output: [`Key`].
pub fn choose_among<'a>(
    instructions: impl Into<Instructions>,
    options: impl IntoIterator<Item = (&'a str, Option<&'a str>)>,
) -> Question<Choose<Key>> {
    let rubric = options
        .into_iter()
        .map(|(k, r)| (k.to_owned(), r.map(str::to_owned)))
        .collect();
    question(
        instructions,
        Choose {
            rubric,
            _c: PhantomData,
        },
    )
}

/// Rate on `L`'s levels. Plain output: the argmax level `L`.
pub fn score<L: Levels>(instructions: impl Into<Instructions>) -> Question<Score<L>> {
    question(
        instructions,
        Score {
            levels: L::LEVELS.iter().map(|s| (*s).to_owned()).collect(),
            _l: PhantomData,
        },
    )
}

/// Rate on runtime levels, low to high. Plain output: [`Rank`].
pub fn score_levels<'a>(
    instructions: impl Into<Instructions>,
    levels: impl IntoIterator<Item = &'a str>,
) -> Question<Score<Rank>> {
    question(
        instructions,
        Score {
            levels: levels.into_iter().map(str::to_owned).collect(),
            _l: PhantomData,
        },
    )
}
