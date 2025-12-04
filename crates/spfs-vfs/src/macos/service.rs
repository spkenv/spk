// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! macOS FUSE service implementation
//!
//! This provides a long-running service that mounts a FUSE filesystem
//! and routes requests based on caller PID to different manifests.

use std::path::PathBuf;
use std::sync::Arc;

use fuser::MountOption;
use spfs::storage::RepositoryHandle;
use spfs::tracking::EnvSpec;
use tokio::sync::{Mutex, mpsc};
use tonic::{Request, Response, Status};

use super::Config;
use super::router::Router;
use crate::Error;
use crate::proto::vfs_service_server::VfsService;
use crate::proto::{
    MountInfo,
    MountRequest,
    MountResponse,
    ShutdownRequest,
    ShutdownResponse,
    StatusRequest,
    StatusResponse,
    UnmountRequest,
    UnmountResponse,
};

/// A macOS FUSE filesystem service
///
/// This service manages the macFUSE mount and provides
/// filesystem operations backed by SPFS repositories.
/// It uses a PID-based router to provide different filesystem
/// views to different process trees.
///
/// The service is designed to be wrapped in an Arc for sharing
/// across async tasks and the gRPC server.
pub struct Service {
    config: Config,
    repos: Vec<Arc<RepositoryHandle>>,
    router: Mutex<Option<Router>>,
    shutdown_tx: Mutex<Option<mpsc::Sender<()>>>,
}

impl Service {
    /// Create a new macOS FUSE service with the given configuration
    pub async fn new(config: Config) -> Result<Arc<Self>, Error> {
        // Clean up any orphaned scratch directories from previous runs
        cleanup_orphaned_scratch_directories().await;

        // Open the configured repositories
        let mut repos = Vec::new();

        // Open local repository using the config
        let spfs_config = spfs::get_config()
            .map_err(|e| Error::String(format!("Failed to load spfs config: {e}")))?;
        let local = spfs_config
            .get_local_repository_handle()
            .await
            .map_err(|e| Error::String(format!("Failed to open local repository: {e}")))?;
        repos.push(Arc::new(local));

        // Open configured remotes
        for remote in &config.remotes {
            match spfs::open_repository(remote).await {
                Ok(repo) => repos.push(Arc::new(repo)),
                Err(e) => {
                    tracing::warn!(%remote, err = ?e, "Failed to open remote repository");
                }
            }
        }

        Ok(Arc::new(Self {
            config,
            repos,
            router: Mutex::new(None),
            shutdown_tx: Mutex::new(None),
        }))
    }

    /// Start the FUSE mount in a background thread.
    ///
    /// This creates the router and spawns a blocking thread to run
    /// the fuser session. Returns immediately after starting.
    pub async fn start_mount(self: &mut Arc<Self>) -> Result<(), Error> {
        let service = Arc::clone(self);

        // Create shutdown channel
        let (shutdown_tx, _shutdown_rx) = mpsc::channel::<()>(1);

        // Create the router
        let repos = service.repos.clone();
        let router = Router::new(repos)
            .await
            .map_err(|e| Error::String(format!("Failed to create router: {e}")))?;

        // Store router in service and start cleanup task
        let router_arc = Arc::new(router.clone());
        router_arc.start_cleanup_task();
        *service.router.lock().await = Some(router.clone());
        *service.shutdown_tx.lock().await = Some(shutdown_tx);

        // Build mount options
        let mut options: Vec<MountOption> = service.config.mount_options.iter().cloned().collect();
        options.push(MountOption::RO); // read-only
        options.push(MountOption::FSName("spfs".to_string()));
        options.push(MountOption::Subtype("spfs".to_string()));
        // Note: AllowOther requires root or special macFUSE configuration
        // For single-user development, we skip it. If needed, run as root or
        // configure macFUSE to allow user_allow_other.

        let mount_path = service.config.mountpoint.clone();

        // Ensure mount point exists
        if !mount_path.exists() {
            std::fs::create_dir_all(&mount_path).map_err(|e| {
                Error::String(format!(
                    "Failed to create mount point {}: {e}",
                    mount_path.display()
                ))
            })?;
        }

        tracing::info!(?mount_path, "Starting macFUSE mount");

        // Spawn the FUSE mount in a blocking thread
        std::thread::spawn(move || {
            // Create a separate runtime for this thread since we may need async
            let result = fuser::mount2(router, &mount_path, &options);
            if let Err(e) = result {
                tracing::error!(err = ?e, "FUSE mount error");
            }
            tracing::info!("FUSE mount thread exiting");
        });

        // Give the mount a moment to start
        std::thread::sleep(std::time::Duration::from_millis(100));

        Ok(())
    }

