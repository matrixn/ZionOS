//! Forge compiles source only inside a disposable constrained builder. Packaging is offline.
pub mod config;
pub mod elf;
pub mod files;
pub mod sandbox;
pub mod target;
pub mod workflow;
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
pub fn fail<T>(message: impl Into<String>) -> Result<T> {
    Err(message.into().into())
}
pub fn hash(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}
pub fn read_limited(path: &std::path::Path, limit: u64) -> Result<Vec<u8>> {
    use std::io::Read;
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > limit {
        return fail(format!("not a bounded regular file: {}", path.display()));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return fail("file grew beyond size limit");
    }
    Ok(bytes)
}
