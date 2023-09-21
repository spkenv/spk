// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashMap;
use std::net::SocketAddr;
use std::os::windows::process::CommandExt;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use libc::c_void;
use proto::vfs_service_server::VfsService;
use spfs::tracking::EnvSpec;
use spfs_cli_common as cli;
use spfs_vfs::proto;
use tonic::{async_trait, Request, Response, Status};
use tracing::instrument;
use windows::core::HRESULT;
use windows::Win32::Foundation::{CloseHandle, ERROR_NO_MORE_FILES};
use windows::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows::Win32::Security::PSECURITY_DESCRIPTOR;
use windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_DIRECTORY;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32Next, PROCESSENTRY32, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::SystemInformation::GetSystemTimeAsFileTime;
use windows::Win32::System::Threading::DETACHED_PROCESS;
use winfsp::filesystem::{DirBuffer, ModificationDescriptor};
use winfsp_sys::FILE_ACCESS_RIGHTS;
//use spfs_vfs::{Config, Session};

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
    async fn run(&mut self, _config: &spfs::Config) -> Result<i32> {
        if self.stop {
            return self.stop().await;
        }

        let init_token = winfsp::winfsp_init().context("Failed to initialize winfsp")?;
        let filesystem = start_service(&self.mountpoint).map(Arc::new)?;
        let fs = Arc::clone(&filesystem);
        let fsp = winfsp::service::FileSystemServiceBuilder::default()
            .with_start(move || Ok(Arc::clone(&fs)))
            .with_stop(|fs| {
                if let Some(f) = fs {
                    let fs = Arc::clone(&f);
                    tracing::info!("Stopping winfsp service...");
                    tokio::task::spawn(async move { fs.shutdown().await });
                }
                Ok(())
            })
            .build("spfs", init_token)
            .context("Failed to construct filesystem service")?;
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::mpsc::channel(4);
        let service = proto::vfs_service_server::VfsServiceServer::new(Service {
            filesystem,
            shutdown: shutdown_tx.clone(),
        });
        tokio::task::spawn(async move {
            if let Err(err) = tokio::signal::ctrl_c().await {
                tracing::error!(?err, "Failed to setup graceful shutdown handler");
            };
            let _ = shutdown_tx.send(()).await;
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
                tracing::info!("Filesystem service shutdown, exiting...");
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
            return Ok(0);
        };
        if is_connection_refused(&err) {
            tracing::warn!("The service does not appear to be running");
            Ok(0)
        } else {
            Err(err.into())
        }
    }
}

#[derive(Debug, Args)]
struct CmdMount {
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

        todo!("mount not implemented");

        Ok(0)
    }
}

fn start_service(mountpoint: &std::path::Path) -> Result<FileSystem> {
    tracing::info!("starting service...");
    // as of writing, the descritor mode is the only one that works in
    // winsfp-rs without causting crashes
    let mode = winfsp::host::FileContextMode::Descriptor;
    let mut params = winfsp::host::VolumeParams::new(mode);
    params
        .filesystem_name("spfs")
        .case_preserved_names(true)
        .case_sensitive_search(true)
        .hard_links(true)
        .read_only_volume(true)
        .volume_serial_number(7737);
    let mut host = winfsp::host::FileSystemHost::new(params, FileSystemContext::default())
        .context("Failed to establish filesystem host")?;
    host.mount(mountpoint)
        .context("Failed to mount spfs filesystem")?;
    host.start().context("Failed to start filesystem")?;
    Ok(FileSystem {
        host: HostController::new(host),
    })
}

struct HostController {
    shutdown: tokio::sync::mpsc::Sender<()>,
}

impl HostController {
    pub fn new(mut host: winfsp::host::FileSystemHost<'static>) -> Self {
        let (shutdown, mut shutdown_rx) = tokio::sync::mpsc::channel(4);
        let local = tokio::task::LocalSet::new();
        let _guard = local.enter();
        tokio::task::spawn_local(async move {
            let _ = shutdown_rx.recv().await;
            host.stop();
            host.unmount();
        });
        Self { shutdown }
    }
}

struct FileSystem {
    host: HostController,
}

impl FileSystem {
    /// Attempts to unmount and shutdown the hosted filesystem mount
    pub async fn shutdown(&self) {
        let _ = self.host.shutdown.send(()).await;
    }
}

struct Service {
    filesystem: Arc<FileSystem>,
    shutdown: tokio::sync::mpsc::Sender<()>,
}

