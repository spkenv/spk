pub mod build;
mod error;
pub mod storage;

pub use error::{Error, Result};

// -- begin python wrappers --

use pyo3::prelude::*;
use spfs;

#[pyclass]
struct Digest {
    inner: spfs::encoding::Digest,
}

#[pyclass]
struct Runtime {
    inner: spfs::runtime::Runtime,
}

#[pymodule]
fn spkrs(_py: Python, m: &PyModule) -> PyResult<()> {
    use self::{build, storage};

    #[pyfn(m, "local_repository")]
    fn local_repository(_py: Python) -> PyResult<storage::SpFSRepository> {
        Ok(storage::local_repository()?)
    }
    #[pyfn(m, "remote_repository")]
    fn remote_repository(_py: Python, path: &str) -> PyResult<storage::SpFSRepository> {
        Ok(storage::remote_repository(path)?)
    }
    #[pyfn(m, "validate_build_changeset")]
    fn validate_build_changeset() -> PyResult<()> {
        fn v() -> crate::Result<()> {
            let diffs = spfs::diff(None, None)?;
            build::validate_build_changeset(diffs, "/spfs")?;
            Ok(())
        }
        Ok(v()?)
    }
    #[pyfn(m, "validate_source_changeset")]
    fn validate_source_changeset() -> PyResult<()> {
        fn v() -> crate::Result<()> {
            let diffs = spfs::diff(None, None)?;
            build::validate_source_changeset(diffs, "/spfs")?;
            Ok(())
        }
        Ok(v()?)
    }
    #[pyfn(m, "reconfigure_runtime")]
    fn reconfigure_runtime(editable: Option<bool>) -> PyResult<()> {
        let editable = editable.unwrap_or(false);
        let v = || -> crate::Result<()> {
            let mut runtime = spfs::active_runtime()?;
            runtime.set_editable(editable)?;
            runtime.reset_all()?;
            runtime.reset_stack()?;
            spfs::remount_runtime(&runtime)?;
            Ok(())
        };
        Ok(v()?)
    }

    m.add_class::<Digest>()?;
    m.add_class::<Runtime>()?;
    m.add_class::<self::storage::SpFSRepository>()?;
    Ok(())
}
