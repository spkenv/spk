//! Storage implemetation which is a client of the built-in spfs server

mod database;
mod payload;
mod repository;
mod tag;

mod proto {
    tonic::include_proto!("spfs");
}

pub use repository::RpcRepository;
