#[derive(guideme::Choice, Clone, Copy, PartialEq, Eq)]
enum Bad {
    #[guide(fallback)]
    A,
    #[guide(fallback)]
    B,
}

fn main() {}