#[async_trait]
impl VfsService for Service {
    async fn shutdown(
        &self,
        _request: Request<proto::ShutdownRequest>,
    ) -> std::result::Result<Response<proto::ShutdownResponse>, Status> {
        self.shutdown
            .send(())
            .await
            .map_err(|_| tonic::Status::not_found("filesystem is already shutting down"))
            .map(|_| Response::new(proto::ShutdownResponse {}))
    }
}

#[derive(Default)]
struct FileSystemContext;

struct FileContext {
    ino: u64,
    dir_buffer: DirBuffer,
}

impl FileContext {
    fn new(ino: u64) -> Self {
        Self {
            ino,
            dir_buffer: DirBuffer::new(),
        }
    }
}
impl std::fmt::Debug for FileContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileContext")
            .field("ino", &self.ino)
            .finish_non_exhaustive()
    }
}

impl FileSystemContext {
    fn get_process_stack(&self) -> std::result::Result<Vec<u32>, winfsp::FspError> {
        // Safety: only valid when called from within the context of an active operation
        // as this information is stored in the local thread storage
        let pid = unsafe { winfsp_sys::FspFileSystemOperationProcessIdF() };
        get_parent_pids(pid)
    }
}

impl winfsp::filesystem::FileSystemContext for FileSystemContext {
    type FileContext = FileContext;

