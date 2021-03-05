//! Exposes spfs functionality directly to python for use in spk
mod digest;
mod runtime;

use pyo3::{prelude::*, wrap_pyfunction};
use spfs::{self, prelude::*};

pub use self::digest::Digest;
pub use self::runtime::Runtime;
use crate::{storage, Result};

#[pyfunction]
fn configure_logging(_py: Python, verbosity: u64) -> Result<()> {
    match verbosity {
        0 => {
            if std::env::var("SPFS_DEBUG").is_ok() {
                std::env::set_var("RUST_LOG", "spfs=debug");
            } else if std::env::var("RUST_LOG").is_err() {
                std::env::set_var("RUST_LOG", "spfs=info");
            }
        }
        1 => std::env::set_var("RUST_LOG", "spfs=debug"),
        _ => std::env::set_var("RUST_LOG", "spfs=trace"),
    }
    use tracing_subscriber::layer::SubscriberExt;
    let filter = tracing_subscriber::filter::EnvFilter::from_default_env();
    let registry = tracing_subscriber::Registry::default().with(filter);
    let mut fmt_layer = tracing_subscriber::fmt::layer().without_time();
    if verbosity < 3 {
        fmt_layer = fmt_layer.with_target(false);
    }
    let sub = registry.with(fmt_layer);
    tracing::subscriber::set_global_default(sub).unwrap();
    Ok(())
}

#[pyfunction]
fn active_runtime(_py: Python) -> Result<Runtime> {
    let rt = spfs::active_runtime()?;
    Ok(Runtime { inner: rt })
}

#[pyfunction]
fn local_repository(_py: Python) -> Result<storage::SpFSRepository> {
    Ok(storage::local_repository()?)
}

#[pyfunction]
fn remote_repository(_py: Python, path: &str) -> Result<storage::SpFSRepository> {
    Ok(storage::remote_repository(path)?)
}

#[pyfunction]
fn reconfigure_runtime(
    editable: Option<bool>,
    reset: Option<Vec<String>>,
    stack: Option<Vec<Digest>>,
) -> Result<()> {
    let mut runtime = spfs::active_runtime()?;

    // make editable first before trying to make any changes
    runtime.set_editable(true)?;
    spfs::remount_runtime(&runtime)?;

    if let Some(editable) = editable {
        runtime.set_editable(editable)?;
    }
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
}

#[pyfunction(args = "*")]
fn build_shell_initialized_command(cmd: String, args: Vec<String>) -> Result<Vec<String>> {
    let cmd = std::ffi::OsString::from(cmd);
    let mut args = args
        .into_iter()
        .map(|a| std::ffi::OsString::from(a))
        .collect();
    let cmd = spfs::build_shell_initialized_command(cmd, &mut args)?;
    let cmd = cmd
        .into_iter()
        .map(|a| a.to_string_lossy().to_string())
        .collect();
    Ok(cmd)
}

#[pyfunction]
fn build_interactive_shell_command() -> Result<Vec<String>> {
    let cmd = spfs::build_interactive_shell_cmd()?;
    let cmd = cmd
        .into_iter()
        .map(|a| a.to_string_lossy().to_string())
        .collect();
    Ok(cmd)
}

#[pyfunction]
fn commit_layer(runtime: &mut Runtime) -> Result<Digest> {
    let layer = spfs::commit_layer(&mut runtime.inner)?;
    Ok(Digest::from(layer.digest()?))
}

pub(crate) fn init_module(py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(configure_logging, m)?)?;
    m.add_function(wrap_pyfunction!(active_runtime, m)?)?;
    m.add_function(wrap_pyfunction!(local_repository, m)?)?;
    m.add_function(wrap_pyfunction!(remote_repository, m)?)?;
    m.add_function(wrap_pyfunction!(reconfigure_runtime, m)?)?;
    m.add_function(wrap_pyfunction!(build_shell_initialized_command, m)?)?;
    m.add_function(wrap_pyfunction!(build_interactive_shell_command, m)?)?;
    m.add_function(wrap_pyfunction!(commit_layer, m)?)?;

    m.add_class::<Digest>()?;
    m.add_class::<Runtime>()?;

    let empty_spfs: spfs::encoding::Digest = spfs::encoding::EMPTY_DIGEST.into();
    let empty_spk = Digest::from(empty_spfs);
    m.setattr::<&str, PyObject>("spfs.EMPTY_DIGEST", empty_spk.into_py(py))?;

    Ok(())
}
