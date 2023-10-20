// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

//! Functions related to the setup and teardown of the spfs runtime environment
//! and related system namespacing
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};

use super::runtime;
use crate::{which, Error, Result};

pub const SPFS_DIR: &str = "/spfs";

const NONE: Option<&str> = None;

/// Manages the configuration of an spfs runtime environment.
///
/// Specifically thing like, privilege escalation, mount namespace,
/// filesystem mounts, etc.
///
/// This type is explicitly not [`Send`] and not [`Sync`] because changes
/// being made often only affect the current thread (eg: unshare).
pub struct RuntimeConfigurator<User = NoUserChanges, MountNamespace = NoMountNamespace> {
    user: User,
    ns: MountNamespace,
}

/// Signifies that the effective user has not been modified
pub struct NoUserChanges;

/// Signifies that the mount namespace has not been modified
pub struct NoMountNamespace;

/// Signifies that the process has become root
pub struct IsRootUser {
    pub original_uid: nix::unistd::Uid,
    pub original_euid: nix::unistd::Uid,
}

/// Signifies that the process became root and has since dropped it
///
/// One dropping root, a process can never regain it.
pub struct IsNonRootUser;

/// Structure representing that the current thread has been moved into a new
/// mount namespace or was already in one and it has been validated.
///
/// Any function that expects an instance of this struct is intended to
/// perform all file I/O from inside the mount namespace. Not all threads of
/// the current process may be inside the mount namespace, so care must be
/// taken to avoid running tasks on threads that are not known to be in the
/// mount namespace. For example, using `tokio::spawn_blocking` or any file IO
/// functions from tokio may run on a thread from tokio's thread pool that is
/// not in the mount namespace.
///
/// This struct is `!Send` and `!Sync` to prevent it from being moved to or
/// referenced from a different thread where the mount namespace may be
/// different.
pub struct ThreadIsInMountNamespace {
    /// The path to the mount namespace this struct represents.
    pub mount_ns: std::path::PathBuf,
    _not_send: NotSendMarker,
    _not_sync: NotSyncMarker,
}

impl ThreadIsInMountNamespace {
    /// Create a new guard without moving into a new mount namespace.
    ///
    /// # Safety
    ///
    /// This reads the existing mount namespace of the calling thread and it
    /// is assumed the caller is already in a new mount namespace.
    pub unsafe fn existing() -> Result<Self> {
        Ok(ThreadIsInMountNamespace {
            mount_ns: std::fs::read_link(format!(
                "/proc/{}/task/{}/ns/mnt",
                std::process::id(),
                nix::unistd::gettid()
            ))
            .map_err(|err| Error::String(format!("Failed to read mount namespace: {err}")))?,
            _not_send: NotSendMarker(std::marker::PhantomData),
            _not_sync: NotSyncMarker(std::marker::PhantomData),
        })
    }
}

/// Structure representing that all threads of the process have been moved
/// into a new mount namespace or was already in one and it has been
/// validated.
///
/// Unlike [`ThreadIsInMountNamespace`], this struct is `Send` and `Sync` and
/// it is safe to use `tokio::spawn_blocking` and tokio file IO.
pub struct ProcessIsInMountNamespace {
    /// The path to the mount namespace this struct represents.
    pub mount_ns: std::path::PathBuf,
}

impl ProcessIsInMountNamespace {
    /// Create a new guard without moving into a new mount namespace.
    ///
    /// # Safety
    ///
    /// This reads the existing mount namespace of the calling thread and it
    /// is assumed the caller is already in a new mount namespace.
    pub unsafe fn existing() -> Result<Self> {
        Ok(ProcessIsInMountNamespace {
            mount_ns: std::fs::read_link("/proc/self/ns/mnt")
                .map_err(|err| Error::String(format!("Failed to read mount namespace: {err}")))?,
        })
    }
}

mod __private {
    use super::{ProcessIsInMountNamespace, ThreadIsInMountNamespace};

    /// Marker trait for [`ThreadIsInMountNamespace`] and [`ProcessIsInMountNamespace`]
    pub trait CurrentThreadIsInMountNamespace {
        /// The path to the mount namespace associated of the current thread
        fn mount_ns(&self) -> &std::path::Path;
    }
    impl CurrentThreadIsInMountNamespace for ThreadIsInMountNamespace {
        #[inline]
        fn mount_ns(&self) -> &std::path::Path {
            &self.mount_ns
        }
    }

    /// Marker trait for [`ProcessIsInMountNamespace`]
    pub trait CurrentProcessIsInMountNamespace: CurrentThreadIsInMountNamespace {}
    impl CurrentThreadIsInMountNamespace for ProcessIsInMountNamespace {
        #[inline]
        fn mount_ns(&self) -> &std::path::Path {
            &self.mount_ns
        }
    }
    impl CurrentProcessIsInMountNamespace for ProcessIsInMountNamespace {}
}

