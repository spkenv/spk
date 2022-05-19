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
static RUNTIME_DIR: &str = "/tmp/spfs-runtime";
static RUNTIME_UPPER_DIR: &str = "/tmp/spfs-runtime/upper";
static RUNTIME_LOWER_DIR: &str = "/tmp/spfs-runtime/lower";
static RUNTIME_WORK_DIR: &str = "/tmp/spfs-runtime/work";

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

    Ok(cmd.spawn()?)
}

pub async fn wait_for_empty_runtime(rt: &runtime::Runtime) -> Result<()> {
    let pid = match rt.status.owner {
        None => return Err(Error::RuntimeNotInitialized(rt.name().into())),
        Some(pid) => pid,
    };

    let mut monitor = cnproc::PidMonitor::new()
        .map_err(|e| crate::Error::String(format!("failed to establish process monitor: {e}")))?;

    let mut tracked_processes = HashSet::new();
    let (events_send, mut events_recv) = tokio::sync::mpsc::unbounded_channel();

    // NOTE(rbottriell):
    // scan for any processes that were already spawned before
    // we had setup the monitoring socket
    let current_pids = find_processes_in_shared_mount_namespace(pid).await?;
    tracked_processes.extend(current_pids);

    tokio::task::spawn_blocking(move || {
        while let Some(event) = monitor.recv() {
            if let Err(_err) = events_send.send(event) {
                // the receiver has stopped listening, no need to continue
                return;
            }
        }
        tracing::warn!("monitor event stream ended unexpectedly");
    });

    let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    while let Some(event) = events_recv.recv().await {
        match event {
            cnproc::PidEvent::Exec(_pid) => {
                // exec is just one process turning into a new one, but
                // the pid will remain the same and so we are not interested
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

async fn find_processes_in_shared_mount_namespace(pid: u32) -> Result<HashSet<u32>> {
    let ns_path = std::path::Path::new(PROC_DIR)
        .join(pid.to_string())
        .join("ns/mnt");

    tracing::debug!(?ns_path, "Getting process namespace");
    let ns = tokio::fs::read_link(&ns_path).await?;
    find_processes_in_mount_namespace(&ns).await
}

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
                    tracing::warn!(?err, "here");
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

pub fn ensure_mount_targets_exist() -> Result<()> {
    tracing::debug!("ensuring mount targets exist...");
    let mut res = runtime::makedirs_with_perms(SPFS_DIR, 0o777);
    if let Err(err) = res {
        return Err(err.wrap(format!("Failed to create {SPFS_DIR}")));
    }
    res = runtime::makedirs_with_perms(RUNTIME_DIR, 0o777);
    if let Err(err) = res {
        return Err(err.wrap(format!("Failed to create {RUNTIME_DIR}")));
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

pub fn mount_runtime(tmpfs_opts: Option<&str>) -> Result<()> {
    use nix::mount::{mount, MsFlags};

    tracing::debug!("mounting runtime...");
    let res = mount(
        NONE,
        RUNTIME_DIR,
        Some("tmpfs"),
        MsFlags::MS_NOEXEC,
        tmpfs_opts,
    );
    if let Err(err) = res {
        Err(Error::wrap_nix(
            err,
            format!("Failed to mount {RUNTIME_DIR}"),
        ))
    } else {
        Ok(())
    }
}

pub fn unmount_runtime() -> Result<()> {
    tracing::debug!("unmounting existing runtime...");
    let result = nix::mount::umount(RUNTIME_DIR);
    if let Err(err) = result {
        return Err(Error::wrap_nix(
            err,
            format!("Failed to unmount {RUNTIME_DIR}"),
        ));
    }
    Ok(())
}

pub fn setup_runtime() -> Result<()> {
    tracing::debug!("setting up runtime...");
    let mut result = runtime::makedirs_with_perms(RUNTIME_LOWER_DIR, 0o777);
    if let Err(err) = result {
        return Err(err.wrap(format!("Failed to create {RUNTIME_LOWER_DIR}")));
    }
    result = runtime::makedirs_with_perms(RUNTIME_UPPER_DIR, 0o777);
    if let Err(err) = result {
        return Err(err.wrap(format!("Failed to create {RUNTIME_UPPER_DIR}")));
    }
    result = runtime::makedirs_with_perms(RUNTIME_WORK_DIR, 0o777);
    if let Err(err) = result {
        return Err(err.wrap(format!("Failed to create {RUNTIME_WORK_DIR}")));
    }
    Ok(())
}

pub fn mask_files(manifest: &super::tracking::Manifest, owner: nix::unistd::Uid) -> Result<()> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    tracing::debug!("masking deleted files...");

    let nodes: Vec<_> = manifest.walk_abs(RUNTIME_UPPER_DIR).collect();
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

pub fn get_overlay_args<P: AsRef<Path>>(lowerdirs: impl IntoIterator<Item = P>) -> Result<String> {
    let mut args = format!("lowerdir={RUNTIME_LOWER_DIR}");
    for path in lowerdirs.into_iter() {
        args = format!("{args}:{}", path.as_ref().to_string_lossy());
    }

    args = format!("{args},upperdir={RUNTIME_UPPER_DIR},workdir={RUNTIME_WORK_DIR}");

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
    editable: bool,
    lowerdirs: impl IntoIterator<Item = P>,
) -> Result<()> {
    tracing::debug!("mounting the overlay filesystem...");
    let mut overlay_args = get_overlay_args(lowerdirs)?;
    if !editable {
        overlay_args = format!("ro,{overlay_args}");
    }
    tracing::debug!("/usr/bin/mount -t overlay -o {overlay_args} none {SPFS_DIR}");
    // for some reason, the overlay mount process creates a bad filesytem if the
    // mount command is called directly from this process. It may be some default
    // option or minor detail in how the standard mount command works - possibly related
    // to this process eventually dropping provilieges, but that is uncertain right now
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
