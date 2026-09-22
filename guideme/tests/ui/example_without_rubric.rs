#[derive(guideme::Choice, Clone, Copy, PartialEq, Eq)]
enum Bad {
    /// billing
    Billing,
    #[guide(example = "502 on every request")]
    Technical,
}
fn main() {}
