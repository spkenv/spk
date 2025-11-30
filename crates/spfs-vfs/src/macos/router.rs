// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! PID-based filesystem router for macOS.
//!
//! Delegates fuser filesystem requests to per-process Mount instances by
//! walking the caller's process tree via libproc.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::sync::{Arc, RwLock};
use std::time::SystemTime;

use fuser::{Filesystem, ReplyAttr, ReplyData, ReplyDirectory, ReplyEntry, ReplyLseek, ReplyOpen, ReplyStatfs, Request};
use spfs::tracking::EnvSpec;
use tracing::instrument;

use super::mount::Mount;
use super::process::get_parent_pids_macos;

/// A PID-based filesystem router for macOS.
///
/// Routes fuser filesystem requests to per-process Mount instances by
/// walking the caller's process tree via libproc. This allows different
/// process trees to see different filesystem views.
#[derive(Clone)]
pub struct Router {
    repos: Vec<Arc<spfs::storage::RepositoryHandle>>,
    routes: Arc<RwLock<HashMap<u32, Arc<Mount>>>>,
    default: Arc<Mount>,
}

impl Router {
    /// Create a new router with the given repositories.
    pub async fn new(repos: Vec<Arc<spfs::storage::RepositoryHandle>>) -> spfs::Result<Self> {
        let default = Arc::new(Mount::empty()?);
        Ok(Self {
            repos,
            routes: Arc::new(RwLock::new(HashMap::new())),
            default,
        })
    }

    /// Mount an environment for a specific process tree (read-only).
    ///
    /// The given PID becomes the root of the process tree that will
    /// see the specified environment.
    #[instrument(skip(self))]
    pub async fn mount(&self, root_pid: u32, env_spec: EnvSpec) -> spfs::Result<()> {
        self.mount_internal(root_pid, env_spec, false, None).await
    }

    /// Mount an editable environment for a specific process tree.
    ///
    /// The given PID becomes the root of the process tree that will
    /// see the specified environment with write support via scratch directory.
    #[instrument(skip(self))]
    pub async fn mount_editable(
        &self,
        root_pid: u32,
        env_spec: EnvSpec,
        runtime_name: &str,
    ) -> spfs::Result<()> {
        self.mount_internal(root_pid, env_spec, true, Some(runtime_name)).await
    }

    async fn mount_internal(
        &self,
        root_pid: u32,
        env_spec: EnvSpec,
        editable: bool,
        runtime_name: Option<&str>,
    ) -> spfs::Result<()> {
        tracing::debug!(%root_pid, %env_spec, %editable, "mount request");
        let mut manifest = Err(spfs::Error::UnknownReference(env_spec.to_string()));
        for repo in &self.repos {
            manifest = spfs::compute_environment_manifest(&env_spec, repo).await;
            if manifest.is_ok() {
                break;
            }
        }
        let manifest = manifest?;

        let mount = if editable {
            let default_name = format!("runtime-{}", root_pid);
            let name = runtime_name.unwrap_or(&default_name);
            Arc::new(Mount::new_editable(
                tokio::runtime::Handle::current(),
                self.repos.clone(),
                manifest,
                name,
            )?)
        } else {
            Arc::new(Mount::new(
                tokio::runtime::Handle::current(),
                self.repos.clone(),
                manifest,
            )?)
        };

        let mut routes = self.routes.write().expect("routes lock");
        if routes.contains_key(&root_pid) {
            return Err(spfs::Error::RuntimeExists(root_pid.to_string()));
        }
        routes.insert(root_pid, mount);
        Ok(())
    }

    /// Unmount an environment for a specific process tree.
    ///
    /// Returns true if the PID had an active mount.
    #[instrument(skip(self))]
    pub fn unmount(&self, root_pid: u32) -> bool {
        let mut routes = self.routes.write().expect("routes lock");
        routes.remove(&root_pid).is_some()
    }

    fn get_mount_for_pid(&self, caller_pid: u32) -> Arc<Mount> {
        let ancestry = get_parent_pids_macos(Some(caller_pid as i32)).unwrap_or_else(|_| vec![caller_pid as i32]);
        let routes = self.routes.read().expect("routes lock");
        for pid in ancestry {
            if let Some(mount) = routes.get(&(pid as u32)) {
                return Arc::clone(mount);
            }
        }
        Arc::clone(&self.default)
    }
}

