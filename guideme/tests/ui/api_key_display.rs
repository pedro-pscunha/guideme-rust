// The API key must never be printable. `Display` is deliberately absent.
fn main() {
    let key = guideme::ApiKey::from("secret");
    let _ = format!("{key}");
}
