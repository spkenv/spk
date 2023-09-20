// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use anyhow::{Context, Result};
use clap::Parser;
use libc::c_void;
use spfs::tracking::EnvSpec;
use spfs_cli_common as cli;
use tracing::instrument;
use windows::Win32::System::SystemInformation::GetSystemTimeAsFileTime;
use windows::Win32::{
    Foundation::STATUS_NONCONTINUABLE_EXCEPTION,
    Security::{
        Authorization::{ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1},
        PSECURITY_DESCRIPTOR,
    },
    Storage::FileSystem::FILE_ATTRIBUTE_DIRECTORY,
};
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

/// Run a fuse
#[derive(Debug, Parser)]
#[clap(name = "spfs-winfsp")]
pub struct CmdWinFsp {
    #[clap(flatten)]
    logging: cli::Logging,

    /// Do not daemonize the filesystem, run it in the foreground instead
    #[clap(long, short)]
    foreground: bool,

    /// Do not disconnect the filesystem logs from stderr
    ///
    /// Although the filesystem will still daemonize, the logs will
    /// still appear in the stderr of the calling process/shell
    #[clap(long, short, env = "SPFS_FUSE_LOG_FOREGROUND")]
    log_foreground: bool,

    /// Options for the mount in the form opt1,opt2=value
    ///
    /// In addition to all existing fuse mount options, the following custom
    /// options are also supported:
    ///
    ///  uid    - the user id that should own all files in the mount, defaults to
    ///           the effective user id of the caller. Only allowed when running
    ///           as root/sudo.
    ///  gid    - the group id that should own all files in the mount, defaults to
    ///           the effective user id of the caller. Only allowed when running
    ///           as root/sudo.
    ///  remote - additional remote repository to read data from, can be given more
    ///           than once
    #[clap(long, short, value_delimiter = ',')]
    options: Vec<String>,

    /// The tag or id of the files to mount
    ///
    /// Use '-' or nothing to request an empty environment
    #[clap(name = "REF")]
    reference: EnvSpec,

    /// The location where to mount the spfs runtime
    #[clap(default_value = "/spfs")]
    mountpoint: std::path::PathBuf,
}

impl cli::CommandName for CmdWinFsp {
    fn command_name(&self) -> &str {
        "winfsp"
    }
}

impl CmdWinFsp {
    pub fn run(&mut self, _config: &spfs::Config) -> Result<i32> {
        let init_token = winfsp::winfsp_init().context("Failed to initialize winfsp")?;
        let fsp = winfsp::service::FileSystemServiceBuilder::default()
            .with_start(|| match start_service() {
                Ok(svc) => Ok(svc),
                Err(err) => {
                    tracing::error!("{err:?}");
                    Err(STATUS_NONCONTINUABLE_EXCEPTION)
                }
            })
            .with_stop(|fs| {
                stop_service(fs);
                Ok(())
            })
            .build("spfs", init_token)
            .unwrap();
        fsp.start()
            .join()
            .unwrap()
            .context("Filesystem failed during runtime")
            .map(|()| 0)
    }
}

fn start_service() -> Result<FileSystem> {
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
    let mut spfs = FileSystem {
        fs: winfsp::host::FileSystemHost::new(params, FileSystemContext::default()).unwrap(),
    };
    spfs.fs
        .mount("C:\\spfs")
        .context("Failed to mount spfs filesystem")?;
    spfs.fs.start().context("Failed to start filesystem")?;
    Ok(spfs)
}

fn stop_service(fs: Option<&mut FileSystem>) {
    if let Some(f) = fs {
        tracing::info!("Stopping winfsp service...");
        f.fs.stop();
    }
}

struct FileSystem {
    fs: winfsp::host::FileSystemHost<'static>,
}

