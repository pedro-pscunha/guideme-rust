#[derive(guideme::Choice, Clone, Copy, PartialEq, Eq)]
enum Bad {
    /// billing
    #[guide(example = "Where is my refund?")]
    Billing,
    /// technical
    #[guide(example = "Where is my refund?")]
    Technical,
}
fn main() {}
