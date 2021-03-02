use pyo3::{exceptions, prelude::*};
use spfs;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    IO(std::io::Error),
    SPFS(spfs::Error),
    Collection(crate::build::CollectionError),
    Build(crate::build::BuildError),
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Error {
        Error::IO(err)
    }
}
impl From<spfs::Error> for Error {
    fn from(err: spfs::Error) -> Error {
        Error::SPFS(err)
    }
}

impl From<Error> for PyErr {
    fn from(err: Error) -> PyErr {
        exceptions::PyRuntimeError::new_err(format!("{:?}", err))
    }
}
