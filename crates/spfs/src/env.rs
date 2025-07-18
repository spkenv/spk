// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! Functions related to the setup and teardown of the spfs runtime environment
//! and related system namespacing

use std::ffi::{CStr, CString};
use std::path::{Path, PathBuf};

use linux_raw_sys::general::{fsconfig_command, mount_attr};
use linux_syscall::{
    SYS_fsconfig,
    SYS_fsmount,
    SYS_fsopen,
    SYS_mount_setattr,
    SYS_move_mount,
    SYS_open_tree,
    syscall,
};

use super::runtime;
use crate::{Error, Result, which};

pub const SPFS_DIR: &str = "/spfs";
pub const SPFS_DIR_PREFIX: &str = "/spfs/";
const SPFS_DIR_CSTR: &CStr = c"/spfs";

const EMPTY_CSTR: &CStr = c"";
const FSCONFIG_INDEX_CSTR: &CStr = c"index";
const FSCONFIG_LOWERDIR_CSTR: &CStr = c"lowerdir";
const FSCONFIG_LOWERDIR_APPEND_CSTR: &CStr = c"lowerdir+";
const FSCONFIG_METACOPY_CSTR: &CStr = c"metacopy";
const FSCONFIG_NONE_CSTR: &CStr = c"none";
const FSCONFIG_ON_CSTR: &CStr = c"on";
const FSCONFIG_RO_CSTR: &CStr = c"ro";
const FSCONFIG_SOURCE_CSTR: &CStr = c"source";
const FSCONFIG_UPPERDIR_CSTR: &CStr = c"upperdir";
const FSCONFIG_WORKDIR_CSTR: &CStr = c"workdir";
const FSOPEN_OVERLAY_CSTR: &CStr = c"overlay";

const NONE: Option<&str> = None;

// Linux syscall constants from /usr/include/linux/mount.h.
const FSCONFIG_SET_STRING: u32 = fsconfig_command::FSCONFIG_SET_STRING as u32;
const FSCONFIG_SET_FLAG: u32 = fsconfig_command::FSCONFIG_SET_FLAG as u32;
const FSCONFIG_CMD_CREATE: u32 = fsconfig_command::FSCONFIG_CMD_CREATE as u32;
const FSCONFIG_DEFAULT: u32 = 0x00;
const FSOPEN_CLOEXEC: u32 = 0x01;
const FSMOUNT_CLOEXEC: u32 = 0x01;
const MOUNT_ATTR_RDONLY: u32 = 0x01;
const MOUNT_ATTR_SIZE_VER0: u32 = 32;
const MOVE_MOUNT_F_EMPTY_PATH: u32 = 0x04;
const OPEN_TREE_CLOEXEC: u32 = 0x80000;
const OPEN_TREE_CLONE: u32 = 0x01;

