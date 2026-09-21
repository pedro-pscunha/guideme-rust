#[derive(guideme::Choice, Clone, Copy, PartialEq, Eq)]
enum Bad {
    #[guide(key = "same")]
    A,
    #[guide(key = "same")]
    B,
}

fn main() {}
