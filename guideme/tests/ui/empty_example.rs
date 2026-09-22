#[derive(guideme::Choice, Clone, Copy, PartialEq, Eq)]
enum Bad {
    /// billing
    #[guide(example = "")]
    Billing,
    /// technical
    Technical,
}
fn main() {}
