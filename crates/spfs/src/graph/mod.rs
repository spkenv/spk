///! Low-level digraph representation and manipulation for data storage.
mod database;
mod error;
mod operations;

pub use error::{
    AmbiguousReferenceError, Error, Result, UnknownObjectError, UnknownReferenceError,
};

pub use database::{Database, DatabaseView};
pub use operations::check_database_integrity;
