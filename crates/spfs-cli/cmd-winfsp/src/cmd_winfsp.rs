// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::net::SocketAddr;
use std::os::windows::process::CommandExt;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand};
use spfs::tracking::EnvSpec;
use spfs_cli_common as cli;
use spfs_vfs::{proto, Service};
use tonic::Request;
use windows::Win32::System::Threading::DETACHED_PROCESS;

// The runtime setup process manages the current namespace
// which operates only on the current thread. For this reason
// we must use a single threaded async runtime, if any.
fn main() {
    // because this function exits right away it does not
    // properly handle destruction of data, so we put the actual
    // logic into a separate function/scope
    std::process::exit(main2())
}
fn main2() -> i32 {
    let mut opt = CmdWinFsp::parse();
    // opt.logging
    //     .log_file
    //     .get_or_insert("/tmp/spfs-runtime/fuse.log".into());
    opt.logging.syslog = true;
    opt.logging.configure();

    let config = match spfs::get_config() {
        Err(err) => {
            tracing::error!(err = ?err, "failed to load config");
            return 1;
        }
        Ok(config) => config,
    };

    let result = opt.run(&config);

    spfs_cli_common::handle_result!(result)
}

/// Run a virtual filesystem backed by winfsp
#[derive(Debug, Parser)]
#[clap(name = "spfs-winfsp")]
pub struct CmdWinFsp {
    #[clap(flatten)]
    logging: cli::Logging,

    #[clap(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
#[clap(name = "spfs-winfsp")]
enum Command {
    Service(CmdService),
    Mount(CmdMount),
}

impl cli::CommandName for CmdWinFsp {
    fn command_name(&self) -> &str {
        "winfsp"
    }
}

impl CmdWinFsp {
    fn run(&mut self, config: &spfs::Config) -> Result<i32> {
        // the actual winfsp filesystem uses it's own threads, and
        // the mount command only needs to send requests to the running
        // service, so a current thread runtime is appropriate
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("Failed to establish async runtime")?;
        let res = match &mut self.command {
            Command::Mount(c) => rt.block_on(c.run(config)),
            Command::Service(c) => rt.block_on(c.run(config)),
        };
        rt.shutdown_timeout(std::time::Duration::from_secs(30));
        res
    }
}

/// Start the background filesystem service
///
/// Typically this process is handled transparently as-needed
/// but can be executed manually to establish an spfs mount ahead
/// of entering any specific environments.
///
/// This will fail if an instance of the filesystem is already mounted
/// at the specified path
#[derive(Debug, Args)]
struct CmdService {
    /// Stop the running service instead of starting it
    #[clap(long, exclusive = true)]
    stop: bool,

    /// The local address to listen on for filesystem control
    ///
    /// If the default value is overriden, any subsequent control commands must
    /// also be given this new value. Conversely, changing the mount point from
    /// its default value should require a change to this value
    #[clap(
        long,
        default_value = "127.0.0.1:37737",
        env = "SPFS_WINFSP_LISTEN_ADDRESS"
    )]
    listen: SocketAddr,

    /// The location where to mount the spfs runtime
    ///
    /// Overriding the default value requires the specification of an
    /// alternative '--listen' address for safety
    #[clap(default_value = "C:\\spfs", requires = "listen")]
    mountpoint: std::path::PathBuf,
}

impl CmdService {
    async fn run(&mut self, config: &spfs::Config) -> Result<i32> {
        if self.stop {
            return self.stop().await;
        }

        let init_token = winfsp::winfsp_init().context("Failed to initialize winfsp")?;
        tracing::info!("starting service...");
        let config = spfs_vfs::Config {
            mountpoint: self.mountpoint.clone(),
            remotes: config.filesystem.secondary_repositories.clone(),
        };
        let service = Service::new(config)
            .await
            .context("Failed to start filesystem service")?;
        let fsp = service
            .build_filesystem_service(init_token)
            .context("Failed to build filesystem service")?;
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::mpsc::channel(4);
        let ctrl_c_shutdown_tx = shutdown_tx.clone();
        let service = proto::vfs_service_server::VfsServiceServer::new(Arc::clone(&service));
        tokio::task::spawn(async move {
            if let Err(err) = tokio::signal::ctrl_c().await {
                tracing::error!(?err, "Failed to setup graceful shutdown handler");
            };
            let _ = ctrl_c_shutdown_tx.send(()).await;
        });
        let service = tonic::transport::Server::builder()
            .add_service(service)
            .serve_with_shutdown(self.listen, async {
                let _ = shutdown_rx.recv().await;
                tracing::info!("shutting down gRPC server...");
            });
        let fs_thread_handle = fsp.start();
        let fs_handle = tokio::task::spawn_blocking(|| fs_thread_handle.join());
        tokio::select! {
            result = fs_handle => {
                result
                    .expect("Filesystem task should not panic")
                    .expect("Filesystem thread should not panic")
                    .context("Filesystem failed during runtime")?;
                tracing::info!("filesystem service shutdown, exiting...");
                let _ = shutdown_tx.send(()).await;
            }
            _ = service => {
                tracing::info!("socket has shutdown, filesystem exiting...");
                fsp.stop();
            }
        }
        Ok(0)
    }