// Linux fcntl constants from /usr/include/fcntl.h.
const AT_EMPTY_PATH: u32 = 0x1000;
const AT_FDCWD: i32 = -100;

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
    ///
    /// # Safety
    ///
    /// This function sets environment variables, see [`std::env::set_var`] for
    /// more details on safety.
    pub unsafe fn current_runtime(
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

        // Safety: the responsibility of the caller.
        unsafe {
            std::env::set_var("SPFS_RUNTIME", rt.name());
        }
        Ok(RuntimeConfigurator::new(self.user, current_ns))
    }

    /// Move this process into the namespace of an existing runtime
    ///
    /// This function will fail if called from a process with multiple threads.
    ///
    /// # Safety
    ///
    /// This function sets environment variables, see [`std::env::set_var`] for
    /// more details on safety.
    pub unsafe fn join_runtime(
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
                };
            }
        };

        if let Err(err) = nix::sched::setns(file, nix::sched::CloneFlags::empty()) {
            return Err(match err {
                nix::errno::Errno::EPERM => Error::new_errno(
                    libc::EPERM,
                    "spfs binary was not installed with required capabilities",
                ),
                _ => err.into(),
            });
        }

        // Safety: the responsibility of the caller.
        unsafe {
            std::env::set_var("SPFS_RUNTIME", rt.name());
        }
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
    ///
    /// Returns false in the case where the path or its parent do not exist.
    async fn is_mounted<P: Into<PathBuf>>(&self, target: P) -> Result<bool> {
        let target = target.into();
        let parent = match target.parent() {
            None => return Ok(false),
            Some(p) => p.to_owned(),
        };

        // A new thread created while holding _guard will be inside the same
        // mount namespace...
        let stat_parent_thread = std::thread::spawn(move || nix::sys::stat::stat(&parent));

        let stat_target_thread = std::thread::spawn(move || nix::sys::stat::stat(&target));

        let (st_parent, st_target) = tokio::task::spawn_blocking(move || {
            let st_parent = stat_parent_thread.join();
            let st_target = stat_target_thread.join();
            (st_parent, st_target)
        })
        .await?;

        let st_parent =
            match st_parent.map_err(|_| Error::String("Failed to stat parent".to_owned()))? {
                // the parent not existing means the child also doesn't exist
                // and so cannot be considered as mounted
                Err(nix::errno::Errno::ENOENT) => {
                    return Ok(false);
                }
                r => r?,
            };
        let st_target =
            match st_target.map_err(|_| Error::String("Failed to stat target".to_owned()))? {
                // a non-existent directory is considered not mounted
                Err(nix::errno::Errno::ENOENT) => {
                    return Ok(false);
                }
                r => r?,
            };

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
    /// Remount key existing mount points so that new mounts and changes
    /// to existing mounts don't propagate to the parent namespace.
    ///
    /// We use MS_SLAVE for system mounts because we still want mount and
    /// unmount events from the system to propagate into this new namespace.
    /// We privatize any existing /spfs mount, though because we are likely
    /// to replace it and don't want to affect any parent runtime.
    pub async fn remove_mount_propagation(&self) -> Result<()> {
        use nix::mount::{MsFlags, mount};

        tracing::debug!("disable sharing of new mounts...");

        let mut res = mount(NONE, "/", NONE, MsFlags::MS_SLAVE, NONE);
        if let Err(err) = res {
            return Err(Error::wrap_nix(
                err,
                "Failed to remove propagation from existing mount: /",
            ));
        }

        if self.is_mounted(SPFS_DIR).await? {
            res = mount(NONE, SPFS_DIR, NONE, MsFlags::MS_PRIVATE, NONE);
            if let Err(err) = res {
                return Err(Error::wrap_nix(
                    err,
                    "Failed to privatize existing mount: /spfs",
                ));
            }
        }

        if self.is_mounted("/tmp").await? {
            res = mount(NONE, "/tmp", NONE, MsFlags::MS_SLAVE, NONE);
            if let Err(err) = res {
                return Err(Error::wrap_nix(
                    err,
                    "Failed to remove propagation from existing mount: /tmp",
                ));
            }
        }
        Ok(())
    }

    /// Check or create the necessary directories for mounting the provided runtime
    pub fn ensure_mount_targets_exist(&self, config: &runtime::Config) -> Result<()> {
        tracing::debug!("ensuring mount targets exist...");
        runtime::makedirs_with_perms(SPFS_DIR, 0o777)
            .map_err(|source| Error::CouldNotCreateSpfsRoot { source })?;

        if let Some(dir) = &config.runtime_dir {
            runtime::makedirs_with_perms(dir, 0o777)
                .map_err(|err| Error::RuntimeWriteError(dir.clone(), err))?
        }
        Ok(())
    }

    pub fn mount_runtime(&self, config: &runtime::Config) -> Result<()> {
        use nix::mount::{MsFlags, mount};

        let dir = match &config.runtime_dir {
            Some(p) => p,
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
            Some(p) => p,
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

    /// Mounts an overlayfs built up from the given list of rendered
    /// layered directories (layer_dirs).
    ///
    /// This first entry in layer_dirs should be the one you expect to
    /// be the bottom-most layer in the overlayfs stack. Each
    /// following entry will be placed on top of the previous one,
    /// with the last entry in layer_dirs becoming the top-most layer
    /// in the overlayfs stack. In the event that multiple layer
    /// directories contain the same file, the one that comes later in
    /// the slice will provide the contents of that file.
    pub(crate) async fn mount_env_overlayfs<P: AsRef<Path>>(
        &self,
        rt: &runtime::Runtime,
        layer_dirs: &[P],
    ) -> Result<()> {
        let spfs_config = crate::Config::current()?;
        if spfs_config.filesystem.use_mount_syscalls {
            mount_overlayfs_syscalls(rt, layer_dirs)?;
        } else {
            mount_overlayfs_command(rt, layer_dirs).await?;
        }
        mount_live_layers(rt).await
    }

    #[cfg(feature = "fuse-backend")]
    pub(crate) async fn mount_fuse_lower_dir(&self, rt: &runtime::Runtime) -> Result<()> {
        self.mount_fuse_onto(rt, &rt.config.lower_dir).await
    }

    #[cfg(feature = "fuse-backend")]
    pub(crate) async fn mount_env_fuse(&self, rt: &runtime::Runtime) -> Result<()> {
        self.mount_fuse_onto(rt, SPFS_DIR).await?;
        mount_live_layers(rt).await
    }

    #[cfg(feature = "fuse-backend")]
    async fn mount_fuse_onto<P>(&self, rt: &runtime::Runtime, path: P) -> Result<()>
    where
        P: AsRef<std::ffi::OsStr>,
    {
        use spfs_encoding::prelude::*;

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
                    )));
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
                let fullpath = node.path.to_path("/");
                if let Some(parent) = fullpath.parent() {
                    tracing::trace!(?parent, "build parent dir for mask");
                    runtime::makedirs_with_perms(parent, 0o777)
                        .map_err(|err| Error::RuntimeWriteError(parent.to_owned(), err))?;
                }
                tracing::trace!(?node.path, "Creating file mask");

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
                )));
            }
        };
        let dest_dir = match new_path.to_str() {
            Some(path) => path,
            None => {
                return Err(Error::RuntimeChangeToDurableError(format!(
                    "new upper_dir '{}' has invalid characters",
                    new_path.display()
                )));
            }
        };

        let args = vec!["-aD", src_dir, dest_dir];
        let cmd_path = match which("rsync") {
            Some(cmd) => cmd,
            None => {
                return Err(Error::RuntimeChangeToDurableError(
                    "rsync is not available on this host".to_string(),
                ));
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
            runtime::MountBackend::FuseOnly => {
                // Unmount any extra paths mounted in the depths of
                // the fuse-only backend before fuse itself is
                // unmounted to avoid issue with lazy unmounting.
                unmount_live_layers(rt).await?;
                std::path::Path::new(SPFS_DIR)
            }
            runtime::MountBackend::OverlayFsWithRenders | runtime::MountBackend::WinFsp => {
                return Ok(());
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
                    )));
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
                            )));
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
pub(crate) const OVERLAY_ARGS_LOWERDIR_APPEND: &str = "lowerdir+";
const OVERLAY_ARGS_LOWERDIR_APPEND_ASSIGN: &str = "lowerdir+=";
const OVERLAY_ARGS_LOWERDIR_ASSIGN: &str = "lowerdir=";

