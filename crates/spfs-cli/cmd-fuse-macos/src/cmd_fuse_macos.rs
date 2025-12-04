// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::net::SocketAddr;
#[cfg(target_os = "macos")]
use std::sync::Arc;

use clap::{Args, Parser, Subcommand};
#[cfg(not(target_os = "macos"))]
use miette::Result;
#[cfg(target_os = "macos")]
use miette::{Context, IntoDiagnostic, Result, bail};
use spfs::tracking::EnvSpec;
use spfs_cli_common as cli;
#[cfg(target_os = "macos")]
use spfs_vfs::macos::{Config, Service, get_parent_pid};
#[cfg(target_os = "macos")]
use spfs_vfs::proto;
#[cfg(target_os = "macos")]
use tonic::Request;

#[cfg(target_os = "macos")]
pub fn main() -> Result<i32> {
    let mut opt = CmdFuseMacos::parse();
    opt.logging.syslog = true;
    // SAFETY: We're in a single-threaded context at this point
    unsafe {
        opt.logging.configure();
    }

    let config = match spfs::get_config() {
        Err(err) => {
            tracing::error!(err = ?err, "failed to load config");
            return Ok(1);
        }
        Ok(config) => config,
    };

    let result = opt.run(&config);
    spfs_cli_common::handle_result!(result)
}

#[cfg(not(target_os = "macos"))]
pub fn main() -> Result<i32> {
    eprintln!("spfs-fuse-macos can only be built and run on macOS");
    eprintln!("For Linux, use: spfs-fuse");
    eprintln!("For Windows, use: spfs-winfsp");
    std::process::exit(1);
}

/// Run a virtual filesystem backed by macFUSE
#[derive(Debug, Parser)]
#[clap(name = "spfs-fuse-macos")]
pub struct CmdFuseMacos {
    #[clap(flatten)]
    logging: cli::Logging,

    #[clap(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Start the macFUSE service daemon
    Service(CmdService),
    /// Mount an environment for the current process tree
    Mount(CmdMount),
    /// Unmount an environment for a specific process tree
    Unmount(CmdUnmount),
    /// Show service status and active mounts
    Status(CmdStatus),
}

impl cli::CommandName for CmdFuseMacos {
    fn command_name(&self) -> &str {
        "fuse-macos"
    }
}

#[cfg(target_os = "macos")]
impl CmdFuseMacos {
    fn run(&mut self, config: &spfs::Config) -> Result<i32> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .into_diagnostic()
            .wrap_err("Failed to establish async runtime")?;
        let res = match &mut self.command {
            Command::Mount(c) => rt.block_on(c.run(config)),
            Command::Unmount(c) => rt.block_on(c.run(config)),
            Command::Service(c) => rt.block_on(c.run(config)),
            Command::Status(c) => rt.block_on(c.run(config)),
        };
        rt.shutdown_timeout(std::time::Duration::from_secs(30));
        res
    }
}

#[derive(Debug, Args)]
struct CmdService {
    /// Stop a running service instead of starting one
    #[clap(long, exclusive = true)]
    stop: bool,

    /// Address for the gRPC service to listen on
    #[clap(
        long,
        default_value = "127.0.0.1:37738",
        env = "SPFS_MACFUSE_LISTEN_ADDRESS"
    )]
    listen: SocketAddr,

    /// Path where the FUSE filesystem should be mounted
    #[clap(default_value = "/spfs")]
    mountpoint: std::path::PathBuf,
}

#[cfg(target_os = "macos")]
impl CmdService {
    async fn run(&mut self, config: &spfs::Config) -> Result<i32> {
        if self.stop {
            return self.stop().await;
        }

        tracing::info!("Starting macFUSE service...");

        let vfs_config = Config {
            mountpoint: self.mountpoint.clone(),
            remotes: config.filesystem.secondary_repositories.clone(),
            ..Default::default()
        };

        let mut service = Service::new(vfs_config)
            .await
            .into_diagnostic()
            .wrap_err("Failed to create service")?;

        service
            .start_mount()
            .await
            .into_diagnostic()
            .wrap_err("Failed to start FUSE mount")?;

        let (shutdown_tx, mut shutdown_rx) = tokio::sync::mpsc::channel(4);
        let ctrl_c_shutdown = shutdown_tx.clone();
        tokio::task::spawn(async move {
            if let Err(err) = tokio::signal::ctrl_c().await {
                tracing::error!(?err, "Failed to setup graceful shutdown handler");
            };
            let _ = ctrl_c_shutdown.send(()).await;
        });

        let grpc_service = proto::vfs_service_server::VfsServiceServer::new(Arc::clone(&service));
        let server = tonic::transport::Server::builder()
            .add_service(grpc_service)
            .serve_with_shutdown(self.listen, async {
                let _ = shutdown_rx.recv().await;
                tracing::info!("Shutting down gRPC server...");
            });

        tracing::info!(listen = %self.listen, mountpoint = %self.mountpoint.display(), "Service started");

        server
            .await
            .into_diagnostic()
            .wrap_err("gRPC server failed")?;

        // Unmount the filesystem on shutdown
        tracing::info!("Unmounting filesystem...");
        if let Err(err) = service.stop().await {
            tracing::warn!(?err, "Failed to cleanly unmount filesystem");
        }

        tracing::info!("Service stopped");
        Ok(0)
    }

