//! Local administrator control plane. Storage/provisioning HTTP APIs stay read-only.
pub mod auth;
pub mod broker;
pub mod ipc;
pub mod platform;
pub mod protocol;
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
pub fn fail<T>(s: impl Into<String>) -> Result<T> {
    Err(s.into().into())
}
pub fn random() -> Result<String> {
    Ok(entropy()?.iter().map(|b| format!("{b:02x}")).collect())
}
pub fn entropy() -> Result<[u8; 32]> {
    use std::io::Read;
    let mut bytes = [0; 32];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes)
}
pub fn hash(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}
pub fn token(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
pub fn now() -> Result<i64> {
    Ok(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs() as i64)
}
