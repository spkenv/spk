use pyo3::prelude::*;

use spfs;

#[pyclass]
pub struct Runtime {
    pub inner: spfs::runtime::Runtime,
}