impl<User, MountNamespace> RuntimeConfigurator<User, MountNamespace> {
    fn new(user: User, ns: MountNamespace) -> Self {
        Self { user, ns }
    }
}

impl Default for RuntimeConfigurator<NoUserChanges, NoMountNamespace> {
    fn default() -> Self {
        Self::new(NoUserChanges, NoMountNamespace)
    }
}

impl<MountNamespace> RuntimeConfigurator<NoUserChanges, MountNamespace> {
    /// Escalate the current process' privileges, becoming root
    pub fn become_root(self) -> Result<RuntimeConfigurator<IsRootUser, MountNamespace>> {
        tracing::debug!("becoming root...");
        let original_euid = nix::unistd::geteuid();
        if let Err(err) = nix::unistd::seteuid(nix::unistd::Uid::from_raw(0)) {
            return Err(Error::wrap_nix(
                err,
                "Failed to become root user (effective)",
            ));
        }
        let original_uid = nix::unistd::getuid();
        if let Err(err) = nix::unistd::setuid(nix::unistd::Uid::from_raw(0)) {
            return Err(Error::wrap_nix(err, "Failed to become root user (actual)"));
        }
        Ok(RuntimeConfigurator::new(
            IsRootUser {
                original_euid,
                original_uid,
            },
            self.ns,
        ))
    }
}

impl<User> RuntimeConfigurator<User, NoMountNamespace> {
    /// Enter a new mount namespace and return a guard that represents the thread
    /// that is in the new namespace.
    pub fn enter_mount_namespace(
        self,
    ) -> Result<RuntimeConfigurator<User, ThreadIsInMountNamespace>> {
        tracing::debug!("entering mount namespace...");
        if let Err(err) = nix::sched::unshare(nix::sched::CloneFlags::CLONE_NEWNS) {
            return Err(Error::wrap_nix(err, "Failed to enter mount namespace"));
        }

        // Safety: we just moved the thread into a new mount namespace.
        let ns = unsafe { ThreadIsInMountNamespace::existing() }?;
        Ok(RuntimeConfigurator::new(self.user, ns))
    }

    /// Make this configurator for an existing runtime.
    ///
    /// The calling thread must already be operating in the provided runtime.
    pub fn current_runtime(
        self,
        rt: &runtime::Runtime,
    ) -> Result<RuntimeConfigurator<User, ProcessIsInMountNamespace>> {
        let Some(runtime_ns) = &rt.config.mount_namespace else {
            return Err(Error::NoActiveRuntime);
        };
        // Safety: we are going to validate that this is the
        // expected namespace for the provided runtime and so
        // is considered to be a valid spfs mount namespace
        let current_ns = unsafe { ProcessIsInMountNamespace::existing() }?;
        if runtime_ns != &current_ns.mount_ns {
            return Err(Error::String(format!(
                "Current runtime does not match expected: {runtime_ns:?} != {:?}",
                current_ns.mount_ns
            )));
        }

        std::env::set_var("SPFS_RUNTIME", rt.name());
        Ok(RuntimeConfigurator::new(self.user, current_ns))
    }

    /// Move this process into the namespace of an existing runtime
    ///
    /// This function will fail if called from a process with multiple threads.
    pub fn join_runtime(
        self,
        rt: &runtime::Runtime,
    ) -> Result<RuntimeConfigurator<User, ThreadIsInMountNamespace>> {
        check_can_join()?;

        let pid = match rt.status.owner {
            None => return Err(Error::RuntimeNotInitialized(rt.name().into())),
            Some(pid) => pid,
        };

        let ns_path = std::path::Path::new("/proc")
            .join(pid.to_string())
            .join("ns/mnt");

        tracing::debug!(?ns_path, "Getting process namespace");
        let file = match std::fs::File::open(&ns_path) {
            Ok(file) => file,
            Err(err) => {
                return match err.kind() {
                    std::io::ErrorKind::NotFound => Err(Error::UnknownRuntime {
                        runtime: rt.name().into(),
                        source: Box::new(err),
                    }),
                    _ => Err(Error::RuntimeReadError(ns_path, err)),
                }
            }
        };

        if let Err(err) = nix::sched::setns(file.as_raw_fd(), nix::sched::CloneFlags::empty()) {
            return Err(match err {
                nix::errno::Errno::EPERM => Error::new_errno(
                    libc::EPERM,
                    "spfs binary was not installed with required capabilities",
                ),
                _ => err.into(),
            });
        }

        std::env::set_var("SPFS_RUNTIME", rt.name());
        // Safety: we've just entered an existing mount namespace
        let ns = unsafe { ThreadIsInMountNamespace::existing() }?;
        Ok(RuntimeConfigurator::new(self.user, ns))
    }
}

