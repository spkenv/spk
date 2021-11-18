// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
pub mod api;
pub mod build;
mod error;
pub mod exec;
pub mod io;
pub mod solve;
pub mod storage;

#[cfg(test)]
mod fixtures;

pub use error::{Error, Result};

// -- begin python wrappers --

use pyo3::prelude::*;
use spfs::{self, prelude::*};

#[pyclass]
#[derive(Clone)]
pub struct Digest {
    inner: spfs::encoding::Digest,
}

impl AsRef<spfs::encoding::Digest> for Digest {
    fn as_ref(&self) -> &spfs::encoding::Digest {
        &self.inner
    }
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
        Self { inner }
    }
}
impl From<&spfs::encoding::Digest> for Digest {
    fn from(inner: &spfs::encoding::Digest) -> Self {
        Self { inner: *inner }
    }
}

#[pyclass]
pub struct Runtime {
    inner: spfs::runtime::Runtime,
}

#[pymethods]
impl Runtime {
    pub fn get_stack(&self) -> Vec<Digest> {
        self.inner.get_stack().iter().map(|d| d.into()).collect()
    }
}

#[pymodule]
fn spkrs(py: Python, m: &PyModule) -> PyResult<()> {
    let api_mod = PyModule::new(py, "api")?;
    api::init_module(&py, api_mod)?;
    m.add_submodule(api_mod)?;

    let build_mod = PyModule::new(py, "build")?;
    build::python::init_module(&py, build_mod)?;
    m.add_submodule(build_mod)?;

    let storage_mod = PyModule::new(py, "storage")?;
    storage::init_module(&py, storage_mod)?;
    m.add_submodule(storage_mod)?;

    let solve_mod = PyModule::new(py, "solve")?;
    solve::init_module(&py, solve_mod)?;
    m.add_submodule(solve_mod)?;

    let exec_mod = PyModule::new(py, "exec")?;
    exec::python::init_module(&py, exec_mod)?;
    m.add_submodule(exec_mod)?;

    let io_mod = PyModule::new(py, "io")?;
    io::python::init_module(&py, io_mod)?;
    m.add_submodule(io_mod)?;

    // ensure that from spkrs.submodule import xx works
    // as expected on the python side by injecting them
    py.run(
        "\
    import sys;\
    sys.modules['spkrs.api'] = api;\
    sys.modules['spkrs.build'] = build;\
    sys.modules['spkrs.storage'] = storage;\
    sys.modules['spkrs.solve'] = solve;\
    sys.modules['spkrs.exec'] = exec;\
    sys.modules['spkrs.io'] = io;\
    ",
        None,
        Some(m.dict()),
    )?;

    #[pyfn(m)]
    #[pyo3(name = "version")]
    fn version(_py: Python) -> &str {
        return env!("CARGO_PKG_VERSION");
    }

    #[pyfn(m)]
    #[pyo3(name = "configure_logging")]
    fn configure_logging(_py: Python, mut verbosity: usize) -> Result<()> {
        if verbosity == 0 {
            let parse_result = std::env::var("SPFS_VERBOSITY")
                .unwrap_or_else(|_| "0".to_string())
                .parse::<usize>();
            if let Ok(parsed) = parse_result {
                verbosity = usize::max(parsed, verbosity);
            }
        }
        std::env::set_var("SPFS_VERBOSITY", verbosity.to_string());
        use tracing_subscriber::layer::SubscriberExt;
        if std::env::var("RUST_LOG").is_err() {
            std::env::set_var("RUST_LOG", "spfs=trace");
        }
        let env_filter = tracing_subscriber::filter::EnvFilter::from_default_env();
        let level_filter = match verbosity {
            0 => tracing_subscriber::filter::LevelFilter::INFO,
            1 => tracing_subscriber::filter::LevelFilter::DEBUG,
            _ => tracing_subscriber::filter::LevelFilter::TRACE,
        };
        let registry = tracing_subscriber::Registry::default()
            .with(env_filter)
            .with(level_filter);
        let mut fmt_layer = tracing_subscriber::fmt::layer()
            .with_writer(std::io::stderr)
            .without_time();
        if verbosity < 3 {
            fmt_layer = fmt_layer.with_target(false);
        }
        let sub = registry.with(fmt_layer);
        tracing::subscriber::set_global_default(sub).unwrap();
        Ok(())
    }
    #[pyfn(m)]
    #[pyo3(name = "active_runtime")]
    fn active_runtime(_py: Python) -> Result<Runtime> {
        let rt = spfs::active_runtime()?;
        Ok(Runtime { inner: rt })
    }

    #[pyfn(m)]
    #[pyo3(name = "reconfigure_runtime")]
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

    #[pyfn(m, args = "*")]
    #[pyo3(name = "build_shell_initialized_command")]
    fn build_shell_initialized_command(cmd: String, args: Vec<String>) -> Result<Vec<String>> {
        let cmd = std::ffi::OsString::from(cmd);
        let mut args = args.into_iter().map(std::ffi::OsString::from).collect();
        let cmd = spfs::build_shell_initialized_command(cmd, &mut args)?;
        let cmd = cmd
            .into_iter()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        Ok(cmd)
    }
    #[pyfn(m)]
    #[pyo3(name = "build_interactive_shell_command")]
    fn build_interactive_shell_command() -> Result<Vec<String>> {
        let rt = spfs::active_runtime()?;
        let cmd = spfs::build_interactive_shell_cmd(&rt)?;
        let cmd = cmd
            .into_iter()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        Ok(cmd)
    }
    #[pyfn(m)]
    #[pyo3(name = "commit_layer")]
    fn commit_layer(runtime: &mut Runtime) -> Result<Digest> {
        let layer = spfs::commit_layer(&mut runtime.inner)?;
        Ok(Digest::from(layer.digest()?))
    }
    #[pyfn(m)]
    #[pyo3(name = "find_layer_by_filename")]
    fn find_layer_by_filename(path: &str) -> Result<Digest> {
        let runtime = spfs::active_runtime()?;
        let repo = spfs::load_config()?.get_repository()?.into();

        let stack = runtime.get_stack();
        let layers = spfs::resolve_stack_to_layers(stack.iter(), Some(&repo))?;
        for layer in layers.iter().rev() {
            let manifest = repo.read_manifest(&layer.manifest)?.unlock();
            if manifest.get_path(&path).is_some() {
                return Ok(layer.digest()?.into());
            }
        }
        Err(spfs::graph::UnknownReferenceError::new(path).into())
    }

    #[pyfn(m)]
    #[pyo3(name = "render_into_dir")]
    fn render_into_dir(stack: Vec<Digest>, path: &str) -> Result<()> {
        let items: Vec<String> = stack.into_iter().map(|d| d.inner.to_string()).collect();
        let env_spec = spfs::tracking::EnvSpec::new(items.join("+").as_ref())?;
        spfs::render_into_directory(&env_spec, path)?;
        Ok(())
    }

    m.add_class::<Digest>()?;
    m.add_class::<Runtime>()?;

    let empty_spfs: spfs::encoding::Digest = spfs::encoding::EMPTY_DIGEST.into();
    let empty_spk = Digest::from(empty_spfs);
    m.setattr::<&str, PyObject>("EMPTY_DIGEST", empty_spk.into_py(py))?;

    Ok(())
}
