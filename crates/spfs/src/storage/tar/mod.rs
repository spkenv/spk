//! An spfs storage implementation where all data is in a single
//! tar file. This is best used as single write, many read
//! archive format as modifying the tar in place is slow.

mod payloads;
pub use payloads::TarPayloadStorage;
mod database;
pub use database::TarDatabase;
mod repository;
pub use repository::TarRepository;
mod tag;
pub use tag::TagStorage;