    /// Stop the FUSE filesystem service
    ///
    /// This unmounts the filesystem. Note that this requires the mount
    /// to be idle (no open files or current directories).
    pub async fn stop(&self) -> Result<(), Error> {
        // Signal router to shutdown cleanup task
        if let Some(router) = self.router.lock().await.as_ref() {
            router.shutdown();
        }

        // Send shutdown signal
        if let Some(tx) = self.shutdown_tx.lock().await.take() {
            let _ = tx.send(()).await;
        }

        // Use umount to unmount
        let status = tokio::process::Command::new("umount")
            .arg(&self.config.mountpoint)
            .status()
            .await
            .map_err(|e| Error::String(format!("Failed to run umount: {e}")))?;

        if !status.success() {
            // Try force unmount
            let status = tokio::process::Command::new("umount")
                .arg("-f")
                .arg(&self.config.mountpoint)
                .status()
                .await
                .map_err(|e| Error::String(format!("Failed to run umount -f: {e}")))?;

            if !status.success() {
                return Err(Error::String(format!(
                    "umount failed with status: {status}"
                )));
            }
        }

        Ok(())
    }

    /// Get the mount point path
    pub fn mountpoint(&self) -> &PathBuf {
        &self.config.mountpoint
    }
}

#[tonic::async_trait]
impl VfsService for Arc<Service> {
    async fn mount(
        &self,
        request: Request<MountRequest>,
    ) -> Result<Response<MountResponse>, Status> {
        let req = request.into_inner();

        let env_spec: EnvSpec = req
            .env_spec
            .parse()
            .map_err(|e| Status::invalid_argument(format!("Invalid env spec: {e}")))?;

        let router_guard = self.router.lock().await;
        let router = router_guard.as_ref().ok_or_else(|| {
            Status::failed_precondition("Service not running - FUSE mount not started")
        })?;

        if req.editable {
            // Editable mount with scratch directory
            let runtime_name = if req.runtime_name.is_empty() {
                format!("runtime-{}", req.root_pid)
            } else {
                req.runtime_name.clone()
            };
            router
                .mount_editable(req.root_pid, env_spec, &runtime_name)
                .await
                .map_err(|e| Status::internal(format!("Failed to mount editable: {e}")))?;
            tracing::info!(root_pid = req.root_pid, %runtime_name, "Mounted editable environment");
        } else {
            // Read-only mount
            router
                .mount(req.root_pid, env_spec)
                .await
                .map_err(|e| Status::internal(format!("Failed to mount: {e}")))?;
            tracing::info!(root_pid = req.root_pid, "Mounted read-only environment");
        }

        Ok(Response::new(MountResponse {}))
    }

    async fn unmount(
        &self,
        request: Request<UnmountRequest>,
    ) -> Result<Response<UnmountResponse>, Status> {
        let req = request.into_inner();

        let router_guard = self.router.lock().await;
        let router = router_guard.as_ref().ok_or_else(|| {
            Status::failed_precondition("Service not running - FUSE mount not started")
        })?;

        let was_mounted = router.unmount(req.root_pid);
        tracing::info!(root_pid = req.root_pid, was_mounted, "Unmounted environment");

        Ok(Response::new(UnmountResponse { was_mounted }))
    }

    async fn shutdown(
        &self,
        _request: Request<ShutdownRequest>,
    ) -> Result<Response<ShutdownResponse>, Status> {
        tracing::info!("Shutdown requested");

        self.stop()
            .await
            .map_err(|e| Status::internal(format!("Failed to stop service: {e}")))?;

        Ok(Response::new(ShutdownResponse {}))
    }

    async fn status(
        &self,
        _request: Request<StatusRequest>,
    ) -> Result<Response<StatusResponse>, Status> {
        let router_guard = self.router.lock().await;
        let router = router_guard.as_ref().ok_or_else(|| {
            Status::failed_precondition("Service not running - FUSE mount not started")
        })?;

        let mounts: Vec<MountInfo> = router
            .iter_mounts()
            .iter()
            .map(|(pid, mount)| MountInfo {
                root_pid: *pid,
                env_spec: mount.env_spec().to_string(),
                editable: mount.is_editable(),
                runtime_name: mount.runtime_name().unwrap_or_default().to_string(),
            })
            .collect();

        Ok(Response::new(StatusResponse {
            active_mounts: mounts.len() as u32,
            mounts,
        }))
    }
}

/// Clean up scratch directories from previous service runs.
///
/// This finds any scratch directories under ~/Library/Caches/spfs/scratch/
/// and removes them if they are older than 24 hours. This handles cases
/// where the service or runtime crashed without proper cleanup.
async fn cleanup_orphaned_scratch_directories() {
    // Use the macOS-approved cache directory
    let scratch_dir = match dirs::cache_dir() {
        Some(cache) => cache.join("spfs").join("scratch"),
        None => return,
    };

    let Ok(mut entries) = tokio::fs::read_dir(&scratch_dir).await else {
        return;
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        // Each entry is a runtime's scratch directory
        // For now, just remove any scratch directories older than 24 hours
        // as a conservative cleanup
        if let Ok(metadata) = entry.metadata().await
            && let Ok(modified) = metadata.modified()
        {
            let age = std::time::SystemTime::now()
                .duration_since(modified)
                .unwrap_or_default();

            if age > std::time::Duration::from_secs(24 * 60 * 60) {
                tracing::info!(path = %entry.path().display(), "removing orphaned scratch directory");
                let _ = tokio::fs::remove_dir_all(entry.path()).await;
            }
        }
    }
}
