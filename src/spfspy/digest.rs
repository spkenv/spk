use pyo3::prelude::*;

use spfs;

use crate::Result;

#[pyclass]
#[derive(Clone)]
pub struct Digest {
    pub inner: spfs::encoding::Digest,
}

#[pyproto]
impl pyo3::PyObjectProtocol for Digest {
    fn __str__(&self) -> Result<String> {
        Ok(self.inner.to_string())
    }
    fn __repr__(&self) -> Result<String> {
        Ok(self.inner.to_string())
    }
}

impl From<spfs::encoding::Digest> for Digest {
    fn from(inner: spfs::encoding::Digest) -> Self {
        Self { inner: inner }
    }
}
