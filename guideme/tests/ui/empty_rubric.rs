#[derive(guideme::Choice, Clone, Copy, PartialEq, Eq)]
enum Bad {
    #[guide(rubric = "   ")]
    Billing,
    /// technical
    Technical,
}
fn main() {}
