//! Local native package transactions. No HTTP, catalog, builder or package hooks.
pub mod database;
pub mod desktop;
pub mod filesystem;
pub mod host;
pub mod manager;
pub mod model;
pub mod resolver;
pub use zpk_runtime::{digest, fail, Result};