/// Operations that do not need root but require the current thread to be in a
/// mount namespace.
impl<User, MountNamespace> RuntimeConfigurator<User, MountNamespace>
where
    MountNamespace: __private::CurrentThreadIsInMountNamespace,
{
    /// Return an error if the spfs filesystem is not mounted.
    pub async fn ensure_mounts_already_exist(&self) -> Result<()> {
        tracing::debug!("ensuring mounts already exist...");
        let res = self.is_mounted(SPFS_DIR).await;
        match res {
            Err(err) => Err(err.wrap("Failed to check for existing mount")),
            Ok(true) => Ok(()),
            Ok(false) => Err(format!("'{SPFS_DIR}' is not mounted, will not remount").into()),
        }
    }

    /// Check if the identified directory is an active mount point.
    async fn is_mounted<P: Into<PathBuf>>(&self, target: P) -> Result<bool> {
        let target = target.into();
        let parent = match target.parent() {
            None => return Ok(false),
            Some(p) => p.to_owned(),
        };

        // A new thread created while holding _guard will be inside the same
        // mount namespace...
        let stat_parent_thread =
            std::thread::spawn(move || nix::sys::stat::stat(&parent).map_err(Error::from));

        let stat_target_thread =
            std::thread::spawn(move || nix::sys::stat::stat(&target).map_err(Error::from));

        let (st_parent, st_target) = tokio::task::spawn_blocking(move || {
            let st_parent = stat_parent_thread.join();
            let st_target = stat_target_thread.join();
            (st_parent, st_target)
        })
        .await?;

        let st_parent =
            st_parent.map_err(|_| Error::String("Failed to stat parent".to_owned()))??;
        let st_target =
            st_target.map_err(|_| Error::String("Failed to stat target".to_owned()))??;

        Ok(st_target.st_dev != st_parent.st_dev)
    }

    /// The path to the mount namespace associated of the current thread
    #[inline]
    pub fn mount_namespace(&self) -> &std::path::Path {
        self.ns.mount_ns()
    }
}

