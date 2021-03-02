use super::config::load_config;
use super::resolve::{resolve_overlay_dirs, resolve_stack_to_layers, which};
use crate::{prelude::*, runtime, tracking, Error, Result};
use std::ffi::OsString;

#[cfg(test)]
#[path = "./status_test.rs"]
mod status_test;

static SPFS_RUNTIME: &str = "SPFS_RUNTIME";

#[derive(Debug)]
pub struct NoRuntimeError {
    pub message: String,
}

impl NoRuntimeError {
    pub fn new<S: AsRef<str>>(details: Option<S>) -> Error {
        let mut msg = "No active runtime".to_string();
        if let Some(details) = details {
            msg = format!("{}: {}", msg, details.as_ref());
        }
        Error::NoRuntime(Self { message: msg })
    }
}

/// Unlock the current runtime file system so that it can be modified.
///
/// Once modified, active changes can be committed
///
/// Errors:
/// - [`NoRuntimeError`]: if there is no active runtime
/// - if the active runtime is already editable
pub fn make_active_runtime_editable() -> Result<()> {
    let mut rt = active_runtime()?;
    if rt.is_editable() {
        return Err("Active runtime is already editable".into());
    }

    rt.set_editable(true)?;
    match remount_runtime(&rt) {
        Err(err) => {
            rt.set_editable(false)?;
            Err(err)
        }
        Ok(_) => Ok(()),
    }
}

/// Remount the given runtime as configured.
pub fn remount_runtime(rt: &runtime::Runtime) -> Result<()> {
    let (cmd, args) = build_spfs_remount_command(rt)?;
    let mut cmd = std::process::Command::new(cmd);
    cmd.args(&args);
    let res = cmd.status()?;
    if res.code() != Some(0) {
        Err("Failed to re-mount runtime filesystem".into())
    } else {
        Ok(())
    }
}

/// Calculate the file manifest for the layers in the given runtime.
///
/// The returned manifest DOES NOT include any active changes to the runtime.
pub fn compute_runtime_manifest(rt: &runtime::Runtime) -> Result<tracking::Manifest> {
    let config = load_config()?;
    let repo = config.get_repository()?;

    let stack = rt.get_stack();
    let layers = resolve_stack_to_layers(stack.into_iter(), None)?;
    let mut manifest = tracking::Manifest::default();
    for layer in layers.iter().rev() {
        manifest.update(&repo.read_manifest(&layer.manifest)?.unlock())
    }
    Ok(manifest)
}

/// Return the active runtime, or raise a NoRuntimeError.
pub fn active_runtime() -> Result<runtime::Runtime> {
    let path =
        std::env::var(SPFS_RUNTIME).map_err(|_| NoRuntimeError::new(Option::<&str>::None))?;
    runtime::Runtime::new(path)
}

/// Initialize the current spfs runtime.
pub fn initialize_runtime() -> Result<runtime::Runtime> {
    active_runtime()
}

pub fn deinitialize_runtime() -> Result<()> {
    let rt = active_runtime()?;
    rt.delete()?;
    std::env::remove_var(SPFS_RUNTIME);
    Ok(())
}

fn build_spfs_remount_command(rt: &runtime::Runtime) -> Result<(OsString, Vec<OsString>)> {
    let exe = match which("spfs-enter") {
        Some(exe) => exe,
        None => return Err("'spfs-enter' not found in PATH".into()),
    };

    let mut args = vec![OsString::from("-r")];

    let overlay_dirs = resolve_overlay_dirs(rt)?;
    for dirpath in overlay_dirs {
        args.push("-d".into());
        args.push(dirpath.into());
    }

    if rt.is_editable() {
        args.push("-e".into())
    }

    Ok((exe.into(), args))
}
