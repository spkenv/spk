// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::os::windows::prelude::FileExt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use dashmap::DashMap;
use libc::c_void;
use spfs::tracking::{Entry, EntryKind};
use tokio::io::AsyncReadExt;
use windows::Win32::Foundation::{ERROR_SEEK_ON_DEVICE, STATUS_NOT_A_DIRECTORY};
use windows::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW,
    SDDL_REVISION_1,
};
use windows::Win32::Security::PSECURITY_DESCRIPTOR;
use windows::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DIRECTORY,
    FILE_ATTRIBUTE_NORMAL,
    FILE_ATTRIBUTE_NOT_CONTENT_INDEXED,
    FILE_ATTRIBUTE_READONLY,
    FILE_ATTRIBUTE_REPARSE_POINT,
};
use windows::Win32::System::SystemInformation::GetSystemTimeAsFileTime;
use winfsp::filesystem::{DirBuffer, DirInfo, ModificationDescriptor, WideNameInfo};
use winfsp_sys::FILE_ACCESS_RIGHTS;

use super::{Handle, Result};

const ROOT_INODE: u64 = 0;

/// A filesystem implementation for WinFSP that presents an existing
/// spfs manifest as read-only
pub struct Mount {
    rt: tokio::runtime::Handle,
    repos: Vec<Arc<spfs::storage::RepositoryHandle>>,
    manifest: spfs::tracking::Manifest,
    security_descriptor: bytes::Bytes,
    next_inode: AtomicU64,
    inodes: DashMap<u64, Arc<Entry<u64>>>,
}

/// Send a winfsp error and return
macro_rules! err {
    ($send:ident, $err:expr) => {{
        let err = $err;
        tracing::error!("{err:?}");
        let errno = err
            .raw_os_error()
            .unwrap_or(windows::Win32::Foundation::ERROR_BUSY.0 as i32);
        let _ = $send.send(Err(winfsp::FspError::WIN32(
            windows::Win32::Foundation::WIN32_ERROR(errno as u32),
        )));
        return;
    }};
}

impl Mount {
    /// Construct a mount that presents the given manifest
    pub fn new(
        rt: tokio::runtime::Handle,
        repos: Vec<Arc<spfs::storage::RepositoryHandle>>,
        manifest: spfs::tracking::Manifest,
    ) -> spfs::Result<Self> {
        /// This syntax describes the default security descriptor settings
        /// that are used for files and directories in the mounted file system.
        /// It essentially provides a sane default ownership as well as
        /// read/write access to all users.
        /// More information about the SDD language and syntax can be found here:
        /// https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-dtyp/4f4251cc-23b6-44b6-93ba-69688422cb06
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
        let fs = Self {
            rt,
            repos,
            manifest,
            security_descriptor,
            next_inode: AtomicU64::new(ROOT_INODE),
            inodes: Default::default(),
        };
        // pre-allocate inodes for all entries in the manifest
        let mut root = fs.manifest.clone().take_root();
        // often manifests do not have appropriate mode bits set
        // at the root because they are not captured from the
        // actual directory upon commit. If we don't properly
        // report this mode as a directory, the filesystem
        // will appear broken
        root.mode |= libc::S_IFDIR as u32;
        fs.allocate_inodes(root);
        Ok(fs)
    }

    fn allocate_inode(&self) -> u64 {
        self.next_inode.fetch_add(1, Ordering::Relaxed)
    }

    fn allocate_inodes(&self, entry: Entry) -> Arc<Entry<u64>> {
        let Entry {
            kind,
            object,
            mode,
            size,
            entries,
            user_data: _,
        } = entry;

        let inode = self.allocate_inode();
        let entries = entries
            .into_iter()
            .map(|(n, e)| (n, self.allocate_inodes(e).as_ref().clone()))
            .collect();
        let entry = Arc::new(Entry {
            kind,
            object,
            mode,
            size,
            entries,
            user_data: inode,
        });
        self.inodes.insert(inode, Arc::clone(&entry));
        entry
    }

    fn attr_from_entry(&self, entry: &Entry<u64>) -> u32 {
        let mut attrs = match entry.kind {
            EntryKind::Blob if entry.is_symlink() => FILE_ATTRIBUTE_REPARSE_POINT.0,
            EntryKind::Blob => FILE_ATTRIBUTE_NORMAL.0,
            EntryKind::Tree => FILE_ATTRIBUTE_DIRECTORY.0,
            // we do not allocate nodes for mask files
            EntryKind::Mask => unreachable!(),
        };
        attrs |= FILE_ATTRIBUTE_NOT_CONTENT_INDEXED.0 | FILE_ATTRIBUTE_READONLY.0;
        attrs
    }