/// Operations that need root and require the current thread to be in a mount
/// namespace.
impl<MountNamespace> RuntimeConfigurator<IsRootUser, MountNamespace>
where
    MountNamespace: __private::CurrentThreadIsInMountNamespace,
{
    /// Privatize mounts in the current namespace, so that new mounts and changes
    /// to existing mounts don't propagate to the parent namespace.
    pub async fn privatize_existing_mounts(&self) -> Result<()> {
        use nix::mount::{mount, MsFlags};

        tracing::debug!("privatizing existing mounts...");

        let mut res = mount(NONE, "/", NONE, MsFlags::MS_PRIVATE, NONE);
        if let Err(err) = res {
            return Err(Error::wrap_nix(
                err,
                "Failed to privatize existing mount: /",
            ));
        }

        if self.is_mounted("/tmp").await? {
            res = mount(NONE, "/tmp", NONE, MsFlags::MS_PRIVATE, NONE);
            if let Err(err) = res {
                return Err(Error::wrap_nix(
                    err,
                    "Failed to privatize existing mount: /tmp",
                ));
            }
        }
        Ok(())
    }

    /// Check or create the necessary directories for mounting the provided runtime
    pub fn ensure_mount_targets_exist(&self, config: &runtime::Config) -> Result<()> {
        tracing::debug!("ensuring mount targets exist...");
        runtime::makedirs_with_perms(SPFS_DIR, 0o777)
            .map_err(|err| err.wrap(format!("Failed to create {SPFS_DIR}")))?;

        if let Some(dir) = &config.runtime_dir {
            runtime::makedirs_with_perms(dir, 0o777)
                .map_err(|err| err.wrap(format!("Failed to create {dir:?}")))?
        }
        Ok(())
    }

    pub fn mount_runtime(&self, config: &runtime::Config) -> Result<()> {
        use nix::mount::{mount, MsFlags};

        let dir = match &config.runtime_dir {
            Some(ref p) => p,
            None => return Ok(()),
        };

        let tmpfs_opts = config
            .tmpfs_size
            .as_ref()
            .map(|size| format!("size={size}"));

        tracing::debug!("mounting runtime...");
        let res = mount(
            NONE,
            dir,
            Some("tmpfs"),
            MsFlags::MS_NOEXEC,
            tmpfs_opts.as_deref(),
        );
        if let Err(err) = res {
            Err(Error::wrap_nix(err, format!("Failed to mount {dir:?}")))
        } else {
            Ok(())
        }
    }

    pub fn unmount_runtime(&self, config: &runtime::Config) -> Result<()> {
        let dir = match &config.runtime_dir {
            Some(ref p) => p,
            None => return Ok(()),
        };

        tracing::debug!("unmounting existing runtime...");
        let result = nix::mount::umount2(dir, nix::mount::MntFlags::MNT_DETACH);
        if let Err(err) = result {
            return Err(Error::wrap_nix(err, format!("Failed to unmount {dir:?}")));
        }
        Ok(())
    }

    pub async fn setup_runtime(&self, rt: &runtime::Runtime) -> Result<()> {
        tracing::debug!("setting up runtime...");
        rt.ensure_required_directories().await
    }

    pub(crate) async fn mount_env_overlayfs<P: AsRef<Path>>(
        &self,
        rt: &runtime::Runtime,
        lowerdirs: impl IntoIterator<Item = P>,
    ) -> Result<()> {
        tracing::debug!("mounting the overlay filesystem...");
        let overlay_args = get_overlay_args(rt, lowerdirs)?;
        let mount = super::resolve::which("mount").unwrap_or_else(|| "/usr/bin/mount".into());
        tracing::debug!("{mount:?} -t overlay -o {overlay_args} none {SPFS_DIR}",);
        // for some reason, the overlay mount process creates a bad filesystem if the
        // mount command is called directly from this process. It may be some default
        // option or minor detail in how the standard mount command works - possibly related
        // to this process eventually dropping privileges, but that is uncertain right now
        let mut cmd = tokio::process::Command::new(mount);
        cmd.args(["-t", "overlay"]);
        cmd.arg("-o");
        cmd.arg(overlay_args);
        cmd.arg("none");
        cmd.arg(SPFS_DIR);
        match cmd.status().await {
            Err(err) => Err(Error::process_spawn_error("mount", err, None)),
            Ok(status) => match status.code() {
                Some(0) => Ok(()),
                _ => Err("Failed to mount overlayfs".into()),
            },
        }
    }

    #[cfg(feature = "fuse-backend")]
    pub(crate) async fn mount_fuse_lower_dir(&self, rt: &runtime::Runtime) -> Result<()> {
        self.mount_fuse_onto(rt, &rt.config.lower_dir).await
    }

    #[cfg(feature = "fuse-backend")]
    pub(crate) async fn mount_env_fuse(&self, rt: &runtime::Runtime) -> Result<()> {
        self.mount_fuse_onto(rt, SPFS_DIR).await
    }

    #[cfg(feature = "fuse-backend")]
    async fn mount_fuse_onto<P>(&self, rt: &runtime::Runtime, path: P) -> Result<()>
    where
        P: AsRef<std::ffi::OsStr>,
    {
        use spfs_encoding::Encodable;

        let path = path.as_ref().to_owned();
        let platform = rt.to_platform().digest()?.to_string();
        let opts = get_fuse_args(&rt.config, &self.user, true);

        // A new thread created in mount namespace will be inside the same
        // mount namespace...
        let mount_and_wait_thread = std::thread::spawn(move || {
            tracing::debug!("mounting the FUSE filesystem...");
            let spfs_fuse = match super::resolve::which_spfs("fuse") {
                None => return Err(Error::MissingBinary("spfs-fuse")),
                Some(exe) => exe,
            };
            let mut cmd = std::process::Command::new(spfs_fuse);
            cmd.arg("-o");
            cmd.arg(opts);
            // We are trusting that the runtime has been saved to the repository
            // and so the platform that the runtime relies on has also been tagged
            cmd.arg(platform);
            cmd.arg(&path);
            // The command logs all output to stderr, and should never hold onto
            // a handle to this process' stdout as it can cause hanging
            cmd.stdout(std::process::Stdio::null());
            // Allowing stderr to be inherited causes this process to hang
            // forever reading from that pipe, even after the child processes
            // has exited (cause unknown).
            // TODO: find a way to still see stderr output from the child
            // process without it hanging.
            cmd.stderr(std::process::Stdio::null());
            tracing::debug!("{cmd:?}");
            match cmd.status() {
                Err(err) => return Err(Error::process_spawn_error("mount", err, None)),
                Ok(status) if status.code() == Some(0) => {}
                Ok(status) => {
                    return Err(Error::String(format!(
                    "Failed to mount fuse filesystem, mount command exited with non-zero status {:?}",
                    status.code()
                )))
                }
            };

            // the fuse filesystem may take some moments to be fully initialized, and we
            // don't want to return until this is true. Otherwise, subsequent operations may
            // see unexpected errors.
            let mut sleep_time_ms = vec![2, 5, 10, 50, 100, 100, 100, 100];
            while let Err(err) = std::fs::symlink_metadata(&path) {
                if let Some(ms) = sleep_time_ms.pop() {
                    std::thread::sleep(std::time::Duration::from_millis(ms));
                } else {
                    tracing::warn!("FUSE did not appear to start after delay: {err}");
                    break;
                }
            }
            Ok(())
        });

        tokio::task::spawn_blocking(move || mount_and_wait_thread.join())
            .await?
            .map_err(|_| Error::String("Failed to mount and wait for fuse".to_owned()))??;

        Ok(())
    }

    pub async fn mask_files(
        &self,
        config: &runtime::Config,
        manifest: super::tracking::Manifest,
    ) -> Result<()> {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        tracing::debug!("masking deleted files...");

        let owner = self.user.original_uid;

        let prefix = config
            .upper_dir
            .to_str()
            .ok_or_else(|| {
                crate::Error::String(format!(
                    "configured runtime upper_dir has invalid characters: {:?}",
                    config.upper_dir
                ))
            })?
            .to_owned();

        // A new thread created in mount namespace will be inside the same
        // mount namespace...
        let mask_files_thread = std::thread::spawn(move || {
            let nodes: Vec<_> = manifest.walk_abs(prefix).collect();
            for node in nodes.iter() {
                if !node.entry.kind.is_mask() {
                    continue;
                }
                let relative_fullpath = node.path.to_path("");
                if let Some(parent) = relative_fullpath.parent() {
                    tracing::trace!(?parent, "build parent dir for mask");
                    runtime::makedirs_with_perms(parent, 0o777)?;
                }
                tracing::trace!(?node.path, "Creating file mask");

                let fullpath = node.path.to_path("/");
                let existing = std::fs::symlink_metadata(&fullpath).ok();
                if let Some(meta) = existing {
                    if runtime::is_removed_entry(&meta) {
                        continue;
                    }
                    if meta.is_file() {
                        std::fs::remove_file(&fullpath)
                            .map_err(|err| Error::RuntimeWriteError(fullpath.clone(), err))?;
                    } else {
                        std::fs::remove_dir_all(&fullpath)
                            .map_err(|err| Error::RuntimeWriteError(fullpath.clone(), err))?;
                    }
                }

                nix::sys::stat::mknod(
                    &fullpath,
                    nix::sys::stat::SFlag::S_IFCHR,
                    nix::sys::stat::Mode::empty(),
                    0,
                )
                .map_err(move |err| {
                    Error::wrap_nix(err, format!("Failed to create file mask: {}", node.path))
                })?;
            }

            for node in nodes.iter().rev() {
                if !node.entry.kind.is_tree() {
                    continue;
                }
                let fullpath = node.path.to_path("/");
                if !fullpath.is_dir() {
                    continue;
                }
                let existing = std::fs::symlink_metadata(&fullpath)
                    .map_err(|err| Error::RuntimeReadError(fullpath.clone(), err))?;
                if existing.permissions().mode() != node.entry.mode {
                    if let Err(err) = std::fs::set_permissions(
                        &fullpath,
                        std::fs::Permissions::from_mode(node.entry.mode),
                    ) {
                        match err.kind() {
                            std::io::ErrorKind::NotFound => continue,
                            _ => {
                                return Err(Error::RuntimeSetPermissionsError(fullpath, err));
                            }
                        }
                    }
                }
                if existing.uid() != owner.as_raw() {
                    let res = nix::unistd::chown(&fullpath, Some(owner), None);
                    match res {
                        Ok(_) | Err(nix::errno::Errno::ENOENT) => continue,
                        Err(err) => {
                            return Err(Error::wrap_nix(
                                err,
                                format!("Failed to set ownership on masked file [{}]", node.path),
                            ));
                        }
                    }
                }
            }
            Ok(())
        });

        tokio::task::spawn_blocking(move || mask_files_thread.join())
            .await?
            .map_err(|_| Error::String("Failed to mask files".to_owned()))??;

        Ok(())
    }
}

