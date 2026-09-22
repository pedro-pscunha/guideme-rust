#[derive(guideme::Choice, Clone, Copy, PartialEq, Eq)]
enum Bad {
    /// billing
    #[guide(example = "Where is my refund?", example = "Where is my refund?")]
    Billing,
    /// technical
    Technical,
}
fn main() {}