    async fn stop(&self) -> Result<i32> {
        let channel = tonic::transport::Endpoint::from_shared(format!("http://{}", self.listen))
            .into_diagnostic()
            .wrap_err("Invalid server address")?
            .connect_lazy();
        let mut client = proto::vfs_service_client::VfsServiceClient::new(channel);
        let res = client
            .shutdown(tonic::Request::new(proto::ShutdownRequest {}))
            .await;
        match res {
            Ok(_) => {
                tracing::info!("Stop request accepted");
                Ok(0)
            }
            Err(err) if is_connection_refused(&err) => {
                tracing::warn!(addr=%self.listen, "The service does not appear to be running");
                Ok(0)
            }
            Err(err) => Err(err).into_diagnostic(),
        }
    }
}

#[derive(Debug, Args)]
struct CmdMount {
    /// The root process ID for the mount (defaults to parent process)
    #[clap(long)]
    root_process: Option<u32>,

    /// Address of the running gRPC service
    #[clap(
        long,
        default_value = "127.0.0.1:37738",
        env = "SPFS_MACFUSE_LISTEN_ADDRESS"
    )]
    service: SocketAddr,

    /// Create an editable mount with write support via scratch directory
    #[clap(long, short = 'e')]
    editable: bool,

    /// Runtime name for scratch directory (auto-generated if not provided)
    #[clap(long)]
    runtime_name: Option<String>,

    /// The environment reference to mount (e.g., tag name or digest)
    #[clap(name = "REF")]
    reference: EnvSpec,
}

#[cfg(target_os = "macos")]
impl CmdMount {
    async fn run(&mut self, _config: &spfs::Config) -> Result<i32> {
        let result = tonic::transport::Endpoint::from_shared(format!("http://{}", self.service))
            .into_diagnostic()
            .wrap_err("Invalid server address")?
            .connect()
            .await;

        let channel = match result {
            Err(err) if is_connection_refused(&err) => {
                bail!("Service is not running. Start it with: spfs-fuse-macos service");
            }
            res => res.into_diagnostic()?,
        };

        let mut client = proto::vfs_service_client::VfsServiceClient::new(channel);

        let root_pid = match self.root_process {
            Some(pid) => pid,
            None => get_parent_pid().into_diagnostic()?,
        };

        let runtime_name = self
            .runtime_name
            .clone()
            .unwrap_or_else(|| format!("runtime-{}", root_pid));

        client
            .mount(Request::new(proto::MountRequest {
                root_pid,
                env_spec: self.reference.to_string(),
                editable: self.editable,
                runtime_name,
            }))
            .await
            .into_diagnostic()
            .wrap_err("Failed to mount filesystem")?;

        if self.editable {
            tracing::info!(root_pid, env_spec = %self.reference, "Editable mount registered");
        } else {
            tracing::info!(root_pid, env_spec = %self.reference, "Mount registered");
        }
        Ok(0)
    }
}

/// Unmount an environment for a specific process tree
#[derive(Debug, Args)]
struct CmdUnmount {
    /// The root process ID to unmount
    #[clap(name = "PID")]
    root_pid: u32,

    /// Address of the running gRPC service
    #[clap(
        long,
        default_value = "127.0.0.1:37738",
        env = "SPFS_MACFUSE_LISTEN_ADDRESS"
    )]
    service: SocketAddr,
}

#[cfg(target_os = "macos")]
impl CmdUnmount {
    async fn run(&self, _config: &spfs::Config) -> Result<i32> {
        let result = tonic::transport::Endpoint::from_shared(format!("http://{}", self.service))
            .into_diagnostic()
            .wrap_err("Invalid server address")?
            .connect()
            .await;

        let channel = match result {
            Err(err) if is_connection_refused(&err) => {
                bail!("Service is not running. Start it with: spfs-fuse-macos service");
            }
            res => res.into_diagnostic()?,
        };

        let mut client = proto::vfs_service_client::VfsServiceClient::new(channel);

        let response = client
            .unmount(Request::new(proto::UnmountRequest {
                root_pid: self.root_pid,
            }))
            .await
            .into_diagnostic()
            .wrap_err("Failed to unmount")?;

        let resp = response.into_inner();
        if resp.was_mounted {
            tracing::info!(root_pid = self.root_pid, "Unmounted environment");
        } else {
            tracing::info!(root_pid = self.root_pid, "No mount found for PID");
        }
        Ok(0)
    }
}

/// Show service status and active mounts
#[derive(Debug, Args)]
struct CmdStatus {
    /// Address of the running gRPC service
    #[clap(
        long,
        default_value = "127.0.0.1:37738",
        env = "SPFS_MACFUSE_LISTEN_ADDRESS"
    )]
    service: SocketAddr,
}

#[cfg(target_os = "macos")]
impl CmdStatus {
    async fn run(&self, _config: &spfs::Config) -> Result<i32> {
        let channel = tonic::transport::Endpoint::from_shared(format!("http://{}", self.service))
            .into_diagnostic()?
            .connect()
            .await
            .into_diagnostic()
            .wrap_err("Failed to connect to service - is it running?")?;

        let mut client = proto::vfs_service_client::VfsServiceClient::new(channel);
        let response = client
            .status(proto::StatusRequest {})
            .await
            .into_diagnostic()?;
        let status = response.into_inner();

        println!("Active mounts: {}", status.active_mounts);

        if status.mounts.is_empty() {
            println!("No active mounts");
        } else {
            println!();
            for mount in status.mounts {
                println!(
                    "  PID {}: {} (editable: {})",
                    mount.root_pid, mount.env_spec, mount.editable
                );
            }
        }

        Ok(0)
    }
}

#[cfg(target_os = "macos")]
fn is_connection_refused(err: &impl std::error::Error) -> bool {
    let err_str = err.to_string();
    err_str.contains("Connection refused") || err_str.contains("connection refused")
}