#[derive(Default)]
struct FileSystemContext {
    one_at_time: std::sync::Mutex<usize>,
}

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
        let mut guard = self.one_at_time.lock().unwrap();
        *guard += 1;
        let path = std::path::PathBuf::from(file_name.to_os_string());
        tracing::info!(?path, security=%security_descriptor.is_some(), op=%guard, "start");

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
        let mut guard = self.one_at_time.lock().unwrap();
        *guard += 1;
        let path = std::path::PathBuf::from(file_name.to_os_string());
        tracing::info!(?path, ?granted_access, ?create_options, op=%guard, "start");

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
        let mut guard = self.one_at_time.lock().unwrap();
        *guard += 1;
        let pattern = pattern.map(|p| p.to_os_string());
        tracing::info!(?context, ?marker, buffer=%buffer.len(), ?pattern, op=%guard, "start");
        let written = context.dir_buffer.read(marker, buffer);
        tracing::debug!(%written, " > done");
        Ok(written)
    }

    #[instrument(skip_all)]
    fn close(&self, context: Self::FileContext) {
        let mut guard = self.one_at_time.lock().unwrap();
        *guard += 1;
        tracing::info!(?context, op=%guard, "start");
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
        let mut guard = self.one_at_time.lock().unwrap();
        *guard += 1;
        tracing::info!(op=%guard, "start");
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
        let mut guard = self.one_at_time.lock().unwrap();
        *guard += 1;
        tracing::info!(?context, ?path, op=%guard, "start");
    }

    #[instrument(skip_all)]
    fn flush(
        &self,
        context: Option<&Self::FileContext>,
        _file_info: &mut winfsp::filesystem::FileInfo,
    ) -> winfsp::Result<()> {
        let mut guard = self.one_at_time.lock().unwrap();
        *guard += 1;
        tracing::info!(?context, op=%guard, "start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn get_file_info(
        &self,
        context: &Self::FileContext,
        file_info: &mut winfsp::filesystem::FileInfo,
    ) -> winfsp::Result<()> {
        let mut guard = self.one_at_time.lock().unwrap();
        *guard += 1;
        tracing::info!(?context, op=%guard, "start");

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
        let mut guard = self.one_at_time.lock().unwrap();
        *guard += 1;
        tracing::info!(?context, op=%guard, "start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn set_security(
        &self,
        context: &Self::FileContext,
        _security_information: u32,
        _modification_descriptor: ModificationDescriptor,
    ) -> winfsp::Result<()> {
        let mut guard = self.one_at_time.lock().unwrap();
        *guard += 1;
        tracing::info!(?context, op=%guard, "start");
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
        let mut guard = self.one_at_time.lock().unwrap();
        *guard += 1;
        tracing::info!(?context, op=%guard, "start");
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
        let mut guard = self.one_at_time.lock().unwrap();
        *guard += 1;
        tracing::info!(?context, op=%guard, "start");
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
        let mut guard = self.one_at_time.lock().unwrap();
        *guard += 1;
        tracing::info!(?context, op=%guard, "start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn set_delete(
        &self,
        context: &Self::FileContext,
        _file_name: &winfsp::U16CStr,
        _delete_file: bool,
    ) -> winfsp::Result<()> {
        let mut guard = self.one_at_time.lock().unwrap();
        *guard += 1;
        tracing::info!(?context, op=%guard, "start");
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
        let mut guard = self.one_at_time.lock().unwrap();
        *guard += 1;
        tracing::info!(?context, op=%guard, "start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn read(
        &self,
        context: &Self::FileContext,
        _buffer: &mut [u8],
        _offset: u64,
    ) -> winfsp::Result<u32> {
        let mut guard = self.one_at_time.lock().unwrap();
        *guard += 1;
        tracing::info!(?context, op=%guard, "start");
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
        let mut guard = self.one_at_time.lock().unwrap();
        *guard += 1;
        tracing::info!(?context, op=%guard, "start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn get_dir_info_by_name(
        &self,
        context: &Self::FileContext,
        _file_name: &winfsp::U16CStr,
        _out_dir_info: &mut winfsp::filesystem::DirInfo,
    ) -> winfsp::Result<()> {
        let mut guard = self.one_at_time.lock().unwrap();
        *guard += 1;
        tracing::info!(?context, op=%guard, "start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn get_volume_info(
        &self,
        _out_volume_info: &mut winfsp::filesystem::VolumeInfo,
    ) -> winfsp::Result<()> {
        let mut guard = self.one_at_time.lock().unwrap();
        *guard += 1;
        tracing::info!(op=%guard, "start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn set_volume_label(
        &self,
        _volume_label: &winfsp::U16CStr,
        _volume_info: &mut winfsp::filesystem::VolumeInfo,
    ) -> winfsp::Result<()> {
        let mut guard = self.one_at_time.lock().unwrap();
        *guard += 1;
        tracing::info!(op=%guard, "start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn get_stream_info(
        &self,
        context: &Self::FileContext,
        _buffer: &mut [u8],
    ) -> winfsp::Result<u32> {
        let mut guard = self.one_at_time.lock().unwrap();
        *guard += 1;
        tracing::info!(?context, op=%guard, "start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn get_reparse_point_by_name(
        &self,
        _file_name: &winfsp::U16CStr,
        _is_directory: bool,
        _buffer: &mut [u8],
    ) -> winfsp::Result<u64> {
        let mut guard = self.one_at_time.lock().unwrap();
        *guard += 1;
        tracing::info!(op=%guard, "start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn get_reparse_point(
        &self,
        context: &Self::FileContext,
        _file_name: &winfsp::U16CStr,
        _buffer: &mut [u8],
    ) -> winfsp::Result<u64> {
        let mut guard = self.one_at_time.lock().unwrap();
        *guard += 1;
        tracing::info!(?context, op=%guard, "start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn set_reparse_point(
        &self,
        context: &Self::FileContext,
        _file_name: &winfsp::U16CStr,
        _buffer: &[u8],
    ) -> winfsp::Result<()> {
        let mut guard = self.one_at_time.lock().unwrap();
        *guard += 1;
        tracing::info!(?context, op=%guard, "start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn delete_reparse_point(
        &self,
        context: &Self::FileContext,
        _file_name: &winfsp::U16CStr,
        _buffer: &[u8],
    ) -> winfsp::Result<()> {
        let mut guard = self.one_at_time.lock().unwrap();
        *guard += 1;
        tracing::info!(?context, op=%guard, "start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn get_extended_attributes(
        &self,
        context: &Self::FileContext,
        _buffer: &mut [u8],
    ) -> winfsp::Result<u32> {
        let mut guard = self.one_at_time.lock().unwrap();
        *guard += 1;
        tracing::info!(?context, op=%guard, "start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn set_extended_attributes(
        &self,
        context: &Self::FileContext,
        _buffer: &[u8],
        _file_info: &mut winfsp::filesystem::FileInfo,
    ) -> winfsp::Result<()> {
        let mut guard = self.one_at_time.lock().unwrap();
        *guard += 1;
        tracing::info!(?context, op=%guard, "start");
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
        let mut guard = self.one_at_time.lock().unwrap();
        *guard += 1;
        tracing::info!(?context, op=%guard, "start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn dispatcher_stopped(&self, _normally: bool) {}
}
