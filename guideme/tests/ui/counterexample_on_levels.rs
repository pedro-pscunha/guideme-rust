#[derive(guideme::Levels, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Bad {
    /// low
    #[guide(counterexample = "a blocking outage")]
    Low,
    /// high
    High,
}
fn main() {}
