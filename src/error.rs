use pyo3::{exceptions, prelude::*};
use spfs;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    SPFS(spfs::Error),
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
