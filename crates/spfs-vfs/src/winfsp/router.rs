// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashMap;
use std::sync::Arc;

use libc::c_void;
use spfs::tracking::Entry;
use tracing::instrument;
use windows::core::HRESULT;
use windows::Win32::Foundation::{CloseHandle, ERROR_NO_MORE_FILES, STATUS_NOT_A_DIRECTORY};
use windows::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows::Win32::Security::PSECURITY_DESCRIPTOR;
use windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_DIRECTORY;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32Next, PROCESSENTRY32, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::SystemInformation::GetSystemTimeAsFileTime;
use windows::Win32::System::Threading::GetCurrentProcessId;
use winfsp::filesystem::{DirBuffer, ModificationDescriptor};
use winfsp_sys::FILE_ACCESS_RIGHTS;

use super::{Handle, Result};

/// Routes filesystem operations based on a list of known mounts and
/// the calling process ID of each request.
///
/// The router is meant to be cheaply clonable so that the same instance
/// can be passed to winfsp as the filesystem but also used to manage
/// routes in the gRPC service.
#[derive(Clone)]
pub struct Router {
    routes: Arc<dashmap::DashMap<u32, Arc<()>>>,
    security_descriptor: bytes::Bytes,
}

impl Router {
    /// Construct an empty router with no mounted filesystem views
    pub fn new() -> Result<Self> {
        let sddl = windows::core::w!("O:BAG:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FA;;;WD)");
        let mut psecurity_descriptor = PSECURITY_DESCRIPTOR(std::ptr::null_mut());
        let mut security_descriptor_size: u32 = 0;
        // Safety: all windows functions are unsafe, and so we are relying on this
        // being an appropriate use of the c++ bindings and calling the function as
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl,
                SDDL_REVISION_1,
                &mut psecurity_descriptor as *mut PSECURITY_DESCRIPTOR,
                Some(&mut security_descriptor_size as *mut u32),
            )?
        };

        // Safety: windows has allocated this pointer, which we are now going to take
        // owenership of so that it can be used and managed safely. Notably, the above function
        // creates a self-relative descriptor, which stores it's information in a contiguous
        // block of memory which we can safely copy for replication later on. This seems the
        // easiest way to create safe rust code that understands the thread safety of
        // this block of data.
        let descriptor_data = unsafe {
            std::slice::from_raw_parts(
                psecurity_descriptor.0 as *const u8,
                security_descriptor_size as usize,
            )
        };
        let security_descriptor = bytes::Bytes::copy_from_slice(descriptor_data);
        Ok(Self {
            routes: Arc::new(dashmap::DashMap::new()),
            security_descriptor,
        })
    }

    fn get_process_stack(&self) -> std::result::Result<Vec<u32>, winfsp::FspError> {
        // Safety: only valid when called from within the context of an active operation
        // as this information is stored in the local thread storage
        let pid = unsafe { winfsp_sys::FspFileSystemOperationProcessIdF() };
        get_parent_pids(Some(pid))
    }
}

impl winfsp::filesystem::FileSystemContext for Router {
    type FileContext = Handle;

    #[instrument(skip_all)]
    fn get_security_by_name(
        &self,
        file_name: &winfsp::U16CStr,
        security_descriptor: Option<&mut [c_void]>,
        resolve_reparse_points: impl FnOnce(
            &winfsp::U16CStr,
        ) -> Option<winfsp::filesystem::FileSecurity>,
    ) -> Result<winfsp::filesystem::FileSecurity> {
        let path = std::path::PathBuf::from(file_name.to_os_string());

        let stack = self.get_process_stack()?;
        tracing::trace!(?path, ?stack, security=%security_descriptor.is_some(),  "start");

        if let Some(security) = resolve_reparse_points(file_name.as_ref()) {
            return Ok(security);
        }

        // a path with no filename component is assumed to be the root path '\\'
        if path.file_name().is_some() {
            return Err(winfsp::FspError::IO(std::io::ErrorKind::NotFound));
        }

        let file_sec = winfsp::filesystem::FileSecurity {
            reparse: false,
            sz_security_descriptor: self.security_descriptor.len() as u64,
            attributes: FILE_ATTRIBUTE_DIRECTORY.0,
        };

        match security_descriptor {
            None => {}
            Some(descriptor) if descriptor.len() <= self.security_descriptor.len() => {
                // not enough space allocated for us to copy the descriptor, so
                // we will only return the size needed and not copy.
            }
            Some(descriptor) => unsafe {
                // enough space must be available in the provided buffer for us to
                // mutate/access it
                std::ptr::copy(
                    self.security_descriptor.as_ptr() as *const c_void,
                    descriptor.as_mut_ptr(),
                    self.security_descriptor.len(),
                )
            },
        }
        Ok(file_sec)
    }

    #[instrument(skip_all)]
    fn open(
        &self,
        file_name: &winfsp::U16CStr,
        create_options: u32,
        granted_access: FILE_ACCESS_RIGHTS,
        file_info: &mut winfsp::filesystem::OpenFileInfo,
    ) -> Result<Self::FileContext> {
        let path = std::path::PathBuf::from(file_name.to_os_string());
        tracing::info!(?path, ?granted_access, ?create_options, "start");

        let now = unsafe { GetSystemTimeAsFileTime() };
        let now = (now.dwHighDateTime as u64) << 32 | now.dwLowDateTime as u64;
        // a path with no filename component is assumed to be the root path '\\'
        let context = Handle::Tree {
            entry: Arc::new(Entry::empty_dir_with_open_perms_with_data(0)),
            dir_buffer: DirBuffer::new(),
        };
        let info = file_info.as_mut();
        info.file_attributes = FILE_ATTRIBUTE_DIRECTORY.0;
        info.index_number = context.ino();
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
    }

