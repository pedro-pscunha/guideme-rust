#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::pedantic,
    missing_docs
)]

use std::collections::BTreeMap;

use guideme::api::Answer;
use guideme::policy::{Outcome, Thresholds, resolve};
use guideme::{Error, Policy, Probability, Verdict};
use proptest::prelude::*;

fn unit() -> impl Strategy<Value = f64> {
    (0u32..=1000).prop_map(|n| f64::from(n) / 1000.0)
}

fn thresholds() -> impl Strategy<Value = Thresholds> {
    (unit(), unit(), unit()).prop_map(|(a, b, c)| Thresholds::new(a.max(b), a.min(b), c).unwrap())
}

fn p(v: f64) -> Probability {
    Probability::try_from(v).unwrap()
}

fn rank(v: f64, t: Thresholds) -> u8 {
    match resolve(&Answer::Noul { noul: p(v) }, t).unwrap() {
        Outcome::Noul(Verdict::No(_)) => 0,
        Outcome::Noul(Verdict::Unsure(_)) => 1,
        Outcome::Noul(Verdict::Yes(_)) => 2,
        Outcome::Choice { .. } | Outcome::Score { .. } => panic!("kind"),
    }
}

proptest! {
    #[test]
    fn noul_is_unsure_exactly_inside_the_open_band(v in unit(), t in thresholds()) {
        let expected = if v >= t.yes_above() { 2 } else if v <= t.no_below() { 0 } else { 1 };
        prop_assert_eq!(rank(v, t), expected);
    }

    #[test]
    fn noul_verdict_is_monotone_in_probability_and_thresholds(
        a in unit(), b in unit(), lo in unit(), hi in unit(), t in thresholds()
    ) {
        let (p_lo, p_hi) = (a.min(b), a.max(b));
        prop_assert!(rank(p_lo, t) <= rank(p_hi, t));

        // raising yes_above (keeping no_below) can only lower the verdict
        let (y1, y2) = (lo.max(t.no_below()).min(hi.max(t.no_below())), lo.max(t.no_below()).max(hi.max(t.no_below())));
        let t1 = Thresholds::new(y1, t.no_below(), t.min_confidence()).unwrap();
        let t2 = Thresholds::new(y2, t.no_below(), t.min_confidence()).unwrap();
        prop_assert!(rank(a, t1) >= rank(a, t2));

        // raising no_below (keeping yes_above) can only lower the verdict
        let (n1, n2) = (lo.min(t.yes_above()).min(hi.min(t.yes_above())), lo.min(t.yes_above()).max(hi.min(t.yes_above())));
        let t3 = Thresholds::new(t.yes_above(), n1, t.min_confidence()).unwrap();
        let t4 = Thresholds::new(t.yes_above(), n2, t.min_confidence()).unwrap();
        prop_assert!(rank(a, t3) >= rank(a, t4));
    }

    #[test]
    fn choice_is_unsure_iff_confidence_below_threshold(
        probs in prop::collection::btree_map("[a-z]{1,4}", unit(), 1..5), c in unit(), t in thresholds()
    ) {
        let choice = probs.iter().max_by(|x, y| x.1.total_cmp(y.1)).unwrap().0.clone();
        let answer = Answer::Choice {
            choice: choice.clone(),
            probabilities: probs.iter().map(|(k, v)| (k.clone(), p(*v))).collect(),
            confidence: c.try_into().unwrap(),
        };
        let Outcome::Choice { key, unsure, ranked, .. } = resolve(&answer, t)? else { panic!("kind") };
        prop_assert_eq!(key, choice);
        prop_assert_eq!(unsure, c < t.min_confidence());
        prop_assert!(ranked.windows(2).all(|w| w[0].1 >= w[1].1));
    }

    #[test]
    fn score_index_is_the_argmax_and_value_stays_in_range(
        probs in prop::collection::vec(unit(), 2..=10), c in unit(), t in thresholds(), pos in unit()
    ) {
        let n = probs.len();
        let value = pos * (n - 1) as f64;
        let answer = Answer::Score {
            score: value,
            legend: (0..n as u8).map(|i| (i, format!("L{i}"))).collect(),
            probabilities: probs.iter().enumerate().map(|(i, v)| (i as u8, p(*v))).collect(),
            confidence: c.try_into().unwrap(),
        };
        let Outcome::Score { index, value: got, unsure, distribution, .. } = resolve(&answer, t)? else { panic!("kind") };
        let argmax = probs.iter().enumerate().fold(0, |best, (i, v)| if *v > probs[best] { i } else { best });
        prop_assert_eq!(index, argmax);
        prop_assert!((got - value).abs() < f64::EPSILON);
        prop_assert_eq!(unsure, c < t.min_confidence());
        prop_assert_eq!(distribution.len(), n);
    }
}

#[test]
fn malformed_answers_are_protocol_errors() {
    let t = Thresholds::default();
    let ghost = Answer::Choice {
        choice: "ghost".into(),
        probabilities: BTreeMap::from([("a".to_owned(), p(1.0))]),
        confidence: 1.0f64.try_into().unwrap(),
    };
    assert!(matches!(resolve(&ghost, t), Err(Error::Protocol { .. })));

    let eleven = Answer::Score {
        score: 0.0,
        legend: (0u8..11).map(|i| (i, format!("L{i}"))).collect(),
        probabilities: (0u8..11)
            .map(|i| (i, p(if i == 0 { 1.0 } else { 0.0 })))
            .collect(),
        confidence: 1.0f64.try_into().unwrap(),
    };
    assert!(matches!(resolve(&eleven, t), Err(Error::Protocol { .. })));
}

#[test]
fn policy_patch_precedence_and_settle_validation() {
    const HOUSE: Policy = Policy::new().yes_above(0.7).no_below(0.3);
    let call = Policy::new().min_confidence(0.6);
    let settled = call.over(HOUSE).settle().unwrap();
    assert_eq!(settled, Thresholds::new(0.7, 0.3, 0.6).unwrap());
    assert!(matches!(
        Policy::new().yes_above(0.2).no_below(0.8).settle(),
        Err(Error::Config { .. })
    ));
    assert!(matches!(
        Policy::new().min_confidence(1.5).settle(),
        Err(Error::Config { .. })
    ));
    assert!(
        serde_json::from_str::<Thresholds>(
            r#"{"yes_above":0.2,"no_below":0.8,"min_confidence":0}"#
        )
        .is_err()
    );
}
