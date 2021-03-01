mod error;
pub mod storage;

use pyo3::prelude::*;
use spfs;

pub use error::{Error, Result};

#[pyclass]
struct Digest {
    inner: spfs::encoding::Digest,
}

#[pymodule]
fn spkrs(_py: Python, m: &PyModule) -> PyResult<()> {
    use self::storage;

    #[pyfn(m, "local_repository")]
    fn local_repository(_py: Python) -> PyResult<storage::SpFSRepository> {
        Ok(storage::local_repository()?)
    }
    #[pyfn(m, "remote_repository")]
    fn remote_repository(_py: Python, path: &str) -> PyResult<storage::SpFSRepository> {
        Ok(storage::remote_repository(path)?)
    }

    m.add_class::<Digest>()?;
    m.add_class::<self::storage::SpFSRepository>()?;
    Ok(())
}
