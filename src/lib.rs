// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
pub mod api;
pub mod build;
mod env;
mod error;
pub mod exec;
mod global;
pub mod io;
mod publish;
pub mod solve;
pub mod storage;
pub mod test;

#[cfg(test)]
mod fixtures;

pub use env::current_env;
pub use error::{Error, Result};
pub use global::{load_spec, save_spec};
pub use publish::Publisher;
pub use solve::{Solution, Solver};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

lazy_static::lazy_static! {
    pub static ref HANDLE: tokio::runtime::Handle = {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        let handle = rt.handle().clone();
        std::thread::spawn(move || rt.block_on(futures::future::pending::<()>()));
        handle
    };
}

// -- begin python wrappers --

use pyo3::prelude::*;
use spfs::{self, prelude::*};

#[pyclass]
#[derive(Clone, PartialEq, Eq)]
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
    fn __richcmp__(&self, other: Self, op: pyo3::class::basic::CompareOp) -> bool {
        use pyo3::class::basic::CompareOp;
        match op {
            CompareOp::Eq => self == &other,
            CompareOp::Ne => self != &other,
            _ => false,
        }
    }

    fn __hash__(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.inner.hash(&mut hasher);
        hasher.finish()
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

    let test_mod = PyModule::new(py, "test")?;
    test::python::init_module(&py, test_mod)?;
    m.add_submodule(test_mod)?;

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
    sys.modules['spkrs.test'] = test;\
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
        HANDLE.block_on(spfs::remount_runtime(&runtime))?;

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
        HANDLE.block_on(spfs::remount_runtime(&runtime))?;
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
        let layer = crate::HANDLE.block_on(spfs::commit_layer(&mut runtime.inner))?;
        Ok(Digest::from(layer.digest()?))
    }

    #[pyfn(m)]
    #[pyo3(name = "render_into_dir")]
    fn render_into_dir(stack: Vec<Digest>, path: &str) -> Result<()> {
        let items: Vec<String> = stack.into_iter().map(|d| d.inner.to_string()).collect();
        let env_spec = spfs::tracking::EnvSpec::new(items.join("+").as_ref())?;
        crate::HANDLE.block_on(spfs::render_into_directory(&env_spec, path))?;
        Ok(())
    }

    #[pyfn(m)]
    #[pyo3(name = "reload_config")]
    fn reload_config() -> Result<()> {
        let config = spfs::load_config()?;
        config.make_current()?;
        Ok(())
    }

    #[pyfn(m)]
    fn current_env() -> Result<solve::Solution> {
        let _guard = crate::HANDLE.enter();
        env::current_env()
    }

    #[pyfn(m)]
    fn load_spec(pkg: api::Ident) -> Result<api::Spec> {
        let _guard = crate::HANDLE.enter();
        global::load_spec(pkg.to_string().as_str())
    }

    #[pyfn(m)]
    fn save_spec(spec: api::Spec) -> Result<()> {
        let _guard = crate::HANDLE.enter();
        global::save_spec(spec)
    }

    m.add_class::<Publisher>()?;
    m.add_class::<Digest>()?;
    m.add_class::<Runtime>()?;

    let empty_spfs: spfs::encoding::Digest = spfs::encoding::EMPTY_DIGEST.into();
    let empty_spk = Digest::from(empty_spfs);
    m.setattr::<&str, PyObject>("EMPTY_DIGEST", empty_spk.into_py(py))?;

    m.add(
        "NoEnvironmentError",
        py.get_type::<env::NoEnvironmentError>(),
    )?;

    Ok(())
}