impl Filesystem for Router {
    fn init(&mut self, _req: &Request<'_>, _config: &mut fuser::KernelConfig) -> Result<(), libc::c_int> {
        tracing::info!("macFUSE filesystem initialized");
        Ok(())
    }

    fn destroy(&mut self) {
        tracing::info!("macFUSE filesystem destroyed");
    }

    #[instrument(skip_all, fields(pid = req.pid()))]
    fn lookup(&mut self, req: &Request<'_>, parent: u64, name: &OsStr, reply: ReplyEntry) {
        self.get_mount_for_pid(req.pid()).lookup(parent, name, reply);
    }

    #[instrument(skip_all, fields(pid = req.pid()))]
    fn getattr(&mut self, req: &Request<'_>, ino: u64, _fh: Option<u64>, reply: ReplyAttr) {
        self.get_mount_for_pid(req.pid()).getattr(ino, reply);
    }

    #[instrument(skip_all, fields(pid = req.pid()))]
    fn readlink(&mut self, req: &Request<'_>, ino: u64, reply: ReplyData) {
        self.get_mount_for_pid(req.pid()).readlink(ino, reply);
    }

    #[instrument(skip_all, fields(pid = req.pid()))]
    fn open(&mut self, req: &Request<'_>, ino: u64, flags: i32, reply: ReplyOpen) {
        self.get_mount_for_pid(req.pid()).open(ino, flags, reply);
    }

    #[allow(clippy::too_many_arguments)]
    #[instrument(skip_all, fields(pid = req.pid()))]
    fn read(&mut self, req: &Request<'_>, ino: u64, fh: u64, offset: i64, size: u32, flags: i32, lock_owner: Option<u64>, reply: ReplyData) {
        self.get_mount_for_pid(req.pid())
            .read(ino, fh, offset, size, flags, lock_owner, reply);
    }

    #[instrument(skip_all, fields(pid = req.pid()))]
    fn release(&mut self, req: &Request<'_>, ino: u64, fh: u64, flags: i32, lock_owner: Option<u64>, flush: bool, reply: fuser::ReplyEmpty) {
        self.get_mount_for_pid(req.pid())
            .release(ino, fh, flags, lock_owner, flush, reply);
    }

    #[instrument(skip_all, fields(pid = req.pid()))]
    fn opendir(&mut self, req: &Request<'_>, ino: u64, flags: i32, reply: ReplyOpen) {
        self.get_mount_for_pid(req.pid()).opendir(ino, flags, reply);
    }

    #[instrument(skip_all, fields(pid = req.pid()))]
    fn readdir(&mut self, req: &Request<'_>, ino: u64, fh: u64, offset: i64, reply: ReplyDirectory) {
        self.get_mount_for_pid(req.pid()).readdir(ino, fh, offset, reply);
    }

    #[instrument(skip_all, fields(pid = req.pid()))]
    fn releasedir(&mut self, req: &Request<'_>, ino: u64, fh: u64, flags: i32, reply: fuser::ReplyEmpty) {
        self.get_mount_for_pid(req.pid()).releasedir(ino, fh, flags, reply);
    }

    #[instrument(skip_all, fields(pid = req.pid()))]
    fn statfs(&mut self, req: &Request<'_>, ino: u64, reply: ReplyStatfs) {
        self.get_mount_for_pid(req.pid()).statfs(ino, reply);
    }

    #[instrument(skip_all, fields(pid = req.pid()))]
    fn lseek(&mut self, req: &Request<'_>, ino: u64, fh: u64, offset: i64, whence: i32, reply: ReplyLseek) {
        self.get_mount_for_pid(req.pid()).lseek(ino, fh, offset, whence, reply);
    }

    #[instrument(skip_all, fields(pid = req.pid()))]
    fn access(&mut self, req: &Request<'_>, ino: u64, mask: i32, reply: fuser::ReplyEmpty) {
        self.get_mount_for_pid(req.pid()).access(ino, mask, reply);
    }

    // ========================================================================
    // Write operations
    // ========================================================================

