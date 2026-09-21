//! Validated scalar newtypes shared by every layer.

use std::borrow::Cow;
use std::fmt;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Serialize};

use crate::Error;

fn unit_interval_schema(name: &str) -> Schema {
    json_schema!({
        "type": "number",
        "minimum": 0.0,
        "maximum": 1.0,
        "description": format!("{name} in the closed unit interval"),
    })
}

/// A probability in `0..=1`, validated once at the wire.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "f64", into = "f64")]
pub struct Probability(f64);

impl Probability {
    /// The raw value.
    pub const fn get(self) -> f64 {
        self.0
    }
}

impl TryFrom<f64> for Probability {
    type Error = Error;
    fn try_from(value: f64) -> Result<Self, Error> {
        if (0.0..=1.0).contains(&value) {
            Ok(Self(value))
        } else {
            Err(Error::Protocol {
                detail: format!("probability {value} is outside 0..=1"),
            })
        }
    }
}

impl From<Probability> for f64 {
    fn from(p: Probability) -> f64 {
        p.0
    }
}

impl JsonSchema for Probability {
    fn schema_name() -> Cow<'static, str> {
        "Probability".into()
    }
    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        unit_interval_schema("probability")
    }
}

/// A confidence in `0..=1`, validated once at the wire. Absent on noul answers.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "f64", into = "f64")]
pub struct Confidence(f64);

impl Confidence {
    /// The raw value.
    pub const fn get(self) -> f64 {
        self.0
    }
}

impl TryFrom<f64> for Confidence {
    type Error = Error;
    fn try_from(value: f64) -> Result<Self, Error> {
        if (0.0..=1.0).contains(&value) {
            Ok(Self(value))
        } else {
            Err(Error::Protocol {
                detail: format!("confidence {value} is outside 0..=1"),
            })
        }
    }
}

impl From<Confidence> for f64 {
    fn from(c: Confidence) -> f64 {
        c.0
    }
}

impl JsonSchema for Confidence {
    fn schema_name() -> Cow<'static, str> {
        "Confidence".into()
    }
    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        unit_interval_schema("confidence")
    }
}

/// A TypeSafe API key. `Debug` never prints it.
#[derive(Clone, PartialEq, Eq)]
pub struct ApiKey(String);

impl ApiKey {
    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl From<String> for ApiKey {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for ApiKey {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

impl fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ApiKey(***)")
    }
}

/// A model name or alias accepted by the `model` field.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct Model(String);

impl Model {
    /// `jev-latest`: the most recent stable release. The crate default.
    pub fn latest() -> Self {
        Self("jev-latest".to_owned())
    }
    /// `jev-preview`: the most recent release, stable or not.
    pub fn preview() -> Self {
        Self("jev-preview".to_owned())
    }
    /// A pinned version such as `jev-1.13.0`.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
    /// The name as sent on the wire.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The content the model evaluates: text, or any JSON-serialisable structure.
///
/// Built by `From`: `"text"`, `String`, `serde_json::Value`, or `&T` for any `T: Serialize`
/// (pass a reference; an owned struct is not accepted, by coherence). A value that cannot be
/// serialised (non-string map keys, NaN) is carried as invalid and rejected with
/// [`Error::Config`] when asked, never silently replaced.
#[derive(Clone, Debug, PartialEq)]
pub struct State(pub(crate) Repr);

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Repr {
    Ready(serde_json::Value),
    Invalid(String),
}

impl State {
    pub(crate) fn value(&self) -> Result<&serde_json::Value, Error> {
        match &self.0 {
            Repr::Ready(v) => Ok(v),
            Repr::Invalid(detail) => Err(Error::Config {
                detail: format!("state is not serialisable: {detail}"),
            }),
        }
    }
}

impl<T: Serialize + ?Sized> From<&T> for State {
    fn from(value: &T) -> Self {
        Self(match serde_json::to_value(value) {
            Ok(v) => Repr::Ready(v),
            Err(e) => Repr::Invalid(e.to_string()),
        })
    }
}

impl From<String> for State {
    fn from(text: String) -> Self {
        Self(Repr::Ready(serde_json::Value::String(text)))
    }
}

impl From<serde_json::Value> for State {
    fn from(value: serde_json::Value) -> Self {
        Self(Repr::Ready(value))
    }
}

impl Serialize for State {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match &self.0 {
            Repr::Ready(v) => v.serialize(s),
            Repr::Invalid(detail) => Err(serde::ser::Error::custom(detail)),
        }
    }
}

impl<'de> Deserialize<'de> for State {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        serde_json::Value::deserialize(d).map(|v| Self(Repr::Ready(v)))
    }
}

impl JsonSchema for State {
    fn schema_name() -> Cow<'static, str> {
        "State".into()
    }
    fn json_schema(g: &mut SchemaGenerator) -> Schema {
        <serde_json::Value as JsonSchema>::json_schema(g)
    }
}

/// The question text, or a structured object holding the question plus data it references.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct Instructions(pub(crate) serde_json::Value);

impl From<&str> for Instructions {
    fn from(s: &str) -> Self {
        Self(serde_json::Value::String(s.to_owned()))
    }
}

impl From<String> for Instructions {
    fn from(s: String) -> Self {
        Self(serde_json::Value::String(s))
    }
}

impl From<serde_json::Value> for Instructions {
    fn from(v: serde_json::Value) -> Self {
        Self(v)
    }
}