    async fn stop(&self) -> Result<i32> {
        let channel = tonic::transport::Endpoint::from_shared(format!("http://{}", self.listen))?
            .connect_lazy();
        let mut client = spfs_vfs::proto::vfs_service_client::VfsServiceClient::new(channel);
        let res = client
            .shutdown(tonic::Request::new(proto::ShutdownRequest {}))
            .await;
        let Err(err) = res else {
            tracing::info!("Stop request accepted");
            return Ok(0);
        };
        if is_connection_refused(&err) {
            tracing::warn!(addr=%self.listen, "The service does not appear to be running");
            Ok(0)
        } else {
            Err(err.into())
        }
    }
}

#[derive(Debug, Args)]
struct CmdMount {
    /// The process id for which the mount will be visible, along
    /// with all of it's children. Defaults to the calling process.
    #[clap(long)]
    root_process: Option<u32>,

    /// The local address to connect to for filesystem control
    ///
    /// If the default value is overriden, any subsequent control commands must
    /// also be given this new value. Conversely, changing the mount point from
    /// its default value should require a change to this value
    #[clap(
        long,
        default_value = "127.0.0.1:37737",
        env = "SPFS_WINFSP_LISTEN_ADDRESS"
    )]
    service: SocketAddr,

    /// The location where to mount the spfs runtime
    ///
    /// Overriding the default value requires the specification of an
    /// alternative '--service' address for safety and is only relevant
    /// when the winfsp service is not already running at the given
    /// service address.
    #[clap(long, default_value = "C:\\spfs", requires = "service")]
    mountpoint: std::path::PathBuf,

    /// The tag or id of the files to mount
    ///
    /// Use '-' or '' to request an empty environment
    #[clap(name = "REF")]
    reference: EnvSpec,
}

impl CmdMount {
    async fn run(&mut self, _config: &spfs::Config) -> Result<i32> {
        let result = tonic::transport::Endpoint::from_shared(format!("http://{}", self.service))?
            .connect()
            .await;
        let channel = match result {
            Err(err) if is_connection_refused(&err) => {
                let exe = std::env::current_exe().context("Failed to get current exe")?;
                let mut cmd = std::process::Command::new(exe);
                cmd.creation_flags(DETACHED_PROCESS.0)
                    .arg("service")
                    .arg("--listen")
                    .arg(self.service.to_string())
                    .arg(&self.mountpoint);
                tracing::debug!(?cmd, "spawning service...");
                let _child = cmd.spawn().context("Failed to start filesystem service")?;
                tonic::transport::Endpoint::from_shared(format!("http://{}", self.service))?
                    .connect()
                    .await?
            }
            res => res?,
        };

        let mut client = spfs_vfs::proto::vfs_service_client::VfsServiceClient::new(channel);

        let lineage = spfs_vfs::winfsp::get_parent_pids(None)?;
        let parent = match self.root_process {
            Some(pid) if lineage.contains(&pid) => pid,
            Some(_pid) => bail!("--root-process must the a parent of the current process"),
            // the first parent of this process
            None => match lineage.into_iter().nth(1) {
                None => bail!("Failed to determine the calling process ID"),
                Some(pid) => pid,
            },
        };
        client
            .mount(Request::new(spfs_vfs::proto::MountRequest {
                root_pid: parent,
                env_spec: self.reference.to_string(),
            }))
            .await
            .context("Failed to mount filesystem")?;

        Ok(0)
    }
}

fn is_connection_refused<T>(err: &T) -> bool
where
    T: std::error::Error,
{
    let Some(mut source) = err.source() else {
        return false;
    };

    while let Some(src) = source.source() {
        source = src;
    }

    if let Some(io_err) = source.downcast_ref::<std::io::Error>() {
        if io_err.kind() == std::io::ErrorKind::ConnectionRefused {
            return true;
        }
    }
    false
}