    #[allow(clippy::too_many_arguments)]
    #[instrument(skip_all, fields(pid = req.pid()))]
    fn write(
        &mut self,
        req: &Request<'_>,
        ino: u64,
        fh: u64,
        offset: i64,
        data: &[u8],
        write_flags: u32,
        flags: i32,
        lock_owner: Option<u64>,
        reply: fuser::ReplyWrite,
    ) {
        self.get_mount_for_pid(req.pid())
            .write(ino, fh, offset, data, write_flags, flags, lock_owner, reply);
    }

    #[allow(clippy::too_many_arguments)]
    #[instrument(skip_all, fields(pid = req.pid()))]
    fn create(
        &mut self,
        req: &Request<'_>,
        parent: u64,
        name: &OsStr,
        mode: u32,
        umask: u32,
        flags: i32,
        reply: fuser::ReplyCreate,
    ) {
        self.get_mount_for_pid(req.pid())
            .create(parent, name, mode, umask, flags, reply);
    }

    #[instrument(skip_all, fields(pid = req.pid()))]
    fn unlink(&mut self, req: &Request<'_>, parent: u64, name: &OsStr, reply: fuser::ReplyEmpty) {
        self.get_mount_for_pid(req.pid()).unlink(parent, name, reply);
    }

    #[instrument(skip_all, fields(pid = req.pid()))]
    fn mkdir(
        &mut self,
        req: &Request<'_>,
        parent: u64,
        name: &OsStr,
        mode: u32,
        umask: u32,
        reply: ReplyEntry,
    ) {
        self.get_mount_for_pid(req.pid())
            .mkdir(parent, name, mode, umask, reply);
    }

    #[instrument(skip_all, fields(pid = req.pid()))]
    fn rmdir(&mut self, req: &Request<'_>, parent: u64, name: &OsStr, reply: fuser::ReplyEmpty) {
        self.get_mount_for_pid(req.pid()).rmdir(parent, name, reply);
    }

    #[allow(clippy::too_many_arguments)]
    #[instrument(skip_all, fields(pid = req.pid()))]
    fn rename(
        &mut self,
        req: &Request<'_>,
        parent: u64,
        name: &OsStr,
        newparent: u64,
        newname: &OsStr,
        flags: u32,
        reply: fuser::ReplyEmpty,
    ) {
        self.get_mount_for_pid(req.pid())
            .rename(parent, name, newparent, newname, flags, reply);
    }

    #[allow(clippy::too_many_arguments)]
    #[instrument(skip_all, fields(pid = req.pid()))]
    fn setattr(
        &mut self,
        req: &Request<'_>,
        ino: u64,
        mode: Option<u32>,
        uid: Option<u32>,
        gid: Option<u32>,
        size: Option<u64>,
        atime: Option<fuser::TimeOrNow>,
        mtime: Option<fuser::TimeOrNow>,
        ctime: Option<SystemTime>,
        fh: Option<u64>,
        crtime: Option<SystemTime>,
        chgtime: Option<SystemTime>,
        bkuptime: Option<SystemTime>,
        flags: Option<u32>,
        reply: ReplyAttr,
    ) {
        self.get_mount_for_pid(req.pid()).setattr(
            ino, mode, uid, gid, size, atime, mtime, ctime, fh, crtime, chgtime, bkuptime, flags,
            reply,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn router_new_creates_default_mount() {
        let router = Router::new(Vec::new()).await.unwrap();
        // Default mount should exist and be empty
        assert!(router.routes.read().unwrap().is_empty());
    }

    #[tokio::test]
    async fn router_unmount_nonexistent_returns_false() {
        let router = Router::new(Vec::new()).await.unwrap();
        assert!(!router.unmount(12345));
    }

    #[tokio::test]
    async fn router_default_mount_is_not_editable() {
        let router = Router::new(Vec::new()).await.unwrap();
        // The default mount should be read-only
        assert!(!router.default.is_editable());
    }

    #[tokio::test]
    async fn router_get_mount_returns_default_for_unknown_pid() {
        let router = Router::new(Vec::new()).await.unwrap();
        // An unknown PID should get the default mount
        let mount = router.get_mount_for_pid(99999);
        assert!(!mount.is_editable());
    }
}
