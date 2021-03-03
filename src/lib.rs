pub mod build;
mod error;
pub mod storage;

pub use error::{Error, Result};

// -- begin python wrappers --

use pyo3::prelude::*;
use spfs;

#[pyclass]
#[derive(Clone)]
pub struct Digest {
    inner: spfs::encoding::Digest,
}

impl From<spfs::encoding::Digest> for Digest {
    fn from(inner: spfs::encoding::Digest) -> Self {
        Self { inner: inner }
    }
}

#[pyclass]
pub struct Runtime {
    inner: spfs::runtime::Runtime,
}

#[pymodule]
fn spkrs(py: Python, m: &PyModule) -> PyResult<()> {
    use self::{build, storage};

    #[pyfn(m, "active_runtime")]
    fn active_runtime(_py: Python) -> PyResult<Runtime> {
        fn v() -> crate::Result<Runtime> {
            let rt = spfs::active_runtime()?;
            Ok(Runtime { inner: rt })
        }
        Ok(v()?)
    }
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
    fn reconfigure_runtime(
        editable: Option<bool>,
        reset: Option<Vec<String>>,
        stack: Option<Vec<Digest>>,
    ) -> PyResult<()> {
        let editable = editable.unwrap_or(false);
        let v = || -> crate::Result<()> {
            let mut runtime = spfs::active_runtime()?;
            runtime.set_editable(editable)?;
            match reset {
                Some(reset) => runtime.reset(reset.as_slice())?,
                None => runtime.reset_all()?,
            }
            runtime.reset_stack()?;
            if let Some(stack) = stack {
                for digest in stack.iter() {
                    runtime.push_digest(&digest.inner)?;
                }
            }
            spfs::remount_runtime(&runtime)?;
            Ok(())
        };
        Ok(v()?)
    }

    m.add_class::<Digest>()?;
    m.add_class::<Runtime>()?;
    m.add_class::<self::storage::SpFSRepository>()?;

    let empty_spfs: spfs::encoding::Digest = spfs::encoding::EMPTY_DIGEST.into();
    let empty_spk = Digest::from(empty_spfs);
    m.setattr::<&str, PyObject>("EMPTY_DIGEST", empty_spk.into_py(py))?;

    Ok(())
}
