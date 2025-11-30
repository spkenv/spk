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

use super::router::Router;
use super::Config;
use crate::proto::{
    MountRequest, MountResponse, ShutdownRequest, ShutdownResponse,
    vfs_service_server::VfsService,
};
use crate::Error;

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
        // Open the configured repositories
        let mut repos = Vec::new();

        // Open local repository
        let local = spfs::open_repository("local")
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
    pub fn start_mount(self: &mut Arc<Self>) -> Result<(), Error> {
        let service = Arc::clone(self);

        // Create shutdown channel
        let (shutdown_tx, _shutdown_rx) = mpsc::channel::<()>(1);

        // Create the router
        let repos = service.repos.clone();
        let rt = tokio::runtime::Handle::current();
        let router = rt.block_on(async {
            Router::new(repos).await
        }).map_err(|e| Error::String(format!("Failed to create router: {e}")))?;

        // Store router in service
        rt.block_on(async {
            *service.router.lock().await = Some(router.clone());
            *service.shutdown_tx.lock().await = Some(shutdown_tx);
        });

        // Build mount options
        let mut options: Vec<MountOption> = service.config.mount_options.iter().cloned().collect();
        options.push(MountOption::RO); // read-only
        options.push(MountOption::FSName("spfs".to_string()));
        options.push(MountOption::Subtype("spfs".to_string()));
        options.push(MountOption::AllowOther); // allow other users to access

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
            router.mount(req.root_pid, env_spec).await.map_err(|e| {
                Status::internal(format!("Failed to mount: {e}"))
            })?;
            tracing::info!(root_pid = req.root_pid, "Mounted read-only environment");
        }

        Ok(Response::new(MountResponse {}))
    }

    async fn shutdown(
        &self,
        _request: Request<ShutdownRequest>,
    ) -> Result<Response<ShutdownResponse>, Status> {
        tracing::info!("Shutdown requested");

        self.stop().await.map_err(|e| {
            Status::internal(format!("Failed to stop service: {e}"))
        })?;

        Ok(Response::new(ShutdownResponse {}))
    }
}
