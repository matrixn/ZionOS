//! ZPK v1: typed declarations and bounded inspection. Never extracts or executes packages.
pub mod archive;
pub mod manifest;

pub use archive::{inspect, Inspection, Limits};
pub use manifest::{Compatibility, Manifest};

#[derive(Debug)]
pub enum Error {
    Manifest(String),
    UnsupportedSchema(u32),
    UnsupportedTarget(String),
    Incompatible(String),
    Archive(String),
    Limit(String),
    Hash(String),
    Io(std::io::Error),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Manifest(s) => write!(f, "invalid manifest: {s}"),
            Self::UnsupportedSchema(v) => write!(f, "unsupported ZPK schema version: {v}"),
            Self::UnsupportedTarget(s) => write!(f, "unsupported target: {s}"),
            Self::Incompatible(s) => write!(f, "incompatible package: {s}"),
            Self::Archive(s) => write!(f, "invalid archive: {s}"),
            Self::Limit(s) => write!(f, "package limit exceeded: {s}"),
            Self::Hash(s) => write!(f, "SHA-256 verification failed: {s}"),
            Self::Io(e) => write!(f, "package I/O: {e}"),
        }
    }
}
impl std::error::Error for Error {}
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
pub type Result<T> = std::result::Result<T, Error>;
