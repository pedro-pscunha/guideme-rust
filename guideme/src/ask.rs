//! A question *shape* is anything [`crate::Guide::ask`] can answer in one request:
//! a single [`Question`], a tuple of shapes, a `Vec` of shapes, or a `BTreeMap` of shapes.

use std::collections::BTreeMap;

use crate::api::{self, QuestionId};
use crate::policy::{Outcome, Thresholds};
use crate::question::{Kind, Question, sealed};
use crate::{Error, Policy};

/// Sealed. Output has the same shape as the input, each question replaced by its answer.
pub trait Ask: sealed::Sealed + Send {
    /// The answer shape.
    type Out;
    #[doc(hidden)]
    type Claim: Send;
    #[doc(hidden)]
    fn encode(self, plan: &mut Plan) -> Result<Self::Claim, Error>;
    #[doc(hidden)]
    fn decode(claim: Self::Claim, reply: &Reply) -> Result<Self::Out, Error>;
}

/// Questions accumulated for one request, with each one's settled thresholds.
#[doc(hidden)]
pub struct Plan {
    base: Policy,
    pub(crate) questions: BTreeMap<QuestionId, api::Question>,
    pub(crate) thresholds: BTreeMap<QuestionId, Thresholds>,
}

impl Plan {
    pub(crate) fn new(base: Policy) -> Self {
        Self {
            base,
            questions: BTreeMap::new(),
            thresholds: BTreeMap::new(),
        }
    }
    fn push(&mut self, question: api::Question, policy: Policy) -> Result<QuestionId, Error> {
        let id = QuestionId::from(format!("q{}", self.questions.len()));
        self.thresholds
            .insert(id.clone(), policy.over(self.base).settle()?);
        self.questions.insert(id.clone(), question);
        Ok(id)
    }
}

/// Resolved outcomes keyed by question id.
#[doc(hidden)]
pub struct Reply {
    pub(crate) outcomes: BTreeMap<QuestionId, (Outcome, Thresholds)>,
}

impl Reply {
    fn get(&self, id: &QuestionId) -> Result<(Outcome, Thresholds), Error> {
        self.outcomes
            .get(id)
            .cloned()
            .ok_or_else(|| Error::Protocol {
                detail: format!("no answer for question {}", id.as_str()),
            })
    }
}

impl<K: Kind> sealed::Sealed for Question<K> {}

impl<K: Kind> Ask for Question<K> {
    type Out = K::Out;
    type Claim = (QuestionId, K, Option<K::Out>);
    fn encode(self, plan: &mut Plan) -> Result<Self::Claim, Error> {
        let id = plan.push(self.kind.wire(self.instructions)?, self.policy)?;
        Ok((id, self.kind, self.or))
    }
    fn decode((id, kind, or): Self::Claim, reply: &Reply) -> Result<K::Out, Error> {
        let (outcome, t) = reply.get(&id)?;
        kind.read(id.as_str(), outcome, t, or)
    }
}

impl<Q: Ask> sealed::Sealed for Vec<Q> {}

impl<Q: Ask> Ask for Vec<Q> {
    type Out = Vec<Q::Out>;
    type Claim = Vec<Q::Claim>;
    fn encode(self, plan: &mut Plan) -> Result<Self::Claim, Error> {
        self.into_iter().map(|q| q.encode(plan)).collect()
    }
    fn decode(claims: Self::Claim, reply: &Reply) -> Result<Self::Out, Error> {
        claims.into_iter().map(|c| Q::decode(c, reply)).collect()
    }
}

impl<K: Ord + Send, Q: Ask> sealed::Sealed for BTreeMap<K, Q> {}

impl<K: Ord + Send, Q: Ask> Ask for BTreeMap<K, Q> {
    type Out = BTreeMap<K, Q::Out>;
    type Claim = Vec<(K, Q::Claim)>;
    fn encode(self, plan: &mut Plan) -> Result<Self::Claim, Error> {
        self.into_iter()
            .map(|(k, q)| Ok((k, q.encode(plan)?)))
            .collect()
    }
    fn decode(claims: Self::Claim, reply: &Reply) -> Result<Self::Out, Error> {
        claims
            .into_iter()
            .map(|(k, c)| Ok((k, Q::decode(c, reply)?)))
            .collect()
    }
}

macro_rules! tuple_ask {
    ($($t:ident $i:tt),+) => {
        impl<$($t: Ask),+> sealed::Sealed for ($($t,)+) {}
        impl<$($t: Ask),+> Ask for ($($t,)+) {
            type Out = ($($t::Out,)+);
            type Claim = ($($t::Claim,)+);
            fn encode(self, plan: &mut Plan) -> Result<Self::Claim, Error> {
                Ok(($(self.$i.encode(plan)?,)+))
            }
            fn decode(claim: Self::Claim, reply: &Reply) -> Result<Self::Out, Error> {
                Ok(($($t::decode(claim.$i, reply)?,)+))
            }
        }
    };
}

tuple_ask!(A 0);
tuple_ask!(A 0, B 1);
tuple_ask!(A 0, B 1, C 2);
tuple_ask!(A 0, B 1, C 2, D 3);
tuple_ask!(A 0, B 1, C 2, D 3, E 4);
tuple_ask!(A 0, B 1, C 2, D 3, E 4, F 5);
tuple_ask!(A 0, B 1, C 2, D 3, E 4, F 5, G 6);
tuple_ask!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7);