/// A struct for holding the options that will be included
/// in the overlayfs mount command when mounting an environment.
#[derive(Default)]
pub(crate) struct OverlayMountOptions {
    /// Specifies that the overlay file system is mounted as read-only
    pub read_only: bool,
    /// The lowerdir+ mount option will be used to append layers when true.
    pub lowerdir_append: bool,
    /// When true, inodes are indexed in the mount so that
    /// files which share the same inode (hardlinks) are broken
    /// in the final mount and changes to one file don't affect
    /// the other.
    ///
    /// This is the desired default behavior for
    /// spfs, since we rely on hardlinks for deduplication but
    /// expect that file to be able to appear in multiple places
    /// as separate files that just so happen to share the same content.
    ///
    /// When disabled, there will be additional restrictions on
    /// remounting the environment since the filesystem will hold
    /// additional handles and may not unmount while files remain held
    ///
    /// It needs to be disabled for durable runtimes because the
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
            lowerdir_append: true,
            break_hardlinks: true,
            metadata_copy_up: true,
        }
    }

    /// Update state variables to match the features supported by the current overlay version.
    fn query(mut self) -> Self {
        let params = runtime::overlayfs::overlayfs_available_options();
        if self.lowerdir_append && !params.contains(OVERLAY_ARGS_LOWERDIR_APPEND) {
            self.lowerdir_append = false;
        }

        self
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

/// Close a file descriptor when this struct is dropped.
struct CloseFd {
    fd: i32,
}

impl CloseFd {
    fn new(fd: i32) -> Self {
        Self { fd }
    }
}

impl Drop for CloseFd {
    fn drop(&mut self) {
        nix::unistd::close(self.fd as std::ffi::c_int).ok();
    }
}

/// Get the overlayfs arguments for the given list of layer directories.
///
/// This returns an error if the arguments would exceed the legal size limit
/// (if known).
///
/// `layer_dirs` must be ordered so the first entry is the first one
/// to be applied, so the bottom-most layer in the overlayfs stack.
/// The next entry will go on top of that, and so on, until the last
/// entry, which will become the top-most layer in the overlayfs
/// stack. In the event that multiple entries contain the same file,
/// the one that is later in the slice will provide the contents of
/// that file.
pub(crate) fn get_overlay_args<P: AsRef<Path>>(
    rt: &runtime::Runtime,
    layer_dirs: &[P],
) -> Result<String> {
    // Allocate a large buffer up front to avoid resizing/copying.
    let mut args = String::with_capacity(4096);

    let mount_options = OverlayMountOptions::new(rt).query();
    for option in mount_options.to_options() {
        args.push_str(option);
        args.push(',');
    }

    // Have to put the lowerdir directories on the command line in
    // reverse order that they given so that the overlayfs will apply
    // them in the order we want (because it goes right to left, where
    // the rightmost on the command line is the bottom layer, and the
    // leftmost is on the top). For more details see:
    // https://docs.kernel.org/filesystems/overlayfs.html#multiple-lower-layers
    if mount_options.lowerdir_append {
        for path in layer_dirs.iter().rev() {
            args.push_str(OVERLAY_ARGS_LOWERDIR_APPEND_ASSIGN);
            args.push_str(&path.as_ref().to_string_lossy());
            args.push(',');
        }
        args.push_str(OVERLAY_ARGS_LOWERDIR_APPEND_ASSIGN);
    } else {
        args.push_str(OVERLAY_ARGS_LOWERDIR_ASSIGN);
        for path in layer_dirs.iter().rev() {
            args.push_str(&path.as_ref().to_string_lossy());
            args.push(':');
        }
    }
    args.push_str(&rt.config.lower_dir.to_string_lossy());

    args.push_str(",upperdir=");
    args.push_str(&rt.config.upper_dir.to_string_lossy());

    args.push_str(",workdir=");
    args.push_str(&rt.config.work_dir.to_string_lossy());

    match nix::unistd::sysconf(nix::unistd::SysconfVar::PAGE_SIZE) {
        Err(_) => tracing::debug!("failed to get page size for checking arg length"),
        Ok(None) => (),
        Ok(Some(size)) => {
            if args.len() as i64 > size - 1 {
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

/// Mount overlayfs layers using the mount command.
async fn mount_overlayfs_command<P: AsRef<Path>>(
    rt: &runtime::Runtime,
    layer_dirs: &[P],
) -> Result<()> {
    tracing::debug!("mounting the overlay filesystem using mount...");
    let overlay_args = get_overlay_args(rt, layer_dirs)?;
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

/// Mount overlayfs layers using mount syscalls.
fn mount_overlayfs_syscalls<P: AsRef<Path>>(rt: &runtime::Runtime, layer_dirs: &[P]) -> Result<()> {
    tracing::debug!("mounting the overlay filesystem using syscalls...");

    let mount_options = OverlayMountOptions::new(rt);
    let params = runtime::overlayfs::overlayfs_available_options();

    // Safety: filesystem name is null-terminated and fd will be closed on exec.
    let rc = unsafe { syscall!(SYS_fsopen, FSOPEN_OVERLAY_CSTR.as_ptr(), FSOPEN_CLOEXEC) };
    let fd = rc.as_u64_unchecked() as i32;
    if fd < 0 {
        return Err(format!(
            "mount_overlayfs_syscalls::SYS_fsopen(overlay, FSOPEN_CLOEXEC) error: {fd}"
        )
        .into());
    }

    // Safety: fd is valid and arguments are null-terminated.
    let rc = unsafe {
        syscall!(
            SYS_fsconfig,
            fd,
            FSCONFIG_SET_STRING,
            FSCONFIG_SOURCE_CSTR.as_ptr(),
            FSCONFIG_NONE_CSTR.as_ptr(),
            FSCONFIG_DEFAULT
        )
    }
    .as_u64_unchecked();
    if rc != 0 {
        return Err(format!(
            "mount_overlayfs_syscalls::SYS_fsconfig({}, FSCONFIG_SET_STRING, source, none, 0) error: {}",
            fd, rc
        )
        .into());
    }

    if mount_options.read_only {
        // Safety: fd is valid and arguments are null-terminated.
        let rc = unsafe {
            syscall!(
                SYS_fsconfig,
                fd,
                FSCONFIG_SET_FLAG,
                FSCONFIG_RO_CSTR.as_ptr(),
                std::ptr::null::<u8>(),
                FSCONFIG_DEFAULT
            )
        }
        .as_u64_unchecked();
        if rc != 0 {
            return Err(format!(
                "mount_overlayfs_syscalls::SYS_fsconfig({}, FSCONFIG_SET_FLAG, ro, NULL) error: {}",
                fd, rc
            )
            .into());
        }
    }

    if !mount_options.break_hardlinks && params.contains(OVERLAY_ARGS_INDEX) {
        // Safety: fd is valid and arguments are null-terminated.
        let rc = unsafe {
            syscall!(
                SYS_fsconfig,
                fd,
                FSCONFIG_SET_STRING,
                FSCONFIG_INDEX_CSTR.as_ptr(),
                FSCONFIG_ON_CSTR.as_ptr(),
                FSCONFIG_DEFAULT
            )
        }
        .as_u64_unchecked();
        if rc != 0 {
            return Err(format!(
                "mount_overlayfs_syscalls::SYS_fsconfig({}, FSCONFIG_SET_STRING, index, on, 0) error: {}",
                fd, rc
            )
            .into());
        }
    }

    if mount_options.metadata_copy_up && params.contains(OVERLAY_ARGS_METACOPY) {
        // Safety: fd is valid and arguments are null-terminated.
        let rc = unsafe {
            syscall!(
                SYS_fsconfig,
                fd,
                FSCONFIG_SET_STRING,
                FSCONFIG_METACOPY_CSTR.as_ptr(),
                FSCONFIG_ON_CSTR.as_ptr(),
                FSCONFIG_DEFAULT
            )
        }
        .as_u64_unchecked();
        if rc != 0 {
            return Err(format!(
                "mount_overlayfs_syscalls::SYS_fsconfig({}, FSCONFIG_SET_STRING, metacopy, on, 0) error: {}",
                fd, rc
            )
            .into());
        }
    }

    // Setup the lowerdir directories in reverse order so that the overlayfs will apply them in
    // the order we want. The first path specified is the top layer and the last is the bottom layer.
    // For more details see: https://docs.kernel.org/filesystems/overlayfs.html#multiple-lower-layers

    if mount_options.lowerdir_append {
        let lower_dir =
            CString::new(rt.config.lower_dir.to_string_lossy().as_ref()).map_err(|err| {
                format!(
                    "unable to create CString from {}: {}",
                    rt.config.lower_dir.to_string_lossy().as_ref(),
                    err
                )
            })?;
        for layer in layer_dirs.iter().rev() {
            let path = CString::new(layer.as_ref().to_string_lossy().as_ref()).map_err(|err| {
                format!(
                    "unable to create CString from {}: {}",
                    layer.as_ref().to_string_lossy().as_ref(),
                    err
                )
            })?;
            // Safety: fd is valid and arguments are null-terminated.
            let rc = unsafe {
                syscall!(
                    SYS_fsconfig,
                    fd,
                    FSCONFIG_SET_STRING,
                    FSCONFIG_LOWERDIR_APPEND_CSTR.as_ptr(),
                    path.as_ptr(),
                    FSCONFIG_DEFAULT
                )
            }
            .as_u64_unchecked();
            if rc != 0 {
                return Err(format!(
                    "mount_overlayfs_syscalls::SYS_fsconfig({}, FSCONFIG_SET_STRING, lowerdir, {:?}, 0) error: {}",
                    fd, path, rc
                )
                .into());
            }
        }
        // Safety: fd is valid and arguments are null-terminated.
        let rc = unsafe {
            syscall!(
                SYS_fsconfig,
                fd,
                FSCONFIG_SET_STRING,
                FSCONFIG_LOWERDIR_APPEND_CSTR.as_ptr(),
                lower_dir.as_ptr(),
                FSCONFIG_DEFAULT
            )
        }
        .as_u64_unchecked();
        if rc != 0 {
            return Err(format!(
                "mount_overlayfs_syscalls::SYS_fsconfig({}, FSCONFIG_SET_STRING, lowerdir, {:?}, 0) error: {}",
                fd, lower_dir, rc
            )
            .into());
        }
    } else {
        let mut lower_dir_value = String::new();
        for layer in layer_dirs.iter().rev() {
            lower_dir_value += layer.as_ref().to_string_lossy().as_ref();
            lower_dir_value += ":";
        }
        lower_dir_value += rt.config.lower_dir.to_string_lossy().as_ref();
        let lower_dir = CString::new(lower_dir_value.as_str())
            .map_err(|err| format!("unable to create CString from {lower_dir_value}: {err}"))?;

        // Safety: fd is valid and arguments are null-terminated.
        let rc = unsafe {
            syscall!(
                SYS_fsconfig,
                fd,
                FSCONFIG_SET_STRING,
                FSCONFIG_LOWERDIR_CSTR.as_ptr(),
                lower_dir.as_ptr(),
                FSCONFIG_DEFAULT
            )
        }
        .as_u64_unchecked();
        if rc != 0 {
            return Err(format!(
                "mount_overlayfs_syscalls::SYS_fsconfig({}, FSCONFIG_SET_STRING, lowerdir, {:?}, 0) error: {}",
                fd, lower_dir, rc
            )
            .into());
        }
    }

    let upper_dir =
        CString::new(rt.config.upper_dir.to_string_lossy().as_ref()).map_err(|err| {
            format!(
                "unable to create CString from {}: {}",
                rt.config.upper_dir.to_string_lossy(),
                err
            )
        })?;
    // Safety: fd is valid and arguments are null-terminated.
    let rc = unsafe {
        syscall!(
            SYS_fsconfig,
            fd,
            FSCONFIG_SET_STRING,
            FSCONFIG_UPPERDIR_CSTR.as_ptr(),
            upper_dir.as_ptr(),
            FSCONFIG_DEFAULT
        )
    }
    .as_u64_unchecked();
    if rc != 0 {
        return Err(format!(
            "mount_overlayfs_syscalls::SYS_fsconfig({}, FSCONFIG_SET_STRING, upperdir, {:?}) error: {}",
            fd, upper_dir, rc
        )
        .into());
    }

    let work_dir = CString::new(rt.config.work_dir.to_string_lossy().as_ref()).map_err(|err| {
        format!(
            "unable to create CString from {}: {}",
            rt.config.work_dir.to_string_lossy(),
            err
        )
    })?;
    // Safety: fd is valid and arguments are null-terminated.
    let rc = unsafe {
        syscall!(
            SYS_fsconfig,
            fd,
            FSCONFIG_SET_STRING,
            FSCONFIG_WORKDIR_CSTR.as_ptr(),
            work_dir.as_ptr(),
            FSCONFIG_DEFAULT
        )
    }
    .as_u64_unchecked();
    if rc != 0 {
        return Err(format!(
            "fsconfig({}, FSCONFIG_SET_STRING, workdir, {:?}) error: {}",
            fd, work_dir, rc
        )
        .into());
    }

    // Safety: fd is valid and FSCONFIG_CMD_CREATE must be supported.
    let rc = unsafe {
        syscall!(
            SYS_fsconfig,
            fd,
            FSCONFIG_CMD_CREATE,
            std::ptr::null::<u8>(),
            std::ptr::null::<u8>(),
            FSCONFIG_DEFAULT
        )
    }
    .as_u64_unchecked() as i32;
    if rc != 0 {
        return Err(format!(
            "mount_overlayfs_syscalls::SYS_fsconfig({}, FSCONFIG_CMD_CREATE, NULL, NULL, 0) error: {}",
            fd, rc
        )
        .into());
    }

    // Safety: fd is valid and and will be closed on exec.
    let mount_fd =
        unsafe { syscall!(SYS_fsmount, fd, FSMOUNT_CLOEXEC, 0u32) }.as_u64_unchecked() as i32;
    if mount_fd < 0 {
        return Err(format!("fsmount({}, FSMOUNT_CLOEXEC, 0) error: {}", mount_fd, rc).into());
    }
    let _close_mount_fd = CloseFd::new(mount_fd); // Close mount_fd when dropped.

    let attr_set = if mount_options.read_only {
        MOUNT_ATTR_RDONLY as u64
    } else {
        0
    };
    let mount_attrs = mount_attr {
        attr_set,
        attr_clr: 0,
        propagation: 0,
        userns_fd: 0,
    };
    // Safety: mount_fd is valid and mount_attr matches the C struct from mount.h.
    let rc = unsafe {
        syscall!(
            SYS_mount_setattr,
            mount_fd,
            EMPTY_CSTR.as_ptr(),
            AT_EMPTY_PATH,
            std::ptr::addr_of!(mount_attrs),
            MOUNT_ATTR_SIZE_VER0
        )
    }
    .as_u64_unchecked();
    if rc != 0 {
        return Err(format!(
            "mount_overlayfs_syscalls::SYS_mount_setattr({}, AT_EMPTY_PATH, &mount_attrs, {}) error: {}",
            mount_fd, MOUNT_ATTR_SIZE_VER0, rc
        )
        .into());
    }

    // Safety: mount_fd is valid and can be mounted at "/spfs".
    let rc = unsafe {
        syscall!(
            SYS_move_mount,
            mount_fd,
            EMPTY_CSTR.as_ptr(),
            AT_FDCWD,
            SPFS_DIR_CSTR.as_ptr(),
            MOVE_MOUNT_F_EMPTY_PATH
        )
    }
    .as_u64_unchecked();
    if rc != 0 {
        return Err(format!(
            "mount_overlayfs_syscalls::SYS_move_mount({}, \"\", AT_FDCWD, {:?}, MOVE_MOUNT_F_EMPTY_PATH) error: {}",
            mount_fd, SPFS_DIR_CSTR, rc
        )
        .into());
    }

    Ok(())
}

/// Mount bind mounts from live layers in the runtime over the top of paths inside /spfs.
async fn mount_live_layers(rt: &runtime::Runtime) -> Result<()> {
    // This requires the mount destinations to exist under
    // /spfs/. If they do not, the mount commands will error. The
    // mount destinations are either provided by one of the layers
    // in the runtime, or by an earlier call to
    // ensure_extra_bind_mount_locations_exist() made in
    // initialize_runtime()
    let live_layers = rt.live_layers();
    if !live_layers.is_empty() {
        let spfs_config = crate::Config::current()?;
        if spfs_config.filesystem.use_mount_syscalls {
            mount_live_layers_syscalls(live_layers)?;
        } else {
            mount_live_layers_command(live_layers).await?;
        }
    }

    Ok(())
}

/// Bind-mount live layers using the "mount" command.
async fn mount_live_layers_command(live_layers: &Vec<runtime::LiveLayer>) -> Result<()> {
    tracing::debug!(
        "mounting extra bind mounts over the {SPFS_DIR} filesystem using the mount command"
    );
    let mount = super::resolve::which("mount").unwrap_or_else(|| "/usr/bin/mount".into());

    for layer in live_layers {
        let injection_mounts = layer.bind_mounts();

        for extra_mount in injection_mounts {
            let dest = if extra_mount.dest.starts_with(SPFS_DIR_PREFIX) {
                PathBuf::from(extra_mount.dest.clone())
            } else {
                PathBuf::from(SPFS_DIR).join(extra_mount.dest.clone())
            };

            let mut cmd = tokio::process::Command::new(mount.clone());
            cmd.arg("--bind");
            cmd.arg(extra_mount.src.to_string_lossy().into_owned());
            cmd.arg(dest);
            tracing::debug!("About to run: {cmd:?}");

            match cmd.status().await {
                Err(err) => return Err(Error::process_spawn_error("mount".to_owned(), err, None)),
                Ok(status) => match status.code() {
                    Some(0) => (),
                    _ => return Err(format!(
                        "Failed to inject bind mount into the {SPFS_DIR} filesystem using: {cmd:?}"
                    )
                    .into()),
                },
            }
        }
    }

    Ok(())
}

/// Bind-mount live layers using mount syscalls.
fn mount_live_layers_syscalls(live_layers: &Vec<runtime::LiveLayer>) -> Result<()> {
    tracing::debug!(
        "mounting extra bind mounts over the {SPFS_DIR} filesystem using mount syscalls"
    );

    for layer in live_layers {
        let injection_mounts = layer.bind_mounts();

        for extra_mount in injection_mounts {
            let Ok(src) = extra_mount.src.canonicalize() else {
                return Err(format!("unable to canonicalize {:?}", extra_mount.src).into());
            };
            let dest = if extra_mount.dest.starts_with(SPFS_DIR_PREFIX) {
                PathBuf::from(extra_mount.dest.clone())
            } else {
                PathBuf::from(SPFS_DIR).join(extra_mount.dest.clone())
            };
            let src_path = CString::new(src.to_string_lossy().as_ref()).map_err(|err| {
                format!(
                    "unable to create CString from {}: {err}",
                    src.to_string_lossy().as_ref(),
                )
            })?;
            let dest_path = CString::new(dest.to_string_lossy().as_ref()).map_err(|err| {
                format!(
                    "unable to create CString from {}: {err}",
                    dest.to_string_lossy().as_ref(),
                )
            })?;

            // Safety: the source path must by valid for use with open_tree().
            let mount_fd = unsafe {
                syscall!(
                    SYS_open_tree,
                    AT_FDCWD,
                    src_path.as_ptr(),
                    OPEN_TREE_CLONE | OPEN_TREE_CLOEXEC
                )
            }
            .as_u64_unchecked() as i32;
            if mount_fd < 0 {
                return Err(format!("mount_live_layers_syscalls::SYS_open_tree(AT_FDCWD, {:?}, OPEN_TREE_CLONE | OPEN_TREE_CLOEXEC) error: {}",
                    src_path, mount_fd).into());
            }
            let _close_mount_fd = CloseFd::new(mount_fd); // Close mount_fd when dropped.

            // Safety: mount_fd must be valid so that we can move the mount to "/spfs".
            let rc = unsafe {
                syscall!(
                    SYS_move_mount,
                    mount_fd,
                    EMPTY_CSTR.as_ptr(),
                    AT_FDCWD,
                    dest_path.as_ptr(),
                    MOVE_MOUNT_F_EMPTY_PATH
                )
            }
            .as_u64_unchecked() as i32;
            if rc != 0 {
                return Err(format!(
                    "mount_overlayfs_syscalls::SYS_move_mount({}, \"\", AT_FDCWD, {:?}, MOVE_MOUNT_F_EMPTY_PATH) error: {}",
                    mount_fd, dest_path, rc
                )
                .into());
            }
        }
    }

    Ok(())
}

/// Unmount the bind mounted items from the live layers
async fn unmount_live_layers(rt: &runtime::Runtime) -> Result<()> {
    let live_layers = rt.live_layers();
    if !live_layers.is_empty() {
        let spfs_config = crate::Config::current()?;
        if spfs_config.filesystem.use_mount_syscalls {
            unmount_live_layers_syscalls(live_layers)?;
        } else {
            unmount_live_layers_command(live_layers).await?;
        }
    }

    Ok(())
}

/// Unmount live layers using the "umount" command.
async fn unmount_live_layers_command(live_layers: &Vec<runtime::LiveLayer>) -> Result<()> {
    tracing::debug!(
        "unmounting the extra bind mounts from the {SPFS_DIR} filesystem using the umount command ..."
    );
    let umount = super::resolve::which("umount").unwrap_or_else(|| "/usr/bin/umount".into());
    for layer in live_layers {
        let injection_mounts = layer.bind_mounts();
        for extra_mount in injection_mounts {
            let mut cmd = tokio::process::Command::new(umount.clone());
            cmd.arg(PathBuf::from(SPFS_DIR).join(extra_mount.dest.clone()));
            tracing::debug!("About to run: {cmd:?}");
            match cmd.status().await {
                Err(err) => {
                    return Err(Error::process_spawn_error("umount".to_owned(), err, None))
                }
                Ok(status) => match status.code() {
                    Some(0) => (),
                    _ => return Err(format!("Failed to unmount a bind mount injected into the {SPFS_DIR} filesystem using: {cmd:?}").into()),
                },
            }
        }
    }

    Ok(())
}

/// Unmount live layers using umount syscalls.
fn unmount_live_layers_syscalls(live_layers: &Vec<runtime::LiveLayer>) -> Result<()> {
    tracing::debug!(
        "unmounting the extra bind mounts from the {SPFS_DIR} filesystem using syscalls ..."
    );
    for layer in live_layers {
        let injection_mounts = layer.bind_mounts();
        for extra_mount in injection_mounts {
            let dest = if extra_mount.dest.starts_with(SPFS_DIR_PREFIX) {
                PathBuf::from(extra_mount.dest.clone())
            } else {
                PathBuf::from(SPFS_DIR).join(extra_mount.dest.clone())
            };
            let flags = nix::mount::MntFlags::empty();
            let result = nix::mount::umount2(&dest, flags);
            if let Err(err) = result {
                return Err(Error::wrap_nix(err, format!("Failed to unmount {dest:?}")));
            }
        }
    }

    Ok(())
}

/// Prevent a structure from being [`Send`].
struct NotSendMarker(std::marker::PhantomData<*mut u8>);

/// Prevent a structure from being [`Sync`].
struct NotSyncMarker(std::marker::PhantomData<std::cell::Cell<u8>>);