/// Operations that need root and require the whole process to be in a mount
/// namespace.
impl<MountNamespace> RuntimeConfigurator<IsRootUser, MountNamespace>
where
    MountNamespace: __private::CurrentProcessIsInMountNamespace,
{
    /// Make a durable upper dir path for the runtime, copy the
    /// contents of its previous upper dir to the new one.
    pub async fn change_runtime_to_durable(&self, runtime: &mut runtime::Runtime) -> Result<i32> {
        // Not all runtime backends support durable runtimes
        match runtime.config.mount_backend {
            runtime::MountBackend::FuseOnly | runtime::MountBackend::WinFsp => {
                // a vfs-only runtime cannot be change to durable
                return Err(Error::RuntimeChangeToDurableError(format!(
                    "{} backend does not support durable runtimes",
                    runtime.config.mount_backend
                )));
            }
            runtime::MountBackend::OverlayFsWithFuse
            | runtime::MountBackend::OverlayFsWithRenders => {}
        }

        tracing::info!("changing runtime to durable");

        let old_upper_dir = runtime.data().config.upper_dir.clone();
        tracing::debug!("old upper dir: {}", old_upper_dir.display());

        let new_path = runtime.setup_durable_upper_dir().await?;
        tracing::debug!("new upper path: {}", new_path.display());
        runtime.ensure_upper_dirs().await?;
        tracing::debug!("ensured upper dirs");

        // this only syncs over the upper_dir contents, not the
        // work_dir because the work_dir is updated and managed
        // internally by overlayfs as changes are made. any edits or
        // changes that have been completed, but not committed, will
        // appear in the upper_dir.
        let src_dir = match old_upper_dir.to_str() {
            Some(path) => path,
            None => {
                return Err(Error::RuntimeChangeToDurableError(format!(
                    "current upper_dir '{}' has invalid characters",
                    old_upper_dir.display()
                )))
            }
        };
        let dest_dir = match new_path.to_str() {
            Some(path) => path,
            None => {
                return Err(Error::RuntimeChangeToDurableError(format!(
                    "new upper_dir '{}' has invalid characters",
                    new_path.display()
                )))
            }
        };

        let args = vec!["-aD", src_dir, dest_dir];
        let cmd_path = match which("rsync") {
            Some(cmd) => cmd,
            None => {
                return Err(Error::RuntimeChangeToDurableError(
                    "rysnc is not available on this host".to_string(),
                ))
            }
        };

        let mut rsync = std::process::Command::new(cmd_path);
        rsync.args(args);
        tracing::debug!("the rsync command: {rsync:?}");

        match rsync.status().map_err(|err| Error::String(err.to_string())) {
            Ok(status) => match status.code() {
                Some(0) => {
                    runtime.set_durable(true);
                    runtime.save_state_to_storage().await?;
                    tracing::info!("runtime saved as durable");
                    Ok(0)
                }
                Some(code) => Err(Error::RuntimeChangeToDurableError(format!(
                    "rsync failed with exit code: {code}"
                ))),
                None => Err(Error::RuntimeChangeToDurableError(
                    "rsync was terminated by an unexpected signal".to_string(),
                )),
            },
            Err(err) => Err(Error::RuntimeChangeToDurableError(format!(
                "rsync failed to run: {err}"
            ))),
        }
    }

    /// Unmount the non-fuse portion of the provided runtime, if applicable.
    pub async fn unmount_env(&self, rt: &runtime::Runtime, lazy: bool) -> Result<()> {
        tracing::debug!("unmounting existing env...");

        // unmount fuse portion first, because once /spfs is unmounted many safety checks will
        // fail and the runtime will effectively not be re-configurable anymore.
        self.unmount_env_fuse(rt, lazy).await?;
        self.unmount_env_overlayfs(rt, lazy).await?;
        Ok(())
    }

    /// Unmount the overlayfs portion of the provided runtime, if applicable
    pub async fn unmount_env_overlayfs(&self, rt: &runtime::Runtime, lazy: bool) -> Result<()> {
        match rt.config.mount_backend {
            runtime::MountBackend::FuseOnly | runtime::MountBackend::WinFsp => {
                // a vfs-only runtime cannot be unmounted this way
                // and should already be handled by a previous call to
                // unmount_env_fuse
                return Ok(());
            }
            runtime::MountBackend::OverlayFsWithFuse
            | runtime::MountBackend::OverlayFsWithRenders => {}
        }

        let mut flags = nix::mount::MntFlags::empty();
        if lazy {
            // Perform a lazy unmount in case there are still open handles to files.
            // This way we can mount over the old one without worrying about busyness
            flags |= nix::mount::MntFlags::MNT_DETACH;
        }
        let result = nix::mount::umount2(SPFS_DIR, flags);
        if let Err(err) = result {
            return Err(Error::wrap_nix(
                err,
                format!("Failed to unmount {SPFS_DIR}"),
            ));
        }
        Ok(())
    }

    /// Unmount the fuse portion of the provided runtime, if applicable.
    async fn unmount_env_fuse(&self, rt: &runtime::Runtime, lazy: bool) -> Result<()> {
        let mount_path = match rt.config.mount_backend {
            runtime::MountBackend::OverlayFsWithFuse => rt.config.lower_dir.as_path(),
            runtime::MountBackend::FuseOnly => std::path::Path::new(SPFS_DIR),
            runtime::MountBackend::OverlayFsWithRenders | runtime::MountBackend::WinFsp => {
                return Ok(())
            }
        };
        tracing::debug!(%lazy, "unmounting existing fuse env @ {mount_path:?}...");

        // The FUSE filesystem can take some time to start up, and
        // if the runtime tries to exit too quickly, the fusermount
        // command can return with errors because the filesystem has
        // not yet initialized and the connection is not ready.
        //
        // A few retries in these cases gives time for the filesystem
        // to enter a ready and connected state.
        let mut retry_after_ms = vec![10, 50, 100, 200, 500, 1000];
        while self.is_mounted(mount_path).await.unwrap_or(true) {
            let flags = if lazy { "-uz" } else { "-u" };
            let child = tokio::process::Command::new("fusermount")
                .arg(flags)
                .arg(mount_path)
                .stderr(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stdin(std::process::Stdio::piped())
                .spawn()
                .map_err(|err| Error::ProcessSpawnError("fusermount".into(), err))?;
            match child.wait_with_output().await {
                Err(err) => {
                    return Err(Error::String(format!(
                        "Failed to unmount FUSE filesystem: {err:?}"
                    )))
                }
                Ok(out) if out.status.code() == Some(0) => continue,
                Ok(out) => {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    match retry_after_ms.pop() {
                        Some(wait_ms) => {
                            tracing::trace!(
                                "Retrying FUSE unmount which failed with, {:?}: {}",
                                out.status.code(),
                                stderr.trim()
                            );
                            tokio::time::sleep(std::time::Duration::from_millis(wait_ms)).await;
                            continue;
                        }
                        None => {
                            return Err(Error::String(format!(
                                "FUSE unmount returned non-zero exit status, {:?}: {}",
                                out.status.code(),
                                stderr.trim()
                            )))
                        }
                    }
                }
            };
        }
        Ok(())
    }
}

/// Operations that need root but have no mount namespace requirements.
impl<MountNamespace> RuntimeConfigurator<IsRootUser, MountNamespace> {
    /// Drop all capabilities and become the original user that
    /// this thread was running as before becoming root
    pub fn become_original_user(
        self,
    ) -> Result<RuntimeConfigurator<IsNonRootUser, MountNamespace>> {
        tracing::debug!("dropping root...");
        let mut result = nix::unistd::setuid(self.user.original_uid);
        if let Err(err) = result {
            return Err(Error::wrap_nix(
                err,
                "Failed to become regular user (actual)",
            ));
        }
        result = nix::unistd::seteuid(self.user.original_euid);
        if let Err(err) = result {
            return Err(Error::wrap_nix(
                err,
                "Failed to become regular user (effective)",
            ));
        }
        self.drop_all_capabilities()?;
        Ok(RuntimeConfigurator::new(IsNonRootUser, self.ns))
    }

    // Drop all of the capabilities held by the current thread
    fn drop_all_capabilities(&self) -> Result<()> {
        tracing::debug!("drop all capabilities/privileges...");
        caps::clear(None, caps::CapSet::Effective)?;
        caps::clear(None, caps::CapSet::Permitted)?;
        caps::clear(None, caps::CapSet::Inheritable)?;

        // the dumpable attribute can become unset when changing pids or
        // calling a binary with capabilities (spfs). Resetting this to one
        // restores ownership of the proc filesystem to the calling user which
        // is important in being able to read and join an existing runtime's namespace
        let result = unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 1) };
        if result != 0 {
            Err(nix::errno::Errno::last().into())
        } else {
            Ok(())
        }
    }
}

