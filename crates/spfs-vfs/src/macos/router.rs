// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! PID-based filesystem router for macOS.
//!
//! Delegates fuser filesystem requests to per-process Mount instances by
//! walking the caller's process tree via sysctl.

use std::collections::HashSet;
use std::ffi::OsStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

use dashmap::DashMap;
use fuser::{
    Filesystem, ReplyAttr, ReplyData, ReplyDirectory, ReplyEntry, ReplyLseek, ReplyOpen,
    ReplyStatfs, Request,
};
use spfs::tracking::EnvSpec;
use tracing::instrument;

use super::mount::Mount;
use super::process::{ProcessWatcher, get_parent_pids_macos};

/// A PID-based filesystem router for macOS.
///
/// Routes fuser filesystem requests to per-process Mount instances by
/// walking the caller's process tree via sysctl. This allows different
/// process trees to see different filesystem views.
#[derive(Clone)]
pub struct Router {
    repos: Vec<Arc<spfs::storage::RepositoryHandle>>,

    // NEW: Runtime-indexed architecture
    runtime_mounts: Arc<DashMap<String, Arc<Mount>>>,
    pid_to_runtime: Arc<DashMap<u32, String>>,
    runtime_pids: Arc<DashMap<String, HashSet<u32>>>,

    // OLD: Will be removed in Phase 6
    routes: Arc<DashMap<u32, Arc<Mount>>>,

    default: Arc<Mount>,
    // Process watcher for cleanup
    process_watcher: Arc<tokio::sync::Mutex<ProcessWatcher>>,
    // Shutdown signal for cleanup task
    shutdown: Arc<AtomicBool>,
}

