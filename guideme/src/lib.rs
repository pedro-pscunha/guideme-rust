//! Type-safe inline judgments from TypeSafe Jev.
//!
//! ```no_run
//! use guideme::{choose, noul, score, Choice, Guide, Levels, Policy};
//!
//! #[derive(Choice, Clone, Copy, PartialEq, Eq, Debug)]
//! enum Department {
//!     /// Payments, invoicing, refunds
//!     Billing,
//!     /// Bugs, outages, integrations
//!     Technical,
//!     /// Pricing, upgrades, new accounts
//!     #[guide(fallback)]
//!     Sales,
//! }
//!
//! #[derive(Levels, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
//! enum Frustration {
//!     /// Calm and polite
//!     Calm,
//!     /// Frustrated
//!     Frustrated,
//!     /// Very angry
//!     VeryAngry,
//! }
//!
//! # async fn demo(ticket: &str) -> Result<(), guideme::Error> {
//! let guide = Guide::from_env()?; // TYPESAFE_API_KEY
//!
//! if guide.ask(noul("Should this ticket be escalated?"), ticket).await? {
//!     // escalate
//! }
//!
//! match guide.ask(choose::<Department>("Which team should handle this?"), ticket).await? {
//!     Department::Billing => {}
//!     Department::Technical => {}
//!     Department::Sales => {}
//! }
//!
//! if guide.ask(score::<Frustration>("How frustrated is the customer?"), ticket).await?
//!     >= Frustration::Frustrated
//! {
//!     // prioritise
//! }
//!
//! // three judgments, one request, one span
//! let (urgent, dept, mood) = guide
//!     .ask(
//!         (
//!             noul("Is this urgent?").yes_above(0.7).no_below(0.3).or(false),
//!             choose::<Department>("Which team?").with(Policy::new().min_confidence(0.6)),
//!             score::<Frustration>("How frustrated?").detail(),
//!         ),
//!         ticket,
//!     )
//!     .await?;
//! # let _ = (urgent, dept, mood);
//! # Ok(()) }
//! ```

pub mod api;
mod ask;
mod error;
mod guide;
pub mod policy;
mod question;
mod scalars;
#[doc(hidden)]
pub mod spec;

pub use ask::Ask;
pub use error::Error;
pub use guide::{Guide, GuideBuilder};
pub use guideme_derive::{Choice, Levels};
pub use policy::{Policy, Thresholds, Verdict};
pub use question::{
    Binary, Choose, Confident, Detailed, Fallible, Key, Kind, Levels, Noul, Options, Question,
    Rank, Ranked, Score, Scored, choose, choose_among, noul, score, score_levels,
};
pub use scalars::{ApiKey, Confidence, Instructions, Model, Probability, State};
