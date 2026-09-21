#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::pedantic,
    missing_docs
)]

use guideme::{Choice, Levels, Options};

#[derive(Choice, Clone, Copy, PartialEq, Eq, Debug)]
enum Department {
    /// Payments, invoicing, refunds
    Billing,
    #[guide(key = "tech", rubric = "Bugs, outages, integrations")]
    Technical,
    /// Pricing, upgrades, new accounts
    #[guide(fallback)]
    Sales,
    NoRubric,
}

#[derive(Levels, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Frustration {
    /// Calm and polite
    Calm,
    /// Frustrated
    Frustrated,
    /// Very angry
    VeryAngry,
}

#[test]
fn choice_rubric_keys_and_fallback_come_from_the_enum() {
    assert_eq!(
        Department::RUBRIC,
        &[
            ("billing", Some("Payments, invoicing, refunds")),
            ("tech", Some("Bugs, outages, integrations")),
            ("sales", Some("Pricing, upgrades, new accounts")),
            ("no_rubric", None),
        ]
    );
    assert_eq!(Department::fallback(), Some(Department::Sales));
    assert_eq!(
        <Frustration as guideme::Levels>::LEVELS,
        &["Calm and polite", "Frustrated", "Very angry"]
    );
}

#[test]
fn choice_key_round_trips_every_variant() {
    const IN_ORDER: [Department; 4] = [
        Department::Billing,
        Department::Technical,
        Department::Sales,
        Department::NoRubric,
    ];
    for (i, (key, _)) in Department::RUBRIC.iter().enumerate() {
        assert_eq!(Department::from_key(key), Some(IN_ORDER[i]));
    }
    assert_eq!(Department::from_key("nope"), None);
}

#[test]
fn level_index_round_trips_in_declaration_order() {
    let count = <Frustration as guideme::Levels>::LEVELS.len();
    let mut previous: Option<Frustration> = None;
    for i in 0..count {
        let level = <Frustration as guideme::Levels>::from_index(i).unwrap();
        assert_eq!(level.index(), i);
        if let Some(lower) = previous {
            assert!(level > lower);
        }
        previous = Some(level);
    }
    assert_eq!(<Frustration as guideme::Levels>::from_index(count), None);
}

#[test]
fn misuse_is_a_compile_error_naming_the_rule() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