impl Router {
    /// Create a new router with the given repositories.
    pub async fn new(repos: Vec<Arc<spfs::storage::RepositoryHandle>>) -> spfs::Result<Self> {
        let default = Arc::new(Mount::empty()?);
        let process_watcher = ProcessWatcher::new()
            .map_err(|e| spfs::Error::String(format!("Failed to create process watcher: {}", e)))?;

        Ok(Self {
            repos,
            // NEW: Initialize runtime-indexed data structures
            runtime_mounts: Arc::new(DashMap::new()),
            pid_to_runtime: Arc::new(DashMap::new()),
            runtime_pids: Arc::new(DashMap::new()),
            // OLD: Keep existing for now
            routes: Arc::new(DashMap::new()),
            default,
            process_watcher: Arc::new(tokio::sync::Mutex::new(process_watcher)),
            shutdown: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Start the background cleanup task.
    ///
    /// This spawns a task that watches for process exits and cleans up
    /// orphaned mounts. Call this after creating the Router.
    pub fn start_cleanup_task(self: &Arc<Self>) {
        let router = Arc::clone(self);
        tokio::spawn(async move {
            router.cleanup_loop().await;
        });
    }

    async fn cleanup_loop(&self) {
        let cleanup_interval = std::time::Duration::from_secs(5);

        while !self.shutdown.load(Ordering::Relaxed) {
            // Wait for process exit or timeout
            let exited_pid = {
                let mut watcher = self.process_watcher.lock().await;
                match watcher.wait_for_exit(cleanup_interval) {
                    Ok(Some(pid)) => Some(pid),
                    Ok(None) => None, // Timeout - do periodic GC
                    Err(e) => {
                        tracing::warn!(error = %e, "process watcher error");
                        None
                    }
                }
            };

            // Handle specific exit
            if let Some(pid) = exited_pid {
                self.cleanup_mount(pid).await;
            }

            // Periodic garbage collection for any missed exits
            self.garbage_collect_dead_mounts().await;
        }

        tracing::debug!("cleanup loop exiting");
    }

    async fn cleanup_mount(&self, exited_pid: u32) {
        let current_pid = std::process::id();

        // NEW PATH: Unregister PID from runtime (decrement refcount)
        let (runtime_name, is_last) = match self.unregister_pid(exited_pid) {
            Some(result) => result,
            None => {
                tracing::info!(
                    %exited_pid,
                    current_pid,
                    "cleanup_mount: PID not registered in runtime index, checking legacy"
                );
                // OLD PATH: Try legacy cleanup
                if let Some((_, mount)) = self.routes.remove(&exited_pid) {
                    tracing::info!(%exited_pid, "Cleaning up legacy mount");
                    if mount.is_editable() {
                        if let Some(scratch) = mount.take_scratch() {
                            if let Err(e) = scratch.cleanup() {
                                tracing::warn!(%exited_pid, error = %e, "Failed to cleanup scratch");
                            }
                        }
                    }
                }
                return;
            }
        };

        let remaining_pids: Vec<u32> = self
            .runtime_pids
            .get(&runtime_name)
            .map(|set| set.iter().copied().collect())
            .unwrap_or_default();

        tracing::info!(
            %exited_pid,
            current_pid,
            %runtime_name,
            is_last,
            ?remaining_pids,
            "cleanup_mount: PID exited"
        );

        // If not the last PID, runtime is still in use
        if !is_last {
            tracing::debug!(%runtime_name, "Runtime still has active PIDs, keeping mount");
            // OLD PATH: Also cleanup from routes if present
            self.routes.remove(&exited_pid);
            return;
        }

        // Refcount reached 0 - check for descendants before cleanup
        tracing::info!(%runtime_name, "Refcount reached 0, checking for descendants");

        // Give recently-spawned children a moment to appear in the process table
        // This handles the case where a process exits immediately after spawning children
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let last_known_pids = vec![exited_pid];
        let descendants = self.check_descendant_pids(&last_known_pids);

        tracing::info!(
            %runtime_name,
            ?descendants,
            ?last_known_pids,
            "Descendant check complete (after 100ms delay)"
        );

        if !descendants.is_empty() {
            tracing::warn!(
                %runtime_name,
                ?descendants,
                "Descendants still alive at refcount 0, registering them"
            );

            // Register descendants to prevent premature cleanup
            for desc_pid in descendants {
                if let Err(e) = self.register_pid(desc_pid, runtime_name.clone()).await {
                    tracing::warn!(%desc_pid, %e, "Failed to register descendant");
                }
            }
            return; // Don't cleanup - descendants now registered
        }

        // Check if runtime is durable (TODO: implement in Phase 7)
        // For now, assume non-durable
        let is_durable = false;

        if is_durable {
            tracing::info!(%runtime_name, "Durable runtime at refcount 0, keeping mount");
            return;
        }

        // Safe to cleanup - non-durable runtime with no PIDs
        tracing::info!(%runtime_name, "Non-durable runtime at refcount 0, cleaning up");

        if let Some((_, mount)) = self.runtime_mounts.remove(&runtime_name) {
            if mount.is_editable() {
                if let Some(scratch) = mount.take_scratch() {
                    if let Err(e) = scratch.cleanup() {
                        tracing::warn!(%runtime_name, error = %e, "Failed to cleanup scratch");
                    }
                }
            }
        }

        self.runtime_pids.remove(&runtime_name);

        // OLD PATH: Also cleanup from routes
        self.routes.remove(&exited_pid);

        tracing::info!(%runtime_name, "Cleanup complete - mount and scratch removed");
    }

    async fn garbage_collect_dead_mounts(&self) {
        // NEW PATH: Collect all registered PIDs from runtime index
        let runtime_pids: Vec<u32> = self.pid_to_runtime.iter().map(|e| *e.key()).collect();

        for pid in runtime_pids {
            if !ProcessWatcher::is_process_alive(pid) {
                tracing::debug!(%pid, "Found dead PID during GC (runtime index)");
                self.cleanup_mount(pid).await;
            }
        }

        // OLD PATH: Also check legacy routes
        let legacy_pids: Vec<u32> = self.routes.iter().map(|r| *r.key()).collect();

        for pid in legacy_pids {
            if !ProcessWatcher::is_process_alive(pid) {
                tracing::debug!(%pid, "Found dead PID during GC (legacy routes)");
                // cleanup_mount will handle removal from routes
                self.cleanup_mount(pid).await;
            }
        }
    }

    /// Signal the cleanup task to stop.
    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }

    /// Get an iterator over all active mounts.
    ///
    /// Returns (pid, mount) pairs for all registered mounts.
    pub fn iter_mounts(&self) -> Vec<(u32, Arc<Mount>)> {
        self.routes
            .iter()
            .map(|entry| (*entry.key(), Arc::clone(entry.value())))
            .collect()
    }

    /// Mount an environment for a specific process tree (read-only).
    ///
    /// The given PID becomes the root of the process tree that will
    /// see the specified environment.
    #[instrument(skip(self, repos))]
    pub async fn mount(
        &self,
        root_pid: u32,
        env_spec: EnvSpec,
        repos: Option<Vec<Arc<spfs::storage::RepositoryHandle>>>,
    ) -> spfs::Result<()> {
        self.mount_internal(root_pid, env_spec, false, None, repos)
            .await
    }

    /// Mount an editable environment for a specific process tree.
    ///
    /// The given PID becomes the root of the process tree that will
    /// see the specified environment with write support via scratch directory.
    #[instrument(skip(self, repos))]
    pub async fn mount_editable(
        &self,
        root_pid: u32,
        env_spec: EnvSpec,
        runtime_name: &str,
        repos: Option<Vec<Arc<spfs::storage::RepositoryHandle>>>,
    ) -> spfs::Result<()> {
        self.mount_internal(root_pid, env_spec, true, Some(runtime_name), repos)
            .await
    }

    async fn mount_internal(
        &self,
        root_pid: u32,
        env_spec: EnvSpec,
        editable: bool,
        runtime_name: Option<&str>,
        repos: Option<Vec<Arc<spfs::storage::RepositoryHandle>>>,
    ) -> spfs::Result<()> {
        let current_pid = std::process::id();
        tracing::info!(
            %root_pid,
            current_pid,
            %env_spec,
            %editable,
            ?runtime_name,
            "mount_internal: called"
        );

        // Determine runtime name
        let runtime_name = if editable {
            runtime_name
                .map(String::from)
                .unwrap_or_else(|| format!("runtime-{}", root_pid))
        } else {
            // Non-editable mounts still use PID-based naming for now
            format!("readonly-{}", root_pid)
        };

        // Check if runtime already has a mount (NEW PATH)
        let mount_exists = self.runtime_mounts.contains_key(&runtime_name);
        let current_pids: Vec<u32> = self
            .runtime_pids
            .get(&runtime_name)
            .map(|set| set.iter().copied().collect())
            .unwrap_or_default();

        tracing::info!(
            %root_pid,
            %runtime_name,
            mount_exists,
            ?current_pids,
            "mount_internal: checking existing mount"
        );

        if let Some(existing_mount) = self.runtime_mounts.get(&runtime_name) {
            // Check if the env_spec matches
            let existing_env_spec = existing_mount.env_spec();
            let new_env_spec_str = env_spec.to_string();

            if existing_env_spec == new_env_spec_str {
                tracing::info!(
                    %root_pid,
                    %runtime_name,
                    ?current_pids,
                    "Runtime already mounted with same env_spec"
                );

                // Check if this PID is already registered
                let already_registered = current_pids.contains(&root_pid);

                if !already_registered {
                    tracing::info!(%root_pid, %runtime_name, "Registering additional PID");
                    self.register_pid(root_pid, runtime_name.clone()).await?;

                    // Also populate OLD path for compatibility
                    self.routes
                        .insert(root_pid, Arc::clone(existing_mount.value()));
                    let mut watcher = self.process_watcher.lock().await;
                    if let Err(e) = watcher.watch(root_pid) {
                        tracing::warn!(%root_pid, error = %e, "failed to watch process");
                    }
                } else {
                    tracing::info!(%root_pid, %runtime_name, "PID already registered - this is a re-mount, no action needed");
                }

                return Ok(());
            } else {
                // env_spec changed - need to update mount with new manifest while preserving scratch
                tracing::info!(
                    %root_pid,
                    %runtime_name,
                    ?current_pids,
                    existing_env_spec,
                    new_env_spec = %new_env_spec_str,
                    "Runtime already mounted but env_spec changed - updating mount with new manifest"
                );

                // Step 1: Extract scratch from existing mount
                let old_mount = existing_mount.clone(); // Clone the Arc
                drop(existing_mount); // Drop the reference to allow mutation later

                let extracted_scratch = old_mount.take_scratch();

                tracing::debug!(
                    %runtime_name,
                    has_scratch = extracted_scratch.is_some(),
                    "Extracted scratch directory from existing mount"
                );

                // Step 2: Compute new manifest from updated env_spec
                let repos = repos.unwrap_or_else(|| self.repos.clone());
                let mut manifest = Err(spfs::Error::UnknownReference(env_spec.to_string()));
                for repo in &repos {
                    manifest = spfs::compute_environment_manifest(&env_spec, repo).await;
                    if manifest.is_ok() {
                        break;
                    }
                }
                let manifest = manifest?;

                tracing::debug!(
                    %runtime_name,
                    "Computed new manifest from updated env_spec"
                );

                // Step 3: Create new mount with new manifest + reused scratch
                let env_spec_str = env_spec.to_string();
                let new_mount = if editable {
                    if let Some(scratch) = extracted_scratch {
                        // Reuse existing scratch - create editable mount without scratch first
                        let mount = Arc::new(Mount::new_editable_without_scratch(
                            tokio::runtime::Handle::current(),
                            repos.clone(),
                            manifest,
                            &runtime_name,
                            env_spec_str,
                        )?);

                        // Install the extracted scratch
                        mount.set_scratch(scratch)?;

                        tracing::debug!(
                            %runtime_name,
                            "Created new mount with reused scratch directory"
                        );

                        mount
                    } else {
                        // No scratch to reuse, create fresh editable mount
                        Arc::new(Mount::new_editable_with_env_spec(
                            tokio::runtime::Handle::current(),
                            repos.clone(),
                            manifest,
                            &runtime_name,
                            env_spec_str,
                        )?)
                    }
                } else {
                    // Read-only mount
                    Arc::new(Mount::new_with_env_spec(
                        tokio::runtime::Handle::current(),
                        repos.clone(),
                        manifest,
                        env_spec_str,
                    )?)
                };

                // Step 4: Replace mount in registry
                self.runtime_mounts
                    .insert(runtime_name.clone(), Arc::clone(&new_mount));

                tracing::info!(
                    %runtime_name,
                    %root_pid,
                    "Mount updated with new manifest and scratch preserved"
                );

                // Register PID if needed
                if !current_pids.contains(&root_pid) {
                    self.register_pid(root_pid, runtime_name.clone()).await?;
                }

                // Update OLD path for compatibility
                self.routes.insert(root_pid, Arc::clone(&new_mount));

                return Ok(());
            }
        }

        // Create new mount (no change)
        let repos = repos.unwrap_or_else(|| self.repos.clone());
        let mut manifest = Err(spfs::Error::UnknownReference(env_spec.to_string()));
        for repo in &repos {
            manifest = spfs::compute_environment_manifest(&env_spec, repo).await;
            if manifest.is_ok() {
                break;
            }
        }
        let manifest = manifest?;

        let env_spec_str = env_spec.to_string();
        let mount = if editable {
            Arc::new(Mount::new_editable_with_env_spec(
                tokio::runtime::Handle::current(),
                repos,
                manifest,
                &runtime_name,
                env_spec_str,
            )?)
        } else {
            Arc::new(Mount::new_with_env_spec(
                tokio::runtime::Handle::current(),
                repos,
                manifest,
                env_spec_str,
            )?)
        };

        // NEW PATH: Insert into runtime_mounts
        self.runtime_mounts
            .insert(runtime_name.clone(), Arc::clone(&mount));
        self.register_pid(root_pid, runtime_name.clone()).await?;

        // OLD PATH: Also insert into routes for compatibility
        match self.routes.entry(root_pid) {
            dashmap::mapref::entry::Entry::Occupied(_) => {
                // Should not happen since we checked runtime_mounts above
                return Err(spfs::Error::RuntimeExists(root_pid.to_string()));
            }
            dashmap::mapref::entry::Entry::Vacant(entry) => {
                entry.insert(Arc::clone(&mount));
            }
        }

        let mut watcher = self.process_watcher.lock().await;
        if let Err(e) = watcher.watch(root_pid) {
            tracing::warn!(%root_pid, error = %e, "failed to watch process for cleanup");
        }

        tracing::info!(%root_pid, %runtime_name, "Mount created and registered");
        Ok(())
    }

    /// Unmount an environment for a specific process tree.
    ///
    /// Returns true if the PID had an active mount.
    ///
    /// Note: On macOS, explicit unmount is a NO-OP for the runtime-indexed path.
    /// The PID stays registered and the mount persists until the process exits.
    /// This is necessary because:
    /// 1. Child processes need to find the mount via ancestry lookup
    /// 2. Unmount/remount patterns need to preserve scratch state
    /// 3. Only ProcessWatcher cleanup should actually remove mounts
    #[instrument(skip(self))]
    pub fn unmount(&self, root_pid: u32) -> bool {
        let current_pid = std::process::id();

        // Check if this PID is registered in the runtime index
        let is_registered = self.pid_to_runtime.contains_key(&root_pid);

        if is_registered {
            if let Some(runtime_name_ref) = self.pid_to_runtime.get(&root_pid) {
                let runtime_name = runtime_name_ref.value().clone();
                tracing::info!(
                    %root_pid,
                    current_pid,
                    %runtime_name,
                    "Unmount requested (explicit) - keeping PID registered for child process access"
                );
            }

            // NEW PATH: Do NOT unregister! Keep PID registered so:
            // 1. Children can still find the mount via ancestry
            // 2. Mount persists for remount
            // Actual cleanup happens when process exits (ProcessWatcher)

            // OLD PATH: Remove from legacy routes for consistency
            self.routes.remove(&root_pid);

            true
        } else {
            // OLD PATH: Fallback to legacy unmount
            tracing::debug!(
                %root_pid,
                current_pid,
                "Unmount requested but PID not registered in runtime index (legacy path)"
            );
            self.routes.remove(&root_pid).is_some()
        }
    }

    /// Register a PID for a runtime.
    ///
    /// Adds the PID to runtime_pids (refcount++) and creates bidirectional mapping.
    /// Starts watching the PID for exit via ProcessWatcher.
    async fn register_pid(&self, pid: u32, runtime_name: String) -> spfs::Result<()> {
        tracing::debug!(%pid, %runtime_name, "Registering PID for runtime");

        // Add to both mappings atomically
        self.pid_to_runtime.insert(pid, runtime_name.clone());
        self.runtime_pids
            .entry(runtime_name.clone())
            .or_insert_with(HashSet::new)
            .insert(pid);

        // Watch this PID for exit
        let mut watcher = self.process_watcher.lock().await;
        if let Err(e) = watcher.watch(pid) {
            tracing::warn!(%pid, %e, "Failed to watch PID - will rely on GC");
        }

        let refcount = self
            .runtime_pids
            .get(&runtime_name)
            .map_or(0, |pids| pids.len());
        tracing::debug!(%pid, %runtime_name, %refcount, "PID registered");

        Ok(())
    }

    /// Unregister a PID from a runtime.
    ///
    /// Removes the PID from runtime_pids (refcount--) and removes mappings.
    /// Returns the runtime name and whether this was the last PID.
    fn unregister_pid(&self, pid: u32) -> Option<(String, bool)> {
        // Remove from pid_to_runtime mapping
        let runtime_name = self.pid_to_runtime.remove(&pid)?;
        let runtime_name = runtime_name.1; // Extract value from (key, value) tuple

        // Remove from runtime_pids mapping (refcount--)
        let is_last = if let Some(mut pids) = self.runtime_pids.get_mut(&runtime_name) {
            pids.remove(&pid);
            let refcount = pids.len();
            tracing::debug!(%pid, %runtime_name, %refcount, "PID unregistered");
            refcount == 0
        } else {
            true // Runtime not found, treat as last
        };

        Some((runtime_name, is_last))
    }

    /// Get the mount for a runtime by name.
    ///
    /// Returns None if the runtime is not mounted.
    fn get_mount_for_runtime(&self, runtime_name: &str) -> Option<Arc<Mount>> {
        self.runtime_mounts
            .get(runtime_name)
            .map(|m| Arc::clone(m.value()))
    }

    /// Check for descendant processes of the given PIDs.
    ///
    /// Returns a HashSet of all living descendant PIDs found.
    fn check_descendant_pids(&self, parent_pids: &[u32]) -> HashSet<u32> {
        let mut descendants = HashSet::new();

        for &pid in parent_pids {
            // Use the process module to find descendants
            match super::process::get_descendant_pids(pid as i32) {
                Ok(desc_pids) => {
                    tracing::debug!(%pid, ?desc_pids, "Found descendants");
                    descendants.extend(desc_pids.into_iter().map(|p| p as u32));
                }
                Err(e) => {
                    tracing::debug!(%pid, error = %e, "Failed to get descendants");
                }
            }
        }

        tracing::debug!(?parent_pids, ?descendants, "Descendant scan complete");
        descendants
    }

    fn get_mount_for_pid(&self, caller_pid: u32) -> Arc<Mount> {
        // Get process ancestry
        let ancestry = match get_parent_pids_macos(Some(caller_pid as i32)) {
            Ok(ancestry) => ancestry,
            Err(e) => {
                tracing::error!("get_parent_pids_macos failed for PID {}: {}", caller_pid, e);
                return Arc::clone(&self.default);
            }
        };

        // NEW PATH: Check each ancestor in pid_to_runtime (O(1) per ancestor)
        for ancestor_pid in &ancestry {
            let pid_u32 = *ancestor_pid as u32;

            // Try new runtime-indexed lookup
            if let Some(runtime_name_ref) = self.pid_to_runtime.get(&pid_u32) {
                let runtime_name = runtime_name_ref.value().clone();
                drop(runtime_name_ref); // Release lock

                if let Some(mount) = self.runtime_mounts.get(&runtime_name) {
                    tracing::trace!(
                        caller_pid,
                        ancestor_pid = pid_u32,
                        %runtime_name,
                        "Found mount via runtime index"
                    );
                    return Arc::clone(mount.value());
                }
            }
        }

        // OLD PATH: Fallback to PID-indexed lookup for backward compatibility
        for ancestor_pid in &ancestry {
            let pid_u32 = *ancestor_pid as u32;
            if let Some(mount) = self.routes.get(&pid_u32) {
                tracing::debug!(
                    caller_pid,
                    ancestor_pid = pid_u32,
                    "Found mount via legacy PID index (compatibility mode)"
                );
                return Arc::clone(mount.value());
            }
        }

        // No mount found - log debug and use default
        tracing::debug!(
            caller_pid,
            ?ancestry,
            registered_runtimes = ?self.runtime_mounts.iter().map(|e| e.key().clone()).collect::<Vec<_>>(),
            "No mount found for PID or its ancestors"
        );
        Arc::clone(&self.default)
    }
}

impl Filesystem for Router {
    fn init(
        &mut self,
        _req: &Request<'_>,
        _config: &mut fuser::KernelConfig,
    ) -> Result<(), libc::c_int> {
        tracing::info!("macFUSE filesystem initialized");
        Ok(())
    }

    fn destroy(&mut self) {
        tracing::info!("macFUSE filesystem destroyed");
    }

    #[instrument(skip_all, fields(pid = req.pid()))]
    fn lookup(&mut self, req: &Request<'_>, parent: u64, name: &OsStr, reply: ReplyEntry) {
        self.get_mount_for_pid(req.pid())
            .lookup(parent, name, reply);
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
    fn read(
        &mut self,
        req: &Request<'_>,
        ino: u64,
        fh: u64,
        offset: i64,
        size: u32,
        flags: i32,
        lock_owner: Option<u64>,
        reply: ReplyData,
    ) {
        self.get_mount_for_pid(req.pid())
            .read(ino, fh, offset, size, flags, lock_owner, reply);
    }

    #[instrument(skip_all, fields(pid = req.pid()))]
    fn release(
        &mut self,
        req: &Request<'_>,
        ino: u64,
        fh: u64,
        flags: i32,
        lock_owner: Option<u64>,
        flush: bool,
        reply: fuser::ReplyEmpty,
    ) {
        self.get_mount_for_pid(req.pid())
            .release(ino, fh, flags, lock_owner, flush, reply);
    }

    #[instrument(skip_all, fields(pid = req.pid()))]
    fn opendir(&mut self, req: &Request<'_>, ino: u64, flags: i32, reply: ReplyOpen) {
        self.get_mount_for_pid(req.pid()).opendir(ino, flags, reply);
    }

    #[instrument(skip_all, fields(pid = req.pid()))]
    fn readdir(
        &mut self,
        req: &Request<'_>,
        ino: u64,
        fh: u64,
        offset: i64,
        reply: ReplyDirectory,
    ) {
        self.get_mount_for_pid(req.pid())
            .readdir(ino, fh, offset, reply);
    }

    #[instrument(skip_all, fields(pid = req.pid()))]
    fn releasedir(
        &mut self,
        req: &Request<'_>,
        ino: u64,
        fh: u64,
        flags: i32,
        reply: fuser::ReplyEmpty,
    ) {
        self.get_mount_for_pid(req.pid())
            .releasedir(ino, fh, flags, reply);
    }

    #[instrument(skip_all, fields(pid = req.pid()))]
    fn statfs(&mut self, req: &Request<'_>, ino: u64, reply: ReplyStatfs) {
        self.get_mount_for_pid(req.pid()).statfs(ino, reply);
    }

    #[instrument(skip_all, fields(pid = req.pid()))]
    fn lseek(
        &mut self,
        req: &Request<'_>,
        ino: u64,
        fh: u64,
        offset: i64,
        whence: i32,
        reply: ReplyLseek,
    ) {
        self.get_mount_for_pid(req.pid())
            .lseek(ino, fh, offset, whence, reply);
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
        self.get_mount_for_pid(req.pid()).write(
            ino,
            fh,
            offset,
            data,
            write_flags,
            flags,
            lock_owner,
            reply,
        );
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
        self.get_mount_for_pid(req.pid())
            .unlink(parent, name, reply);
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
        assert!(router.routes.is_empty());
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
