use super::config::get_config;
use super::status::remount_runtime;
use crate::prelude::*;
use crate::{graph, runtime, Error, Result};

#[cfg(test)]
#[path = "./commit_test.rs"]
mod commit_test;

/// Denotes that a layer or manifest either is or would be empty.
#[derive(Debug)]
pub struct NothingToCommitError {
    message: String,
}

impl NothingToCommitError {
    pub fn new<S: AsRef<str>>(message: S) -> Error {
        Error::NothingToCommit(Self {
            message: format!("Nothing to commit: {}", message.as_ref()),
        })
    }
}

/// Commit the working file changes of a runtime to a new layer.
pub fn commit_layer(runtime: &mut runtime::Runtime) -> Result<graph::Layer> {
    let config = get_config()?;
    let mut repo = config.get_repository()?;
    let manifest = repo.commit_dir(runtime.upper_dir.as_path())?;
    if manifest.is_empty() {
        return Err(NothingToCommitError::new("layer would be empty"));
    }
    let layer = repo.create_layer(&graph::Manifest::from(&manifest))?;
    runtime.push_digest(&layer.digest()?)?;
    runtime.set_editable(false)?;
    remount_runtime(runtime)?;
    Ok(layer)
}

/// Commit the full layer stack and working files to a new platform.
pub fn commit_platform(runtime: &mut runtime::Runtime) -> Result<graph::Platform> {
    let config = get_config()?;
    let mut repo = config.get_repository()?;

    match commit_layer(runtime) {
        Ok(_) | Err(Error::NothingToCommit(_)) => (),
        Err(err) => return Err(err.into()),
    }

    let stack = runtime.get_stack();
    if stack.len() == 0 {
        Err(NothingToCommitError::new("platform would be empty"))
    } else {
        repo.create_platform(stack.clone())
    }
}
