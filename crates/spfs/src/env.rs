// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

//! Functions related to the setup and management of the spfs runtime environment
//! and related system namespacing
use std::collections::HashSet;
use std::os::unix::io::AsRawFd;
use std::path::Path;

use super::runtime;
use crate::{Error, Result};

static PROC_DIR: &str = "/proc";
static SPFS_DIR: &str = "/spfs";

const NONE: Option<&str> = None;

/// Move this thread into the namespace of an existing runtime
pub fn join_runtime(rt: &runtime::Runtime) -> Result<()> {
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
                std::io::ErrorKind::NotFound => Err(Error::UnknownRuntime(rt.name().into())),
                _ => Err(err.into()),
            }
        }
    };

    if let Err(err) = nix::sched::setns(file.as_raw_fd(), nix::sched::CloneFlags::empty()) {
        return Err(match err.as_errno() {
            Some(nix::errno::Errno::EPERM) => Error::new_errno(
                libc::EPERM,
                "spfs binary was not installed with required capabilities",
            ),
            _ => err.into(),
        });
    }

    std::env::set_var("SPFS_RUNTIME", rt.name());
    drop_all_capabilities()?;
    Ok(())
}

// Checks if the current process will be able to join an existing runtime
fn check_can_join() -> Result<()> {
    if palaver::thread::count() != 1 {
        return Err("Program must be single-threaded to join an existing runtime".into());
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

pub fn enter_mount_namespace() -> Result<()> {
    tracing::debug!("entering mount namespace...");
    if let Err(err) = nix::sched::unshare(nix::sched::CloneFlags::CLONE_NEWNS) {
        Err(Error::wrap_nix(err, "Failed to enter mount namespace"))
    } else {
        Ok(())
    }
}

/// Run an spfs monitor for the provided runtime
///
/// The monitor command will spawn but immediately fail
/// if there is already a monitor registered to this runtime
pub fn spawn_monitor_for_runtime(rt: &runtime::Runtime) -> Result<tokio::process::Child> {
    let exe = match super::resolve::which_spfs("monitor") {
        None => return Err(Error::MissingBinary("spfs")),
        Some(exe) => exe,
    };

    let mut cmd = tokio::process::Command::new(exe);
    cmd.arg("--runtime-storage");
    cmd.arg(rt.storage().address().as_str());
    cmd.arg("--runtime");
    cmd.arg(rt.name());
    // the monitor process should be fully detached from any controlling
    // terminal. Otherwise, using spfs run under output-capturing circumstances
    // can cause the command to hang forever. Eg: output=$(spfs run - -- echo "hello")
    cmd.stdout(std::process::Stdio::null());
    cmd.stderr(std::process::Stdio::null());
    cmd.stdin(std::process::Stdio::null());

    unsafe {
        // avoid creating zombie processes by moving the monitor
        // into a separate process group
        cmd.pre_exec(|| match nix::unistd::setsid() {
            Ok(_pid) => Ok(()),
            Err(err) => Err(match err.as_errno() {
                Some(errno) => std::io::Error::from_raw_os_error(errno as i32),
                None => std::io::Error::new(std::io::ErrorKind::Other, err),
            }),
        });
    }

    Ok(cmd.spawn()?)
}

/// When provided an active runtime, wait until all contained processes exit
///
/// This is a privileged operation that may fail with a permission
/// issue if the calling process is not root or CAP_NET_ADMIN
pub async fn wait_for_empty_runtime(rt: &runtime::Runtime) -> Result<()> {
    let pid = match rt.status.owner {
        None => return Err(Error::RuntimeNotInitialized(rt.name().into())),
        Some(pid) => pid,
    };

    // we could use our own pid here, but then when running in a container
    // or pid namespace the process id would actually be wrong. Passing
    // zero will get the kernel to determine our pid from its perspective
    let mut monitor = cnproc::PidMonitor::from_id(0)
        .map_err(|e| crate::Error::String(format!("failed to establish process monitor: {e}")))?;

    let mut tracked_processes = HashSet::new();
    let (events_send, mut events_recv) = tokio::sync::mpsc::unbounded_channel();

    // NOTE(rbottriell):
    // scan for any processes that were already spawned before
    // we had setup the monitoring socket, this will establish
    // the initial set of parent processes that we can track the
    // lineage of
    // This explicitly happens AFTER setting up the monitor so that
    // we will receive news of any new process created out of whatever
    // we find in this scan
    // BUG(rbottriell) There is still a race condition here where a
    // child is created as we scan, and the parent exits before we
    // are able to see it so we don't think the child is relevant
    let current_pids = find_processes_in_shared_mount_namespace(pid).await?;
    tracked_processes.extend(current_pids);

    // it's possible that the runtime process(es)
    // completed before we were even able to see them
    if tracked_processes.is_empty() {
        return Ok(());
    }

    tokio::task::spawn_blocking(move || {
        while let Some(event) = monitor.recv() {
            if let Err(_err) = events_send.send(event) {
                // the receiver has stopped listening, no need to continue
                return;
            }
        }
        tracing::warn!("monitor event stream ended unexpectedly");
    });

    while let Some(event) = events_recv.recv().await {
        match event {
            cnproc::PidEvent::Exec(_pid) => {
                // exec is just one process turning into a new one with
                // the pid remaining the same, so we are not interested...
                // remember that launching a new process is a fork and then exec
            }
            cnproc::PidEvent::Fork { parent, pid } => {
                if tracked_processes.contains(&(parent as u32)) {
                    tracked_processes.insert(pid as u32);
                    tracing::trace!(?tracked_processes, "runtime monitor");
                }
            }
            cnproc::PidEvent::Exit(pid) => {
                if tracked_processes.remove(&(pid as u32)) {
                    tracing::trace!(?tracked_processes, "runtime monitor");
                }
                if tracked_processes.is_empty() {
                    break;
                }
            }
            cnproc::PidEvent::Coredump(_) => {}
        }
    }
    Ok(())
}

/// Identify the mount namespace of the provided process id, and
/// then find other processes on this machine which share that namespace
///
/// If the provided process does not exist, an empty set is returned
async fn find_processes_in_shared_mount_namespace(pid: u32) -> Result<HashSet<u32>> {
    let ns_path = std::path::Path::new(PROC_DIR)
        .join(pid.to_string())
        .join("ns/mnt");

    tracing::debug!(?ns_path, "Getting process namespace");
    let ns = match tokio::fs::read_link(&ns_path).await {
        Ok(ns) => ns,
        Err(err) => match err.kind() {
            // it's possible that the runtime process already exited
            // or was never started
            std::io::ErrorKind::NotFound => return Ok(HashSet::new()),
            _ => return Err(err.into()),
        },
    };
    find_processes_in_mount_namespace(&ns).await
}

/// Provided the namespace symlink content from /proc fs,
/// scan for all processes that share the same namespace
async fn find_processes_in_mount_namespace(ns: &std::path::Path) -> Result<HashSet<u32>> {
    let mut found_processes = HashSet::new();

    let mut read_dir = tokio::fs::read_dir(PROC_DIR).await?;
    while let Some(entry) = read_dir.next_entry().await? {
        let pid = match entry.file_name().to_str().map(|s| s.parse::<u32>()) {
            Some(Ok(pid)) => pid,
            // don't bother reading proc dirs that are not named with a valid pid
            _ => continue,
        };
        let found_ns = match tokio::fs::read_link(entry.path().join("ns/mnt")).await {
            Ok(p) => p,
            Err(err) => match err.raw_os_error() {
                Some(libc::ENOENT) => continue,
                Some(libc::ENOTDIR) => continue,
                Some(libc::EACCES) => continue,
                Some(libc::EPERM) => continue,
                _ => {
                    return Err(err.into());
                }
            },
        };
        if found_ns == ns {
            found_processes.insert(pid);
        }
    }
    Ok(found_processes)
}

pub fn privatize_existing_mounts() -> Result<()> {
    use nix::mount::{mount, MsFlags};

    tracing::debug!("privatizing existing mounts...");

    let mut res = mount(NONE, "/", NONE, MsFlags::MS_PRIVATE, NONE);
    if let Err(err) = res {
        return Err(Error::wrap_nix(
            err,
            "Failed to privatize existing mount: /",
        ));
    }

    if is_mounted("/tmp")? {
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

pub fn ensure_mount_targets_exist(config: &runtime::Config) -> Result<()> {
    tracing::debug!("ensuring mount targets exist...");
    runtime::makedirs_with_perms(SPFS_DIR, 0o777)
        .map_err(|err| err.wrap(format!("Failed to create {SPFS_DIR}")))?;

    if let Some(dir) = &config.runtime_dir {
        runtime::makedirs_with_perms(dir, 0o777)
            .map_err(|err| err.wrap(format!("Failed to create {dir:?}")))?
    }
    Ok(())
}

pub fn ensure_mounts_already_exist() -> Result<()> {
    tracing::debug!("ensuring mounts already exist...");
    let res = is_mounted(SPFS_DIR);
    match res {
        Err(err) => Err(err.wrap("Failed to check for existing mount")),
        Ok(true) => Ok(()),
        Ok(false) => Err(format!("'{SPFS_DIR}' is not mounted, will not remount").into()),
    }
}

pub struct Uids {
    pub uid: nix::unistd::Uid,
    pub euid: nix::unistd::Uid,
}

pub fn become_root() -> Result<Uids> {
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
    Ok(Uids {
        euid: original_euid,
        uid: original_uid,
    })
}

pub fn mount_runtime(config: &runtime::Config) -> Result<()> {
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

pub fn unmount_runtime(config: &runtime::Config) -> Result<()> {
    let dir = match &config.runtime_dir {
        Some(ref p) => p,
        None => return Ok(()),
    };

    tracing::debug!("unmounting existing runtime...");
    let result = nix::mount::umount(dir);
    if let Err(err) = result {
        return Err(Error::wrap_nix(err, format!("Failed to unmount {dir:?}")));
    }
    Ok(())
}

pub async fn setup_runtime(rt: &runtime::Runtime) -> Result<()> {
    tracing::debug!("setting up runtime...");
    rt.ensure_required_directories().await
}

pub fn mask_files(
    config: &runtime::Config,
    manifest: &super::tracking::Manifest,
    owner: nix::unistd::Uid,
) -> Result<()> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    tracing::debug!("masking deleted files...");

    let prefix = config.upper_dir.to_str().ok_or_else(|| {
        crate::Error::String(format!(
            "configured runtime upper_dir has invalid characters: {:?}",
            config.upper_dir
        ))
    })?;
    let nodes: Vec<_> = manifest.walk_abs(&prefix).collect();
    for node in nodes.iter() {
        if !node.entry.kind.is_mask() {
            continue;
        }
        let fullpath = node.path.to_path("");
        if let Some(parent) = fullpath.parent() {
            tracing::trace!(?parent, "build parent dir for mask");
            runtime::makedirs_with_perms(parent, 0o777)?;
        }
        tracing::trace!(?node.path, "Creating file mask");

        let existing = fullpath.symlink_metadata().ok();
        if let Some(meta) = existing {
            if runtime::is_removed_entry(&meta) {
                continue;
            }
            if meta.is_file() {
                std::fs::remove_file(&fullpath)?;
            } else {
                std::fs::remove_dir_all(&fullpath)?;
            }
        }

        if let Err(err) = nix::sys::stat::mknod(
            &fullpath,
            nix::sys::stat::SFlag::S_IFCHR,
            nix::sys::stat::Mode::empty(),
            0,
        ) {
            return Err(Error::wrap_nix(
                err,
                format!("Failed to create file mask: {fullpath:?}"),
            ));
        }
    }

    for node in nodes.iter().rev() {
        if !node.entry.kind.is_tree() {
            continue;
        }
        let fullpath = node.path.to_path("/");
        if !fullpath.is_dir() {
            continue;
        }
        let existing = &fullpath.symlink_metadata()?;
        if existing.permissions().mode() != node.entry.mode {
            if let Err(err) = std::fs::set_permissions(
                &fullpath,
                std::fs::Permissions::from_mode(node.entry.mode),
            ) {
                match err.kind() {
                    std::io::ErrorKind::NotFound => continue,
                    _ => {
                        return Err(Error::wrap_io(
                            err,
                            format!("Failed to set permissions on masked file [{}]", node.path),
                        ));
                    }
                }
            }
        }
        if existing.uid() != owner.as_raw() {
            if let Err(err) = nix::unistd::chown(&fullpath, Some(owner), None) {
                match err.as_errno() {
                    Some(nix::errno::Errno::ENOENT) => continue,
                    _ => {
                        return Err(Error::wrap_nix(
                            err,
                            format!("Failed to set ownership on masked file [{}]", node.path),
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

pub fn get_overlay_args<P: AsRef<Path>>(
    config: &runtime::Config,
    lower_dirs: impl IntoIterator<Item = P>,
) -> Result<String> {
    let mut args = format!("lowerdir={}", config.lower_dir.display());
    for path in lower_dirs.into_iter() {
        args = format!("{args}:{}", path.as_ref().to_string_lossy());
    }

    args = format!(
        "{args},upperdir={},workdir={}",
        config.upper_dir.display(),
        config.work_dir.display()
    );

    match nix::unistd::sysconf(nix::unistd::SysconfVar::PAGE_SIZE) {
        Err(_) => tracing::debug!("failed to get page size for checking arg length"),
        Ok(None) => (),
        Ok(Some(size)) => {
            if args.as_bytes().len() as i64 > size - 1 {
                return Err("Mount args would be too large for the kernel, reduce your config value for filesystem.max_layers".into());
            }
        }
    };
    Ok(args)
}

pub fn mount_env<P: AsRef<Path>>(
    rt: &runtime::Runtime,
    lower_dirs: impl IntoIterator<Item = P>,
) -> Result<()> {
    tracing::debug!("mounting the overlay filesystem...");
    let mut overlay_args = get_overlay_args(&rt.config, lower_dirs)?;
    if !rt.status.editable {
        overlay_args = format!("ro,{overlay_args}");
    }
    tracing::debug!("/usr/bin/mount -t overlay -o {overlay_args} none {SPFS_DIR}");
    // for some reason, the overlay mount process creates a bad filesystem if the
    // mount command is called directly from this process. It may be some default
    // option or minor detail in how the standard mount command works - possibly related
    // to this process eventually dropping privileges, but that is uncertain right now
    let mut cmd = std::process::Command::new("mount");
    cmd.args(&["-t", "overlay"]);
    cmd.arg("-o");
    cmd.arg(overlay_args);
    cmd.arg("none");
    cmd.arg(SPFS_DIR);
    match cmd.status() {
        Err(err) => Err(Error::wrap_io(
            err,
            "Failed to run mount command for overlay",
        )),
        Ok(status) => match status.code() {
            Some(0) => Ok(()),
            _ => Err("Failed to mount overlayfs".into()),
        },
    }
}

pub fn unmount_env() -> Result<()> {
    tracing::debug!("unmounting existing env...");
    // Perform a lazy unmount in case there are still open handles to files.
    // This way we can mount over the old one without worrying about business
    let result = nix::mount::umount2(SPFS_DIR, nix::mount::MntFlags::MNT_DETACH);
    if let Err(err) = result {
        return Err(Error::wrap_nix(
            err,
            format!("Failed to unmount {SPFS_DIR}"),
        ));
    }
    Ok(())
}

pub fn become_original_user(uids: Uids) -> Result<()> {
    tracing::debug!("dropping root...");
    let mut result = nix::unistd::setuid(uids.uid);
    if let Err(err) = result {
        return Err(Error::wrap_nix(
            err,
            "Failed to become regular user (actual)",
        ));
    }
    result = nix::unistd::seteuid(uids.euid);
    if let Err(err) = result {
        return Err(Error::wrap_nix(
            err,
            "Failed to become regular user (effective)",
        ));
    }
    Ok(())
}

fn is_mounted<P: AsRef<Path>>(target: P) -> Result<bool> {
    let target = target.as_ref();
    let parent = match target.parent() {
        None => return Ok(false),
        Some(p) => p,
    };

    let st_parent = nix::sys::stat::stat(parent)?;
    let st_target = nix::sys::stat::stat(target)?;

    Ok(st_target.st_dev != st_parent.st_dev)
}

// Drop all of the capabilities held by the current thread
pub fn drop_all_capabilities() -> Result<()> {
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
