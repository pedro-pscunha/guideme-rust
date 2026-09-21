#[derive(guideme::Levels, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Bad {
    /// low
    #[guide(key = "lo")]
    Low,
    /// high
    High,
}
fn main() {}