// Checks if the current process will be able to join an existing runtime
fn check_can_join() -> Result<()> {
    match procfs::process::Process::myself()
        .map_err(|err| Error::String(err.to_string()))?
        .stat()
        .map_err(|err| Error::String(err.to_string()))?
        .num_threads
    {
        count @ 2.. => {
            return Err(format!(
                "Program must be single-threaded to join an existing runtime (has {count} threads)"
            )
            .into());
        }
        1 => (),
        i => {
            return Err(format!("Unexpected negative thread count: {i}").into());
        }
    }

    if !have_required_join_capabilities()? {
        return Err("Missing required capabilities to join an existing runtime".into());
    }
    Ok(())
}

// Checks if the current process has the capabilities required
// to join an existing runtime
fn have_required_join_capabilities() -> Result<bool> {
    let effective = caps::read(None, caps::CapSet::Effective)?;
    Ok(effective.contains(&caps::Capability::CAP_SYS_ADMIN)
        && effective.contains(&caps::Capability::CAP_SYS_CHROOT))
}

const OVERLAY_ARGS_RO_PREFIX: &str = "ro";
const OVERLAY_ARGS_INDEX: &str = "index";
const OVERLAY_ARGS_INDEX_ON: &str = "index=on";
const OVERLAY_ARGS_METACOPY: &str = "metacopy";
const OVERLAY_ARGS_METACOPY_ON: &str = "metacopy=on";

