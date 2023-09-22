// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use libc::c_void;
use spfs::tracking::EnvSpec;
use tracing::instrument;
use windows::core::HRESULT;
use windows::Win32::Foundation::{CloseHandle, ERROR_NO_MORE_FILES};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32Next, PROCESSENTRY32, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::GetCurrentProcessId;
use winfsp::filesystem::{FileSystemContext, ModificationDescriptor};
use winfsp_sys::FILE_ACCESS_RIGHTS;

use super::{Handle, Mount, Result};

/// Routes filesystem operations based on a list of known mounts and
/// the calling process ID of each request.
///
/// The router is meant to be cheaply clonable so that the same instance
/// can be passed to winfsp as the filesystem but also used to manage
/// routes in the gRPC service.
#[derive(Clone)]
pub struct Router {
    repos: Vec<Arc<spfs::storage::RepositoryHandle>>,
    // TODO: rwlock is not ideal, as we'd like to be able to continue
    // uninterrupted when new filesystems are mounted
    routes: Arc<RwLock<HashMap<u32, Arc<Mount>>>>,
    default: Arc<Mount>,
}

impl Router {
    /// Construct an empty router with no mounted filesystem views
    pub async fn new(repos: Vec<Arc<spfs::storage::RepositoryHandle>>) -> spfs::Result<Self> {
        let default = Arc::new(Mount::new(
            tokio::runtime::Handle::current(),
            Vec::new(),
            spfs::tracking::Manifest::default(),
        )?);
        Ok(Self {
            repos,
            routes: Arc::new(RwLock::new(HashMap::default())),
            default,
        })
    }

    /// Add a new mount to this router, presenting the identified env_spec to
    /// the given process id and all its children
    pub async fn mount(&self, root_pid: u32, env_spec: EnvSpec) -> spfs::Result<()> {
        tracing::debug!("Computing environment manifest...");
        let mut manifest = Err(spfs::Error::UnknownReference(env_spec.to_string()));
        for repo in self.repos.iter() {
            manifest = spfs::compute_environment_manifest(&env_spec, &repo).await;
            if manifest.is_ok() {
                break;
            }
        }
        let manifest = manifest?;
        let rt = tokio::runtime::Handle::current();
        let mount = Mount::new(rt, self.repos.clone(), manifest)?;
        tracing::info!(%root_pid, env_spec=%env_spec.to_string(),"mounted");
        let mut routes = self.routes.write().expect("lock is never poisoned");
        if routes.contains_key(&root_pid) {
            return Err(spfs::Error::RuntimeExists(root_pid.to_string()));
        }
        routes.insert(root_pid, Arc::new(mount));
        Ok(())
    }

    fn get_calling_process(&self) -> u32 {
        // Safety: only valid when called from within the context of an active operation
        // as this information is stored in the local thread storage
        unsafe { winfsp_sys::FspFileSystemOperationProcessIdF() }
    }

    fn get_process_stack(&self) -> std::result::Result<Vec<u32>, winfsp::FspError> {
        let pid = self.get_calling_process();
        get_parent_pids(Some(pid))
    }

    fn get_filesystem_for_calling_process(&self) -> Result<Arc<Mount>> {
        let stack = self.get_process_stack()?;
        let routes = self.routes.read().expect("Lock is never poioned");
        for pid in stack {
            if let Some(mount) = routes.get(&pid).map(Arc::clone) {
                return Ok(mount);
            }
        }
        Ok(Arc::clone(&self.default))
    }
}

