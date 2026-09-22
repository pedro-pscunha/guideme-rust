#[derive(guideme::Choice, Clone, Copy, PartialEq, Eq)]
enum Bad {
    /// billing
    #[guide(counterexample = "  ")]
    Billing,
    /// technical
    Technical,
}
fn main() {}
