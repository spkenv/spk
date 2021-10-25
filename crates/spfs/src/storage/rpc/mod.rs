//! Storage implemetation which is a client of the built-in spfs server

mod database;
mod payload;
mod repository;
mod tag;

pub use repository::RpcRepository;
