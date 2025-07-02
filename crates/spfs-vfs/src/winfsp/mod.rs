// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! A virtual filesystem implementation backed by winfsp

use std::sync::Arc;

use proto::vfs_service_server::VfsService;
use spfs::storage::FromConfig;
use tonic::{Request, Response, Status, async_trait};
use tracing::instrument;
pub use winfsp::Result;

use crate::proto;

mod handle;
mod mount;
mod router;

pub use handle::Handle;
pub use mount::Mount;
pub use router::{Router, get_parent_pids};
use winfsp::host::VolumeParams;

/// Options to configure the WinFSP filesystem and
/// its behavior at runtime
#[derive(Debug, Clone)]
pub struct Config {
    /// The location on the host that this service will appear
    pub mountpoint: std::path::PathBuf,
    /// Remote repositories that can be read from.
    ///
    /// These are in addition to the local repository and
    /// are searched in order to find data.
    pub remotes: Vec<String>,
}

/// A global service that is presented as part of the filesystem via winfsp.
///
/// A single instance of this service runs at each mountpoint. The service exposes
/// a local gRPC endpoint used to control the runtime and add per-process mounts
/// that are managed via the [`Router`].
pub struct Service {
    //repos: Vec<Arc<spfs::storage::RepositoryHandle>>,
    _config: Config,
    router: Router,
    host: HostController,
    // ttl: Duration,
    // next_inode: AtomicU64,
    // next_handle: AtomicU64,
    // inodes: DashMap<u64, Arc<Entry<u64>>>,
    // handles: DashMap<u64, Handle>,
}

impl Service {
    /// Establish a new service with the provided config.
    ///
    /// The returned service is registered with winfsp, mounted
    /// to the windows filesystem, and visible to users.
    pub async fn new(config: Config) -> spfs::Result<Arc<Self>> {
        let spfs_config = spfs::Config::current()?;
        tracing::debug!("Opening repositories...");
        let proxy_config = spfs::storage::proxy::Config {
            primary: format!(
                "file://{}?create=true",
                spfs_config.storage.root.to_string_lossy()
            ),
            secondary: config.remotes.clone(),
        };
        let repo = spfs::storage::ProxyRepository::from_config(proxy_config)
            .await
            .map_err(|source| spfs::Error::FailedToOpenRepository {
                repository: "<WinFSP Repository Stack>".into(),
                source,
            })?;
        let repos = repo.into_stack().into_iter().map(Arc::new).collect();

        // as of writing, the descriptor mode is the only one that works in
        // winfsp-rs without causing crashes
        let mode = winfsp::host::FileContextMode::Descriptor;
        let mut params = winfsp::host::VolumeParams::new(mode);
        params
            .filesystem_name("spfs")
            .case_preserved_names(true)
            .case_sensitive_search(true)
            .hard_links(true)
            .read_only_volume(false)
            .volume_serial_number(7737);
        let router = Router::new(repos).await?;
        let host = HostController::new(&config.mountpoint, params, router.clone()).await?;
        Ok(Arc::new(Self {
            host,
            router,
            _config: config,
        }))
    }

    /// Initialize this service's filesystem in winfsp
    pub fn build_filesystem_service(
        self: &Arc<Self>,
        init_token: winfsp::FspInit,
    ) -> Result<winfsp::service::FileSystemService<Router>> {
        let self_start = Arc::clone(self);
        winfsp::service::FileSystemServiceBuilder::default()
            .with_start(move || Ok(self_start.router.clone()))
            .with_stop(|_fs| Ok(()))
            .build("spfs", init_token)
    }
}

/// Holds a [`winfsp::host::FileSystemHost`], allowing it to be
/// shutdown gracefully from an async runtime
struct HostController {
    shutdown: tokio::sync::mpsc::Sender<()>,
}

impl HostController {
    pub async fn new<P: Into<std::path::PathBuf>>(
        mountpoint: P,
        params: VolumeParams,
        router: Router,
    ) -> spfs::Result<Self> {
        let mountpoint = mountpoint.into();
        let (shutdown, mut shutdown_rx) = tokio::sync::mpsc::channel(4);
        let (result_tx, result_rx) = tokio::sync::oneshot::channel();
        // The filesystem host is not Send and so must be contained to the
        // thread in which it was created. For this reason we spawn a separate
        // thread to handle the lifecycle.
        std::thread::spawn(move || {
            let mut host = match winfsp::host::FileSystemHost::new(params, router) {
                Ok(h) => h,
                Err(err) => {
                    let _ = result_tx.send(Err(winfsp::FspError::from(err)));
                    return;
                }
            };
            if let Err(err) = host.mount(&mountpoint) {
                let _ = result_tx.send(Err(winfsp::FspError::from(err)));
                return;
            }
            if let Err(err) = host.start() {
                let _ = result_tx.send(Err(winfsp::FspError::from(err)));
                return;
            }
            let _ = result_tx.send(Ok(()));
            let _ = shutdown_rx.blocking_recv();
            tracing::debug!("shutdown message received, stopping host...");
            host.stop();
            tracing::debug!("host stopped, unmounting...");
            host.unmount();
            tracing::debug!("host unmounted...");
        });
        match result_rx.await.expect("Startup thread should not panic") {
            Ok(()) => Ok(Self { shutdown }),
            Err(winfsp::FspError::HRESULT(r)) => Err(spfs::Error::Win(r.into())),
            Err(winfsp::FspError::WIN32(w)) => Err(spfs::Error::Win(w.into())),
            Err(winfsp::FspError::NTSTATUS(s)) => Err(spfs::Error::Win(s.into())),
            Err(winfsp::FspError::IO(i)) => Err(spfs::Error::String(i.to_string())),
        }
    }

    /// Attempt to unmount and shutdown the underlying filesystem.
    ///
    /// Returns true if the shutdown message was successfully received,
    /// and false if the filesystem appears to have already stopped or
    /// crashed.
    pub fn shutdown(&self) -> impl std::future::Future<Output = bool> + 'static {
        // do not hold a reference to self across the await so that
        // the future returned by this function can be considered 'static
        let tx = self.shutdown.clone();
        async move { tx.send(()).await.is_ok() }
    }
}

#[async_trait]
impl VfsService for Arc<Service> {
    #[instrument(skip_all)]
    async fn shutdown(
        &self,
        _request: Request<proto::ShutdownRequest>,
    ) -> std::result::Result<Response<proto::ShutdownResponse>, Status> {
        tracing::debug!("received");
        if self.host.shutdown().await {
            Ok(Response::new(proto::ShutdownResponse {}))
        } else {
            Err(tonic::Status::not_found(
                "filesystem is already shutting down",
            ))
        }
    }

    #[instrument(skip_all)]
    async fn mount(
        &self,
        request: Request<proto::MountRequest>,
    ) -> std::result::Result<Response<proto::MountResponse>, Status> {
        tracing::debug!("received");
        let inner = request.into_inner();
        let env_spec = spfs::tracking::EnvSpec::parse(&inner.env_spec).map_err(|err| {
            Status::invalid_argument(format!("Provided env spec was invalid: {err}"))
        })?;
        if let Err(err) = self.router.mount(inner.root_pid, env_spec).await {
            tracing::error!("{err}");
            return Err(Status::internal(format!(
                "Failed to mount filesystem: {err}"
            )));
        }
        Ok(Response::new(proto::MountResponse {}))
    }
}