    #[instrument(skip_all)]
    fn read_directory(
        &self,
        context: &Self::FileContext,
        pattern: Option<&winfsp::U16CStr>,
        marker: winfsp::filesystem::DirMarker,
        buffer: &mut [u8],
    ) -> Result<u32> {
        let pattern = pattern.map(|p| p.to_os_string());
        tracing::info!(?context, ?marker, buffer=%buffer.len(), ?pattern,  "start");
        let Handle::Tree {
            entry: _,
            dir_buffer,
        } = context
        else {
            return Err(winfsp::FspError::NTSTATUS(STATUS_NOT_A_DIRECTORY));
        };
        let written = dir_buffer.read(marker, buffer);
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
    ) -> Result<Self::FileContext> {
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
    ) -> Result<()> {
        tracing::info!(?context, "start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn get_file_info(
        &self,
        context: &Self::FileContext,
        file_info: &mut winfsp::filesystem::FileInfo,
    ) -> Result<()> {
        tracing::info!(?context, "start");

        let now = unsafe { GetSystemTimeAsFileTime() };
        let now = (now.dwHighDateTime as u64) << 32 | now.dwLowDateTime as u64;
        file_info.file_attributes = FILE_ATTRIBUTE_DIRECTORY.0;
        file_info.index_number = context.ino();
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
    }

    #[instrument(skip_all)]
    fn get_security(
        &self,
        context: &Self::FileContext,
        _security_descriptor: Option<&mut [c_void]>,
    ) -> Result<u64> {
        tracing::info!(?context, "start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn set_security(
        &self,
        context: &Self::FileContext,
        _security_information: u32,
        _modification_descriptor: ModificationDescriptor,
    ) -> Result<()> {
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
    ) -> Result<()> {
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
    ) -> Result<()> {
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
    ) -> Result<()> {
        tracing::info!(?context, "start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn set_delete(
        &self,
        context: &Self::FileContext,
        _file_name: &winfsp::U16CStr,
        _delete_file: bool,
    ) -> Result<()> {
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
    ) -> Result<()> {
        tracing::info!(?context, "start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn read(&self, context: &Self::FileContext, _buffer: &mut [u8], _offset: u64) -> Result<u32> {
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
    ) -> Result<u32> {
        tracing::info!(?context, "start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn get_dir_info_by_name(
        &self,
        context: &Self::FileContext,
        _file_name: &winfsp::U16CStr,
        _out_dir_info: &mut winfsp::filesystem::DirInfo,
    ) -> Result<()> {
        tracing::info!(?context, "start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn get_volume_info(&self, _out_volume_info: &mut winfsp::filesystem::VolumeInfo) -> Result<()> {
        tracing::info!("start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn set_volume_label(
        &self,
        _volume_label: &winfsp::U16CStr,
        _volume_info: &mut winfsp::filesystem::VolumeInfo,
    ) -> Result<()> {
        tracing::info!("start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn get_stream_info(&self, context: &Self::FileContext, _buffer: &mut [u8]) -> Result<u32> {
        tracing::info!(?context, "start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn get_reparse_point_by_name(
        &self,
        _file_name: &winfsp::U16CStr,
        _is_directory: bool,
        _buffer: &mut [u8],
    ) -> Result<u64> {
        tracing::info!("start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn get_reparse_point(
        &self,
        context: &Self::FileContext,
        _file_name: &winfsp::U16CStr,
        _buffer: &mut [u8],
    ) -> Result<u64> {
        tracing::info!(?context, "start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn set_reparse_point(
        &self,
        context: &Self::FileContext,
        _file_name: &winfsp::U16CStr,
        _buffer: &[u8],
    ) -> Result<()> {
        tracing::info!(?context, "start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn delete_reparse_point(
        &self,
        context: &Self::FileContext,
        _file_name: &winfsp::U16CStr,
        _buffer: &[u8],
    ) -> Result<()> {
        tracing::info!(?context, "start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn get_extended_attributes(
        &self,
        context: &Self::FileContext,
        _buffer: &mut [u8],
    ) -> Result<u32> {
        tracing::info!(?context, "start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn set_extended_attributes(
        &self,
        context: &Self::FileContext,
        _buffer: &[u8],
        _file_info: &mut winfsp::filesystem::FileInfo,
    ) -> Result<()> {
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
    ) -> Result<u32> {
        tracing::info!(?context, "start");
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    #[instrument(skip_all)]
    fn dispatcher_stopped(&self, _normally: bool) {}
}

/// Return a list of pids such that the first pid is the root one
/// and each subsequent pid is the direct parent of the previous.
///
/// When `root` is None, the current process is used.
pub fn get_parent_pids(root: Option<u32>) -> std::result::Result<Vec<u32>, winfsp::FspError> {
    let mut child = match root {
        Some(pid) => pid,
        None => {
            // Safety: all windows API function are generated as unsafe
            // but this one should be infallible
            unsafe { GetCurrentProcessId() }
        }
    };
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
