#[derive(guideme::Choice, Clone, Copy, PartialEq, Eq)]
enum Bad {
    #[guide(rubric = "   ", example = "My card was charged twice")]
    Billing,
    /// technical
    Technical,
}
fn main() {}