    fn inode_from_path(&self, path: &winfsp::U16CStr) -> Option<Arc<Entry<u64>>> {
        let path = std::path::PathBuf::from(path.to_os_string());
        let Ok(relative) = path.strip_prefix(r"\\") else {
            return None;
        };
        let Some(str_path) = relative.to_str() else {
            return None;
        };

        const TRIM_START: &[char] = &['/', '.'];
        const TRIM_END: &[char] = &['/'];
        let path = str_path.replace('\\', "/");
        let path = path
            .trim_start_matches(TRIM_START)
            .trim_end_matches(TRIM_END);
        let mut entry = self
            .inodes
            .get(&ROOT_INODE)
            .map(|kv| Arc::clone(kv.value()));
        if path.is_empty() {
            return entry;
        }
        for step in path.split('/') {
            let Some(current) = entry.take() else {
                return None;
            };
            let EntryKind::Tree = current.kind else {
                return None;
            };
            let Some(child) = current.entries.get(step) else {
                return None;
            };
            entry = self
                .inodes
                .get(&child.user_data)
                .map(|kv| Arc::clone(kv.value()));
        }

        entry
    }
}

impl winfsp::filesystem::FileSystemContext for Mount {
    type FileContext = Handle;

    fn get_security_by_name(
        &self,
        file_name: &winfsp::U16CStr,
        security_descriptor: Option<&mut [c_void]>,
        resolve_reparse_points: impl FnOnce(
            &winfsp::U16CStr,
        ) -> Option<winfsp::filesystem::FileSecurity>,
    ) -> Result<winfsp::filesystem::FileSecurity> {
        if let Some(security) = resolve_reparse_points(file_name) {
            return Ok(security);
        }

        let Some(entry) = self.inode_from_path(file_name) else {
            return Err(winfsp::FspError::IO(std::io::ErrorKind::NotFound));
        };

        if entry.kind == EntryKind::Mask {
            return Err(winfsp::FspError::IO(std::io::ErrorKind::NotFound));
        };

        let attributes = self.attr_from_entry(&entry);
        let file_sec = winfsp::filesystem::FileSecurity {
            reparse: entry.is_symlink(),
            sz_security_descriptor: self.security_descriptor.len() as u64,
            attributes,
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

    fn open(
        &self,
        file_name: &winfsp::U16CStr,
        _create_options: u32,
        _granted_access: FILE_ACCESS_RIGHTS,
        file_info: &mut winfsp::filesystem::OpenFileInfo,
    ) -> winfsp::Result<Self::FileContext> {
        let Some(entry) = self.inode_from_path(file_name) else {
            return Err(winfsp::FspError::IO(std::io::ErrorKind::NotFound));
        };

        let attributes = self.attr_from_entry(&entry);
        let now = unsafe { GetSystemTimeAsFileTime() };
        let now = (now.dwHighDateTime as u64) << 32 | now.dwLowDateTime as u64;
        let info = file_info.as_mut();
        info.file_attributes = attributes;
        info.index_number = entry.user_data;
        info.file_size = entry.size;
        info.ea_size = 0;
        info.creation_time = now;
        info.change_time = now;
        info.last_access_time = now;
        info.last_write_time = now;
        info.hard_links = 0;
        info.reparse_tag = 0;

        if entry.is_dir() {
            return Ok(Handle::Tree { entry });
        }

        let (send, recv) = tokio::sync::oneshot::channel();
        let repos = self.repos.clone();
        let digest = entry.object;
        self.rt.spawn(async move {
            for repo in repos.into_iter() {
                match &*repo {
                    spfs::storage::RepositoryHandle::FS(fs_repo) => {
                        let Ok(fs_repo) = fs_repo.opened().await else {
                            let _ =
                                send.send(Err(winfsp::FspError::IO(std::io::ErrorKind::NotFound)));
                            return;
                        };
                        let payload_path = fs_repo.payloads.build_digest_path(&digest);
                        match std::fs::OpenOptions::new().read(true).open(payload_path) {
                            Ok(file) => {
                                let _ = send.send(Ok(Some(Handle::BlobFile { entry, file })));
                                return;
                            }
                            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                                continue;
                            }
                            Err(err) => err!(send, err),
                        }
                    }
                    repo => match repo.open_payload(digest).await {
                        Ok((stream, _)) => {
                            // TODO: try to leverage the returned file path?
                            let _ = send.send(Ok(Some(Handle::BlobStream {
                                entry,
                                offset: Arc::new(AtomicU64::new(0)),
                                stream: Arc::new(tokio::sync::Mutex::new(stream)),
                            })));
                            // TODO: are there attribute flags to identify this as a non-seekable file?
                            return;
                        }
                        Err(spfs::Error::UnknownObject(_)) => continue,
                        Err(err) => err!(send, err),
                    },
                }
            }
            let _ = send.send(Ok(None));
        });

        let handle = tokio::task::block_in_place(move || recv.blocking_recv())
            .expect("the task does not panic")?;
        let Some(handle) = handle else {
            return Err(winfsp::FspError::IO(std::io::ErrorKind::NotFound));
        };

        Ok(handle)
    }

    fn read_directory(
        &self,
        context: &Self::FileContext,
        pattern: Option<&winfsp::U16CStr>,
        marker: winfsp::filesystem::DirMarker,
        buffer: &mut [u8],
    ) -> Result<u32> {
        // TODO: this pattern should be checked
        let _pattern = pattern.map(|p| p.to_os_string());
        let Handle::Tree { entry } = context else {
            return Err(winfsp::FspError::NTSTATUS(STATUS_NOT_A_DIRECTORY));
        };
        let dir_buffer = DirBuffer::new();
        if let Ok(dir_buffer) = dir_buffer.acquire(true, Some(entry.entries.len() as u32)) {
            let mut dir_info = DirInfo::<255>::default();
            let now = unsafe { GetSystemTimeAsFileTime() };
            let now = (now.dwHighDateTime as u64) << 32 | now.dwLowDateTime as u64;
            for (name, entry) in entry.entries.iter() {
                if let Some(inner) = marker.inner_as_cstr() {
                    // to support chunked reads, only process entries after
                    // the name held by the provided marker
                    if inner.to_string_lossy() >= *name {
                        continue;
                    }
                }
                dir_info.set_name(name)?;
                let info = dir_info.file_info_mut();
                let attributes = self.attr_from_entry(entry);
                info.file_attributes = attributes;
                info.index_number = entry.user_data;
                info.file_size = entry.size;
                info.ea_size = 0;
                info.creation_time = now;
                info.change_time = now;
                info.last_access_time = now;
                info.last_write_time = now;
                info.hard_links = 0;
                info.reparse_tag = 0;
                dir_buffer.write(&mut dir_info)?;
            }
        }
        Ok(dir_buffer.read(marker, buffer))
    }

    fn close(&self, _context: Self::FileContext) {}

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
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    fn cleanup(
        &self,
        _context: &Self::FileContext,
        _file_name: Option<&winfsp::U16CStr>,
        _flags: u32,
    ) {
    }

    fn flush(
        &self,
        _context: Option<&Self::FileContext>,
        _file_info: &mut winfsp::filesystem::FileInfo,
    ) -> Result<()> {
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    fn get_file_info(
        &self,
        context: &Self::FileContext,
        file_info: &mut winfsp::filesystem::FileInfo,
    ) -> winfsp::Result<()> {
        let entry = context.entry();
        let now = unsafe { GetSystemTimeAsFileTime() };
        let now = (now.dwHighDateTime as u64) << 32 | now.dwLowDateTime as u64;
        file_info.file_attributes = self.attr_from_entry(entry);
        file_info.index_number = entry.user_data;
        file_info.file_size = 0;
        file_info.ea_size = 0;
        file_info.creation_time = now;
        file_info.change_time = now;
        file_info.last_access_time = now;
        file_info.last_write_time = now;
        file_info.hard_links = 0;
        file_info.reparse_tag = 0;
        Ok(())
    }

    fn get_security(
        &self,
        _context: &Self::FileContext,
        _security_descriptor: Option<&mut [c_void]>,
    ) -> Result<u64> {
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    fn set_security(
        &self,
        _context: &Self::FileContext,
        _security_information: u32,
        _modification_descriptor: ModificationDescriptor,
    ) -> Result<()> {
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    fn overwrite(
        &self,
        _context: &Self::FileContext,
        _file_attributes: winfsp_sys::FILE_FLAGS_AND_ATTRIBUTES,
        _replace_file_attributes: bool,
        _allocation_size: u64,
        _extra_buffer: Option<&[u8]>,
        _file_info: &mut winfsp::filesystem::FileInfo,
    ) -> Result<()> {
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    fn rename(
        &self,
        _context: &Self::FileContext,
        _file_name: &winfsp::U16CStr,
        _new_file_name: &winfsp::U16CStr,
        _replace_if_exists: bool,
    ) -> Result<()> {
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    fn set_basic_info(
        &self,
        _context: &Self::FileContext,
        _file_attributes: u32,
        _creation_time: u64,
        _last_access_time: u64,
        _last_write_time: u64,
        _last_change_time: u64,
        _file_info: &mut winfsp::filesystem::FileInfo,
    ) -> Result<()> {
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    fn set_delete(
        &self,
        _context: &Self::FileContext,
        _file_name: &winfsp::U16CStr,
        _delete_file: bool,
    ) -> Result<()> {
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    fn set_file_size(
        &self,
        _context: &Self::FileContext,
        _new_size: u64,
        _set_allocation_size: bool,
        _file_info: &mut winfsp::filesystem::FileInfo,
    ) -> Result<()> {
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    fn read(&self, context: &Self::FileContext, buffer: &mut [u8], offset: u64) -> Result<u32> {
        match context {
            Handle::BlobFile { entry: _, file } => Ok(file.seek_read(buffer, offset)? as u32),
            Handle::BlobStream {
                entry: _,
                stream,
                offset: last_offset,
            } => {
                let last_offset = Arc::clone(last_offset);
                let stream = Arc::clone(stream);
                let res = self.rt.block_on(async move {
                    let mut stream = stream.lock().await;
                    // load the offset only after we have received the mutex lock
                    // to ensure that it is validated and modified atomically
                    let last = last_offset.load(Ordering::Relaxed);
                    if offset != last {
                        // TODO: these are meant to be normal files, not device files
                        // so it's not clear that this is an appropriate error
                        return Err(winfsp::FspError::WIN32(ERROR_SEEK_ON_DEVICE));
                    }
                    let read = stream.read(buffer).await?;
                    last_offset.fetch_add(read as u64, Ordering::Relaxed);
                    Ok(read)
                });
                Ok(res? as u32)
            }
            Handle::Tree { entry: _ } => {
                Err(windows::Win32::Foundation::STATUS_FILE_IS_A_DIRECTORY.into())
            }
        }
    }

    fn write(
        &self,
        _context: &Self::FileContext,
        _buffer: &[u8],
        _offset: u64,
        _write_to_eof: bool,
        _constrained_io: bool,
        _file_info: &mut winfsp::filesystem::FileInfo,
    ) -> Result<u32> {
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    fn get_dir_info_by_name(
        &self,
        _context: &Self::FileContext,
        _file_name: &winfsp::U16CStr,
        _out_dir_info: &mut winfsp::filesystem::DirInfo,
    ) -> Result<()> {
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    fn get_volume_info(&self, _out_volume_info: &mut winfsp::filesystem::VolumeInfo) -> Result<()> {
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    fn set_volume_label(
        &self,
        _volume_label: &winfsp::U16CStr,
        _volume_info: &mut winfsp::filesystem::VolumeInfo,
    ) -> Result<()> {
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    fn get_stream_info(&self, _context: &Self::FileContext, _buffer: &mut [u8]) -> Result<u32> {
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    fn get_reparse_point_by_name(
        &self,
        _file_name: &winfsp::U16CStr,
        _is_directory: bool,
        _buffer: &mut [u8],
    ) -> Result<u64> {
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    fn get_reparse_point(
        &self,
        _context: &Self::FileContext,
        _file_name: &winfsp::U16CStr,
        _buffer: &mut [u8],
    ) -> Result<u64> {
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    fn set_reparse_point(
        &self,
        _context: &Self::FileContext,
        _file_name: &winfsp::U16CStr,
        _buffer: &[u8],
    ) -> Result<()> {
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    fn delete_reparse_point(
        &self,
        _context: &Self::FileContext,
        _file_name: &winfsp::U16CStr,
        _buffer: &[u8],
    ) -> Result<()> {
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    fn get_extended_attributes(
        &self,
        _context: &Self::FileContext,
        _buffer: &mut [u8],
    ) -> Result<u32> {
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    fn set_extended_attributes(
        &self,
        _context: &Self::FileContext,
        _buffer: &[u8],
        _file_info: &mut winfsp::filesystem::FileInfo,
    ) -> Result<()> {
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    fn control(
        &self,
        _context: &Self::FileContext,
        _control_code: u32,
        _input: &[u8],
        _output: &mut [u8],
    ) -> Result<u32> {
        Err(windows::Win32::Foundation::STATUS_INVALID_DEVICE_REQUEST.into())
    }

    fn dispatcher_stopped(&self, _normally: bool) {}
}
