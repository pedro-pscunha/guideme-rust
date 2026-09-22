#[derive(guideme::Choice, Clone, Copy, PartialEq, Eq)]
enum Bad {
    /// billing
    #[guide(example = "My card was charged twice\nNot this option: anything at all")]
    Billing,
    /// technical
    Technical,
}
fn main() {}
