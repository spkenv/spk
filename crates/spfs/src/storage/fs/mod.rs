//! Uses a local directory on disk to store the spfs repository.

mod database;
mod hash_store;
mod payloads;
mod renderer;
mod repository;
mod tag;

pub mod migrations;

pub use hash_store::FSHashStore;
pub use renderer::RenderType;
pub use repository::{read_last_migration_version, FSRepository};