/// A struct for holding the options that will be included
/// in the overlayfs mount command when mounting an environment.
#[derive(Default)]
pub(crate) struct OverlayMountOptions {
    /// Specifies that the overlay file system is mounted as read-only
    pub read_only: bool,
    /// When true, inodes are indexed in the mount so that
    /// files which share the same inode (hardlinks) are broken
    /// in the final mount and changes to one file don't affect
    /// the other.
    ///
    /// This is the desired default behavior for
    /// spfs, since we rely on hardlinks for deduplication but
    /// expect that file to be able to appear in mutliple places
    /// as separate files that just so happen to share the same content.
    ///
    /// When disabled, there will be additional restrictions on
    /// remounting the environment since the filesystem will hold
    /// additional handles and may not unmount while files remain held
    ///
    /// It needs to be disabled for durable runtimes becaues the
    /// overlayfs index option it enables prevents sharing across
    /// subsequent invocations of durable runtimes.
    /// https://www.kernel.org/doc/html/latest/filesystems/overlayfs.html#sharing-and-copying-layers
    break_hardlinks: bool,
    /// When true, overlayfs will use extended file attributes to avoid
    /// copying file data when only the metadata of a file has changed.
    /// https://www.kernel.org/doc/html/latest/filesystems/overlayfs.html#metadata-only-copy-up
    metadata_copy_up: bool,
}

