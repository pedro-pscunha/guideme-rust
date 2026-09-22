#[derive(guideme::Choice, Clone, Copy, PartialEq, Eq)]
enum Bad {
    /// billing
    #[guide(
        counterexample = "The dashboard is down",
        counterexample = "The dashboard is down"
    )]
    Billing,
    /// technical
    Technical,
}
fn main() {}