    #[instrument(skip_all)]
    fn get_security_by_name(
        &self,
        file_name: &winfsp::U16CStr,
        security_descriptor: Option<&mut [c_void]>,
        resolve_reparse_points: impl FnOnce(
            &winfsp::U16CStr,
        ) -> Option<winfsp::filesystem::FileSecurity>,
    ) -> winfsp::Result<winfsp::filesystem::FileSecurity> {
        let path = std::path::PathBuf::from(file_name.to_os_string());

        let stack = self.get_process_stack()?;
        tracing::info!(?path, ?stack, security=%security_descriptor.is_some(),  "start");

        if let Some(security) = resolve_reparse_points(file_name.as_ref()) {
            return Ok(security);
        }

        // a path with no filename component is assumed to be the root path '\\'
        if path.file_name().is_some() {
            tracing::info!(" > done [not found]");
            return Err(winfsp::FspError::IO(std::io::ErrorKind::NotFound));
        }

        let mut file_sec = winfsp::filesystem::FileSecurity {
            reparse: false,
            sz_security_descriptor: 0,
            attributes: FILE_ATTRIBUTE_DIRECTORY.0,
        };

        let sddl = windows::core::w!("O:BAG:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FA;;;WD)");
        let mut psecurity_descriptor = PSECURITY_DESCRIPTOR(std::ptr::null_mut());
        let mut psecurity_descriptor_len: u32 = 0;
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl,
                SDDL_REVISION_1,
                &mut psecurity_descriptor as *mut PSECURITY_DESCRIPTOR,
                Some(&mut psecurity_descriptor_len as *mut u32),
            )?
        };
        tracing::debug!(%psecurity_descriptor_len, "parsed descriptor");
        file_sec.sz_security_descriptor = psecurity_descriptor_len as u64;

        match security_descriptor {
            None => {}
            Some(descriptor) if descriptor.len() as u64 <= file_sec.sz_security_descriptor => {
                tracing::warn!(
                    "needed {}, got {}",
                    file_sec.sz_security_descriptor,
                    descriptor.len()
                );
            }
            Some(descriptor) => unsafe {
                // enough space must be available in the provided buffer for us to
                // mutate/access it
                std::ptr::copy(
                    psecurity_descriptor.0 as *const c_void,
                    descriptor.as_mut_ptr(),
                    file_sec.sz_security_descriptor as usize,
                )
            },
        }

        tracing::info!(" > done");
        Ok(file_sec)
    }

    #[instrument(skip_all)]
    fn open(
        &self,
        file_name: &winfsp::U16CStr,
        create_options: u32,
        granted_access: FILE_ACCESS_RIGHTS,
        file_info: &mut winfsp::filesystem::OpenFileInfo,
    ) -> winfsp::Result<Self::FileContext> {
        let path = std::path::PathBuf::from(file_name.to_os_string());
        tracing::info!(?path, ?granted_access, ?create_options, "start");

        if path.file_name().is_none() {
            let now = unsafe { GetSystemTimeAsFileTime() };
            let now = (now.dwHighDateTime as u64) << 32 | now.dwLowDateTime as u64;
            // a path with no filename component is assumed to be the root path '\\'
            let context = FileContext::new(0);
            let info = file_info.as_mut();
            info.file_attributes = FILE_ATTRIBUTE_DIRECTORY.0;
            info.index_number = context.ino;
            info.file_size = 0;
            info.ea_size = 0;
            info.creation_time = now;
            info.change_time = now;
            info.last_access_time = now;
            info.last_write_time = now;
            info.hard_links = 0;
            info.reparse_tag = 0;
            tracing::info!(" > open done");
            Ok(context)
        } else {
            tracing::info!(" > open done [not found]");
            Err(winfsp::FspError::IO(std::io::ErrorKind::NotFound))
        }
    }

    #[instrument(skip_all)]
    fn read_directory(
        &self,
        context: &Self::FileContext,
        pattern: Option<&winfsp::U16CStr>,
        marker: winfsp::filesystem::DirMarker,
        buffer: &mut [u8],
    ) -> winfsp::Result<u32> {
        let pattern = pattern.map(|p| p.to_os_string());
        tracing::info!(?context, ?marker, buffer=%buffer.len(), ?pattern,  "start");
        let written = context.dir_buffer.read(marker, buffer);
        tracing::debug!(%written, " > done");
        Ok(written)
    }

    #[instrument(skip_all)]
    fn close(&self, context: Self::FileContext) {
        tracing::info!(?context, "start");
    }

    #[instrument(skip_all)]
    fn create(
        &self,
        _file_name: &winfsp::U16CStr,
        _create_options: u32,
        _granted_access: FILE_ACCESS_RIGHTS,
        _file_attributes: winfsp_sys::FILE_FLAGS_AND_ATTRIBUTES,
        _security_descriptor: Option<&[c_void]>,
        _allocation_size: u64,
        _extra_buffer: Option<&[u8]>,
        _extra_buffer_is_reparse_point: bool,
        _file_info: &mut winfsp::filesystem::OpenFileInfo,
    ) -> winfsp::Result<Self::FileContext> {
        tracing::info!("start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn cleanup(
        &self,
        context: &Self::FileContext,
        file_name: Option<&winfsp::U16CStr>,
        _flags: u32,
    ) {
        let path = file_name
            .map(|f| f.to_os_string())
            .map(std::path::PathBuf::from);
        tracing::info!(?context, ?path, "start");
    }

    #[instrument(skip_all)]
    fn flush(
        &self,
        context: Option<&Self::FileContext>,
        _file_info: &mut winfsp::filesystem::FileInfo,
    ) -> winfsp::Result<()> {
        tracing::info!(?context, "start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn get_file_info(
        &self,
        context: &Self::FileContext,
        file_info: &mut winfsp::filesystem::FileInfo,
    ) -> winfsp::Result<()> {
        tracing::info!(?context, "start");

        if context.ino == 0 {
            let now = unsafe { GetSystemTimeAsFileTime() };
            let now = (now.dwHighDateTime as u64) << 32 | now.dwLowDateTime as u64;
            file_info.file_attributes = FILE_ATTRIBUTE_DIRECTORY.0;
            file_info.index_number = context.ino;
            file_info.file_size = 1;
            file_info.ea_size = 0;
            file_info.creation_time = now;
            file_info.change_time = now;
            file_info.last_access_time = now;
            file_info.last_write_time = now;
            file_info.hard_links = 0;
            file_info.reparse_tag = 0;
            tracing::info!(" > done");
            Ok(())
        } else {
            tracing::info!(" > done [not found]");
            Err(winfsp::FspError::IO(std::io::ErrorKind::NotFound))
        }
    }

    #[instrument(skip_all)]
    fn get_security(
        &self,
        context: &Self::FileContext,
        _security_descriptor: Option<&mut [c_void]>,
    ) -> winfsp::Result<u64> {
        tracing::info!(?context, "start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn set_security(
        &self,
        context: &Self::FileContext,
        _security_information: u32,
        _modification_descriptor: ModificationDescriptor,
    ) -> winfsp::Result<()> {
        tracing::info!(?context, "start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn overwrite(
        &self,
        context: &Self::FileContext,
        _file_attributes: winfsp_sys::FILE_FLAGS_AND_ATTRIBUTES,
        _replace_file_attributes: bool,
        _allocation_size: u64,
        _extra_buffer: Option<&[u8]>,
        _file_info: &mut winfsp::filesystem::FileInfo,
    ) -> winfsp::Result<()> {
        tracing::info!(?context, "start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn rename(
        &self,
        context: &Self::FileContext,
        _file_name: &winfsp::U16CStr,
        _new_file_name: &winfsp::U16CStr,
        _replace_if_exists: bool,
    ) -> winfsp::Result<()> {
        tracing::info!(?context, "start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn set_basic_info(
        &self,
        context: &Self::FileContext,
        _file_attributes: u32,
        _creation_time: u64,
        _last_access_time: u64,
        _last_write_time: u64,
        _last_change_time: u64,
        _file_info: &mut winfsp::filesystem::FileInfo,
    ) -> winfsp::Result<()> {
        tracing::info!(?context, "start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn set_delete(
        &self,
        context: &Self::FileContext,
        _file_name: &winfsp::U16CStr,
        _delete_file: bool,
    ) -> winfsp::Result<()> {
        tracing::info!(?context, "start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn set_file_size(
        &self,
        context: &Self::FileContext,
        _new_size: u64,
        _set_allocation_size: bool,
        _file_info: &mut winfsp::filesystem::FileInfo,
    ) -> winfsp::Result<()> {
        tracing::info!(?context, "start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn read(
        &self,
        context: &Self::FileContext,
        _buffer: &mut [u8],
        _offset: u64,
    ) -> winfsp::Result<u32> {
        tracing::info!(?context, "start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn write(
        &self,
        context: &Self::FileContext,
        _buffer: &[u8],
        _offset: u64,
        _write_to_eof: bool,
        _constrained_io: bool,
        _file_info: &mut winfsp::filesystem::FileInfo,
    ) -> winfsp::Result<u32> {
        tracing::info!(?context, "start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn get_dir_info_by_name(
        &self,
        context: &Self::FileContext,
        _file_name: &winfsp::U16CStr,
        _out_dir_info: &mut winfsp::filesystem::DirInfo,
    ) -> winfsp::Result<()> {
        tracing::info!(?context, "start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn get_volume_info(
        &self,
        _out_volume_info: &mut winfsp::filesystem::VolumeInfo,
    ) -> winfsp::Result<()> {
        tracing::info!("start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn set_volume_label(
        &self,
        _volume_label: &winfsp::U16CStr,
        _volume_info: &mut winfsp::filesystem::VolumeInfo,
    ) -> winfsp::Result<()> {
        tracing::info!("start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn get_stream_info(
        &self,
        context: &Self::FileContext,
        _buffer: &mut [u8],
    ) -> winfsp::Result<u32> {
        tracing::info!(?context, "start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn get_reparse_point_by_name(
        &self,
        _file_name: &winfsp::U16CStr,
        _is_directory: bool,
        _buffer: &mut [u8],
    ) -> winfsp::Result<u64> {
        tracing::info!("start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn get_reparse_point(
        &self,
        context: &Self::FileContext,
        _file_name: &winfsp::U16CStr,
        _buffer: &mut [u8],
    ) -> winfsp::Result<u64> {
        tracing::info!(?context, "start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn set_reparse_point(
        &self,
        context: &Self::FileContext,
        _file_name: &winfsp::U16CStr,
        _buffer: &[u8],
    ) -> winfsp::Result<()> {
        tracing::info!(?context, "start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn delete_reparse_point(
        &self,
        context: &Self::FileContext,
        _file_name: &winfsp::U16CStr,
        _buffer: &[u8],
    ) -> winfsp::Result<()> {
        tracing::info!(?context, "start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn get_extended_attributes(
        &self,
        context: &Self::FileContext,
        _buffer: &mut [u8],
    ) -> winfsp::Result<u32> {
        tracing::info!(?context, "start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn set_extended_attributes(
        &self,
        context: &Self::FileContext,
        _buffer: &[u8],
        _file_info: &mut winfsp::filesystem::FileInfo,
    ) -> winfsp::Result<()> {
        tracing::info!(?context, "start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn control(
        &self,
        context: &Self::FileContext,
        _control_code: u32,
        _input: &[u8],
        _output: &mut [u8],
    ) -> winfsp::Result<u32> {
        tracing::info!(?context, "start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn dispatcher_stopped(&self, _normally: bool) {}
}

fn get_parent_pids(mut child: u32) -> std::result::Result<Vec<u32>, winfsp::FspError> {
    let no_more_files = HRESULT::from(ERROR_NO_MORE_FILES);
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, child)? };
    let mut parents = HashMap::new();
    let mut process = PROCESSENTRY32::default();
    process.dwSize = std::mem::size_of::<PROCESSENTRY32>() as u32;
    loop {
        match unsafe { Process32Next(snapshot, &mut process as *mut PROCESSENTRY32) } {
            Ok(()) => {
                parents.insert(process.th32ProcessID, process.th32ParentProcessID);
            }
            Err(err) if err.code() == no_more_files => break,
            Err(err) => tracing::error!(%err, "error"),
        }
    }
    let mut stack = Vec::with_capacity(8);
    stack.push(child);
    while let Some(parent) = parents.get(&child) {
        stack.push(*parent);
        child = *parent;
    }
    let _ = unsafe { CloseHandle(snapshot) };
    Ok(stack)
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