impl OverlayMountOptions {
    /// Create the mount options for a runtime state
    fn new(rt: &runtime::Runtime) -> Self {
        Self {
            read_only: !rt.status.editable,
            break_hardlinks: true,
            metadata_copy_up: true,
        }
    }

    /// Return the options that should be included in the mount request.
    pub fn to_options(&self) -> Vec<&'static str> {
        let params = runtime::overlayfs::overlayfs_available_options();
        let mut opts = Vec::new();
        if self.read_only {
            opts.push(OVERLAY_ARGS_RO_PREFIX);
        }
        if !self.break_hardlinks && params.contains(OVERLAY_ARGS_INDEX) {
            opts.push(OVERLAY_ARGS_INDEX_ON);
        }
        if self.metadata_copy_up && params.contains(OVERLAY_ARGS_METACOPY) {
            opts.push(OVERLAY_ARGS_METACOPY_ON);
        }
        opts
    }
}

/// Get the overlayfs arguments for the given list of lower layer directories.
///
/// This returns an error if the arguments would exceed the legal size limit
/// (if known).
///
/// `prefix` is prepended to the generated overlay args.
pub(crate) fn get_overlay_args<P: AsRef<Path>>(
    rt: &runtime::Runtime,
    lowerdirs: impl IntoIterator<Item = P>,
) -> Result<String> {
    // Allocate a large buffer up front to avoid resizing/copying.
    let mut args = String::with_capacity(4096);

    let mount_options = OverlayMountOptions::new(rt);
    for option in mount_options.to_options() {
        args.push_str(option);
        args.push(',');
    }

    args.push_str("lowerdir=");
    args.push_str(&rt.config.lower_dir.to_string_lossy());
    for path in lowerdirs.into_iter() {
        args.push(':');
        args.push_str(&path.as_ref().to_string_lossy());
    }

    args.push_str(",upperdir=");
    args.push_str(&rt.config.upper_dir.to_string_lossy());

    args.push_str(",workdir=");
    args.push_str(&rt.config.work_dir.to_string_lossy());

    match nix::unistd::sysconf(nix::unistd::SysconfVar::PAGE_SIZE) {
        Err(_) => tracing::debug!("failed to get page size for checking arg length"),
        Ok(None) => (),
        Ok(Some(size)) => {
            if args.as_bytes().len() as i64 > size - 1 {
                return Err(
                    "Mount args would be too large for the kernel; reduce the number of layers"
                        .into(),
                );
            }
        }
    };
    Ok(args)
}

#[cfg(feature = "fuse-backend")]
fn get_fuse_args(config: &runtime::Config, owner: &IsRootUser, read_only: bool) -> String {
    use fuser::MountOption::*;
    use itertools::Itertools;

    let mut opts = vec![
        NoDev,
        NoAtime,
        NoSuid,
        Exec,
        AutoUnmount,
        AllowOther,
        CUSTOM(format!("uid={}", owner.original_uid)),
        CUSTOM(format!("gid={}", nix::unistd::getgid())),
    ];
    opts.push(if read_only { RO } else { RW });
    opts.extend(
        config
            .secondary_repositories
            .iter()
            .map(|r| CUSTOM(format!("remote={r}"))),
    );
    opts.iter().map(option_to_string).join(",")
}

// Format option to be passed to libfuse or kernel. A copy of
// [`fuser::mnt::mount_option::option_to_string`], but it is private
#[cfg(feature = "fuse-backend")]
pub fn option_to_string(option: &fuser::MountOption) -> String {
    use fuser::MountOption;
    match option {
        MountOption::FSName(name) => format!("fsname={}", name),
        MountOption::Subtype(subtype) => format!("subtype={}", subtype),
        MountOption::CUSTOM(value) => value.to_string(),
        MountOption::AutoUnmount => "auto_unmount".to_string(),
        MountOption::AllowOther => "allow_other".to_string(),
        // AllowRoot is implemented by allowing everyone access and then restricting to
        // root + owner within fuser
        MountOption::AllowRoot => "allow_other".to_string(),
        MountOption::DefaultPermissions => "default_permissions".to_string(),
        MountOption::Dev => "dev".to_string(),
        MountOption::NoDev => "nodev".to_string(),
        MountOption::Suid => "suid".to_string(),
        MountOption::NoSuid => "nosuid".to_string(),
        MountOption::RO => "ro".to_string(),
        MountOption::RW => "rw".to_string(),
        MountOption::Exec => "exec".to_string(),
        MountOption::NoExec => "noexec".to_string(),
        MountOption::Atime => "atime".to_string(),
        MountOption::NoAtime => "noatime".to_string(),
        MountOption::DirSync => "dirsync".to_string(),
        MountOption::Sync => "sync".to_string(),
        MountOption::Async => "async".to_string(),
    }
}

/// Prevent a structure from being [`Send`].
struct NotSendMarker(std::marker::PhantomData<*mut u8>);

/// Prevent a structure from being [`Sync`].
struct NotSyncMarker(std::marker::PhantomData<std::cell::Cell<u8>>);
