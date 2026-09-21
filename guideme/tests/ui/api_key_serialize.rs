// The API key must never be serialised. `Serialize` is deliberately absent.
fn main() {
    let key = guideme::ApiKey::from("secret");
    let _ = serde_json::to_string(&key);
}
