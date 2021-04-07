use super::config::{load_config, Config};
use super::resolve::{resolve_overlay_dirs, resolve_stack_to_layers};
use crate::{bootstrap, env, prelude::*, runtime, tracking, Error, Result};

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
    let (cmd, args) = bootstrap::build_spfs_remount_command(rt)?;
    let mut cmd = std::process::Command::new(cmd);
    cmd.args(&args);
    tracing::debug!("{:?}", cmd);
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
    let name =
        std::env::var(SPFS_RUNTIME).map_err(|_| NoRuntimeError::new(Option::<&str>::None))?;
    let config = load_config()?;
    let storage = config.get_runtime_storage()?;
    storage.read_runtime(name)
}

/// Reinitialize the current spfs runtime as rt (in case of runtime config changes).
pub fn reinitialize_runtime(rt: &runtime::Runtime, config: &Config) -> Result<()> {
    let dirs = resolve_overlay_dirs(&rt)?;
    tracing::debug!("computing runtime manifest");
    let manifest = compute_runtime_manifest(&rt)?;

    let tmpfs_opts = config
        .filesystem
        .tmpfs_size
        .as_ref()
        .map(|size| format!("size={}", size));

    let original = env::become_root()?;
    env::ensure_mounts_already_exist()?;
    env::unmount_env()?;
    env::unmount_runtime()?;
    env::mount_runtime(tmpfs_opts.as_ref().map(|s| s.as_str()))?;
    env::setup_runtime()?;
    env::unlock_runtime(tmpfs_opts.as_ref().map(|s| s.as_str()))?;
    env::mount_env(&dirs)?;
    env::mask_files(&manifest)?;
    env::set_runtime_lock(rt.is_editable(), None)?;
    env::become_original_user(original)?;
    env::drop_all_capabilities()?;
    Ok(())
}

/// Initialize the current runtime as rt.
pub fn initialize_runtime(rt: &runtime::Runtime, config: &Config) -> Result<()> {
    let dirs = resolve_overlay_dirs(&rt)?;
    tracing::debug!("computing runtime manifest");
    let manifest = compute_runtime_manifest(&rt)?;

    let tmpfs_opts = config
        .filesystem
        .tmpfs_size
        .as_ref()
        .map(|size| format!("size={}", size));

    env::enter_mount_namespace()?;
    let original = env::become_root()?;
    env::privatize_existing_mounts()?;
    env::ensure_mount_targets_exist()?;
    env::mount_runtime(tmpfs_opts.as_ref().map(|s| s.as_str()))?;
    env::setup_runtime()?;
    env::mount_env(&dirs)?;
    env::mask_files(&manifest)?;
    env::set_runtime_lock(rt.is_editable(), None)?;
    env::become_original_user(original)?;
    env::drop_all_capabilities()?;
    Ok(())
}
