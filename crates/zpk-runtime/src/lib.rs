//! Native ZPK runtime. No builder, Docker/OCI integration or package install hooks.
pub mod backend;
pub mod config;
pub mod installed;
pub mod launch;
pub mod policy;
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
pub fn fail<T>(message: impl Into<String>) -> Result<T> {
    Err(message.into().into())
}
pub fn digest(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}