impl FileSystemContext for Router {
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
        tracing::debug!("recv");
        let mount = self.get_filesystem_for_calling_process()?;
        let res =
            mount.get_security_by_name(file_name, security_descriptor, resolve_reparse_points);
        tracing::debug!("done");
        res
    }

    #[instrument(skip_all)]
    fn open(
        &self,
        file_name: &winfsp::U16CStr,
        create_options: u32,
        granted_access: FILE_ACCESS_RIGHTS,
        file_info: &mut winfsp::filesystem::OpenFileInfo,
    ) -> Result<Self::FileContext> {
        tracing::debug!("recv");
        let mount = self.get_filesystem_for_calling_process()?;
        let res = mount.open(file_name, create_options, granted_access, file_info);
        tracing::debug!("done");
        res
    }

    #[instrument(skip_all)]
    fn read_directory(
        &self,
        context: &Self::FileContext,
        pattern: Option<&winfsp::U16CStr>,
        marker: winfsp::filesystem::DirMarker,
        buffer: &mut [u8],
    ) -> Result<u32> {
        tracing::debug!("recv");
        let mount = self.get_filesystem_for_calling_process()?;
        let res = mount.read_directory(context, pattern, marker, buffer);
        tracing::debug!("done");
        res
    }

    #[instrument(skip_all)]
    fn close(&self, context: Self::FileContext) {
        tracing::debug!("recv");
        let Ok(mount) = self.get_filesystem_for_calling_process() else {
            tracing::warn!("Failed to retrieve filesystem for calling process, and cannot fail");
            return;
        };
        mount.close(context);
        tracing::debug!("done");
    }

    #[instrument(skip_all)]
    fn create(
        &self,
        file_name: &winfsp::U16CStr,
        create_options: u32,
        granted_access: FILE_ACCESS_RIGHTS,
        file_attributes: winfsp_sys::FILE_FLAGS_AND_ATTRIBUTES,
        security_descriptor: Option<&[c_void]>,
        allocation_size: u64,
        extra_buffer: Option<&[u8]>,
        extra_buffer_is_reparse_point: bool,
        file_info: &mut winfsp::filesystem::OpenFileInfo,
    ) -> Result<Self::FileContext> {
        tracing::debug!("recv");
        let mount = self.get_filesystem_for_calling_process()?;
        let res = mount.create(
            file_name,
            create_options,
            granted_access,
            file_attributes,
            security_descriptor,
            allocation_size,
            extra_buffer,
            extra_buffer_is_reparse_point,
            file_info,
        );
        tracing::debug!("done");
        res
    }

    #[instrument(skip_all)]
    fn cleanup(
        &self,
        context: &Self::FileContext,
        file_name: Option<&winfsp::U16CStr>,
        flags: u32,
    ) {
        tracing::debug!("recv");
        let Ok(mount) = self.get_filesystem_for_calling_process() else {
            tracing::warn!("Failed to retrieve filesystem for calling process, and cannot fail");
            return;
        };
        mount.cleanup(context, file_name, flags);
        tracing::debug!("done");
    }

    #[instrument(skip_all)]
    fn flush(
        &self,
        context: Option<&Self::FileContext>,
        file_info: &mut winfsp::filesystem::FileInfo,
    ) -> Result<()> {
        tracing::debug!("recv");
        let mount = self.get_filesystem_for_calling_process()?;
        let res = mount.flush(context, file_info);
        tracing::debug!("done");
        res
    }

    #[instrument(skip_all)]
    fn get_file_info(
        &self,
        context: &Self::FileContext,
        file_info: &mut winfsp::filesystem::FileInfo,
    ) -> Result<()> {
        tracing::debug!("recv");
        let mount = self.get_filesystem_for_calling_process()?;
        let res = mount.get_file_info(context, file_info);
        tracing::debug!("done");
        res
    }

    #[instrument(skip_all)]
    fn get_security(
        &self,
        context: &Self::FileContext,
        security_descriptor: Option<&mut [c_void]>,
    ) -> Result<u64> {
        tracing::debug!("recv");
        let mount = self.get_filesystem_for_calling_process()?;
        let res = mount.get_security(context, security_descriptor);
        tracing::debug!("done");
        res
    }

    #[instrument(skip_all)]
    fn set_security(
        &self,
        context: &Self::FileContext,
        security_information: u32,
        modification_descriptor: ModificationDescriptor,
    ) -> Result<()> {
        tracing::debug!("recv");
        let mount = self.get_filesystem_for_calling_process()?;
        let res = mount.set_security(context, security_information, modification_descriptor);
        tracing::debug!("done");
        res
    }

    #[instrument(skip_all)]
    fn overwrite(
        &self,
        context: &Self::FileContext,
        file_attributes: winfsp_sys::FILE_FLAGS_AND_ATTRIBUTES,
        replace_file_attributes: bool,
        allocation_size: u64,
        extra_buffer: Option<&[u8]>,
        file_info: &mut winfsp::filesystem::FileInfo,
    ) -> Result<()> {
        tracing::debug!("recv");
        let mount = self.get_filesystem_for_calling_process()?;
        let res = mount.overwrite(
            context,
            file_attributes,
            replace_file_attributes,
            allocation_size,
            extra_buffer,
            file_info,
        );
        tracing::debug!("done");
        res
    }

    #[instrument(skip_all)]
    fn rename(
        &self,
        context: &Self::FileContext,
        file_name: &winfsp::U16CStr,
        new_file_name: &winfsp::U16CStr,
        replace_if_exists: bool,
    ) -> Result<()> {
        tracing::debug!("recv");
        let mount = self.get_filesystem_for_calling_process()?;
        let res = mount.rename(context, file_name, new_file_name, replace_if_exists);
        tracing::debug!("done");
        res
    }

    #[instrument(skip_all)]
    fn set_basic_info(
        &self,
        context: &Self::FileContext,
        file_attributes: u32,
        creation_time: u64,
        last_access_time: u64,
        last_write_time: u64,
        last_change_time: u64,
        file_info: &mut winfsp::filesystem::FileInfo,
    ) -> Result<()> {
        tracing::debug!("recv");
        let mount = self.get_filesystem_for_calling_process()?;
        let res = mount.set_basic_info(
            context,
            file_attributes,
            creation_time,
            last_access_time,
            last_write_time,
            last_change_time,
            file_info,
        );
        tracing::debug!("done");
        res
    }

    #[instrument(skip_all)]
    fn set_delete(
        &self,
        context: &Self::FileContext,
        file_name: &winfsp::U16CStr,
        delete_file: bool,
    ) -> Result<()> {
        tracing::debug!("recv");
        let mount = self.get_filesystem_for_calling_process()?;
        let res = mount.set_delete(context, file_name, delete_file);
        tracing::debug!("done");
        res
    }

    #[instrument(skip_all)]
    fn set_file_size(
        &self,
        context: &Self::FileContext,
        new_size: u64,
        set_allocation_size: bool,
        file_info: &mut winfsp::filesystem::FileInfo,
    ) -> Result<()> {
        tracing::debug!("recv");
        let mount = self.get_filesystem_for_calling_process()?;
        let res = mount.set_file_size(context, new_size, set_allocation_size, file_info);
        tracing::debug!("done");
        res
    }

    #[instrument(skip_all)]
    fn read(&self, context: &Self::FileContext, buffer: &mut [u8], offset: u64) -> Result<u32> {
        tracing::debug!("recv");
        let mount = self.get_filesystem_for_calling_process()?;
        let res = mount.read(context, buffer, offset);
        tracing::debug!("done");
        res
    }

    #[instrument(skip_all)]
    fn write(
        &self,
        context: &Self::FileContext,
        buffer: &[u8],
        offset: u64,
        write_to_eof: bool,
        constrained_io: bool,
        file_info: &mut winfsp::filesystem::FileInfo,
    ) -> Result<u32> {
        tracing::debug!("recv");
        let mount = self.get_filesystem_for_calling_process()?;
        let res = mount.write(
            context,
            buffer,
            offset,
            write_to_eof,
            constrained_io,
            file_info,
        );
        tracing::debug!("done");
        res
    }

    #[instrument(skip_all)]
    fn get_dir_info_by_name(
        &self,
        context: &Self::FileContext,
        file_name: &winfsp::U16CStr,
        out_dir_info: &mut winfsp::filesystem::DirInfo,
    ) -> Result<()> {
        tracing::debug!("recv");
        let mount = self.get_filesystem_for_calling_process()?;
        let res = mount.get_dir_info_by_name(context, file_name, out_dir_info);
        tracing::debug!("done");
        res
    }

    #[instrument(skip_all)]
    fn get_volume_info(&self, out_volume_info: &mut winfsp::filesystem::VolumeInfo) -> Result<()> {
        tracing::debug!("recv");
        let mount = self.get_filesystem_for_calling_process()?;
        let res = mount.get_volume_info(out_volume_info);
        tracing::debug!("done");
        res
    }

    #[instrument(skip_all)]
    fn set_volume_label(
        &self,
        volume_label: &winfsp::U16CStr,
        volume_info: &mut winfsp::filesystem::VolumeInfo,
    ) -> Result<()> {
        tracing::debug!("recv");
        let mount = self.get_filesystem_for_calling_process()?;
        let res = mount.set_volume_label(volume_label, volume_info);
        tracing::debug!("done");
        res
    }

    #[instrument(skip_all)]
    fn get_stream_info(&self, context: &Self::FileContext, buffer: &mut [u8]) -> Result<u32> {
        tracing::debug!("recv");
        let mount = self.get_filesystem_for_calling_process()?;
        let res = mount.get_stream_info(context, buffer);
        tracing::debug!("done");
        res
    }

    #[instrument(skip_all)]
    fn get_reparse_point_by_name(
        &self,
        file_name: &winfsp::U16CStr,
        is_directory: bool,
        buffer: &mut [u8],
    ) -> Result<u64> {
        tracing::debug!("recv");
        let mount = self.get_filesystem_for_calling_process()?;
        let res = mount.get_reparse_point_by_name(file_name, is_directory, buffer);
        tracing::debug!("done");
        res
    }

    #[instrument(skip_all)]
    fn get_reparse_point(
        &self,
        context: &Self::FileContext,
        file_name: &winfsp::U16CStr,
        buffer: &mut [u8],
    ) -> Result<u64> {
        tracing::debug!("recv");
        let mount = self.get_filesystem_for_calling_process()?;
        let res = mount.get_reparse_point(context, file_name, buffer);
        tracing::debug!("done");
        res
    }

    #[instrument(skip_all)]
    fn set_reparse_point(
        &self,
        context: &Self::FileContext,
        file_name: &winfsp::U16CStr,
        buffer: &[u8],
    ) -> Result<()> {
        tracing::debug!("recv");
        let mount = self.get_filesystem_for_calling_process()?;
        let res = mount.set_reparse_point(context, file_name, buffer);
        tracing::debug!("done");
        res
    }

    #[instrument(skip_all)]
    fn delete_reparse_point(
        &self,
        context: &Self::FileContext,
        file_name: &winfsp::U16CStr,
        buffer: &[u8],
    ) -> Result<()> {
        tracing::debug!("recv");
        let mount = self.get_filesystem_for_calling_process()?;
        let res = mount.delete_reparse_point(context, file_name, buffer);
        tracing::debug!("done");
        res
    }

    #[instrument(skip_all)]
    fn get_extended_attributes(
        &self,
        context: &Self::FileContext,
        buffer: &mut [u8],
    ) -> Result<u32> {
        tracing::debug!("recv");
        let mount = self.get_filesystem_for_calling_process()?;
        let res = mount.get_extended_attributes(context, buffer);
        tracing::debug!("done");
        res
    }

    #[instrument(skip_all)]
    fn set_extended_attributes(
        &self,
        context: &Self::FileContext,
        buffer: &[u8],
        file_info: &mut winfsp::filesystem::FileInfo,
    ) -> Result<()> {
        tracing::debug!("recv");
        let mount = self.get_filesystem_for_calling_process()?;
        let res = mount.set_extended_attributes(context, buffer, file_info);
        tracing::debug!("done");
        res
    }

    #[instrument(skip_all)]
    fn control(
        &self,
        context: &Self::FileContext,
        control_code: u32,
        input: &[u8],
        output: &mut [u8],
    ) -> Result<u32> {
        tracing::debug!("recv");
        let mount = self.get_filesystem_for_calling_process()?;
        let res = mount.control(context, control_code, input, output);
        tracing::debug!("done");
        res
    }

    #[instrument(skip_all)]
    fn dispatcher_stopped(&self, normally: bool) {
        tracing::debug!("recv");
        let Ok(mount) = self.get_filesystem_for_calling_process() else {
            tracing::warn!("Failed to retrieve filesystem for calling process, and cannot fail");
            return;
        };
        mount.dispatcher_stopped(normally);

        tracing::debug!("done");
    }
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
        if parent == &child {
            break;
        }
        stack.push(*parent);
        child = *parent;
    }
    let _ = unsafe { CloseHandle(snapshot) };
    Ok(stack)
}
