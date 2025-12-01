// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! macOS process ancestry tracking using libproc

use std::os::fd::{AsRawFd, OwnedFd};
use std::io;

use libproc::libproc::bsd_info::BSDInfo;
use libproc::libproc::proc_pid::pidinfo;

/// Error type for process ancestry operations
#[derive(Debug, thiserror::Error)]
pub enum ProcessError {
    /// Failed to get process info
    #[error("Failed to get process info for PID {pid}: {message}")]
    InfoError {
        /// The process ID that failed
        pid: i32,
        /// The error message
        message: String,
    },
}

/// Get the process ancestry chain from a given PID up to launchd (PID 1)
///
/// Returns a vector starting with the given PID, followed by its parent,
/// grandparent, etc., up to PID 1 (launchd).
pub fn get_parent_pids_macos(root: Option<i32>) -> Result<Vec<i32>, ProcessError> {
    let mut current = match root {
        Some(pid) => pid,
        None => std::process::id() as i32,
    };

    let mut stack = vec![current];
    const MAX_DEPTH: usize = 100;

    for _ in 0..MAX_DEPTH {
        let info: BSDInfo = pidinfo(current, 0)
            .map_err(|e| ProcessError::InfoError {
                pid: current,
                message: e.to_string(),
            })?;

        let parent = info.pbi_ppid as i32;

        // Stop at launchd (PID 1) or if parent == self (orphan)
        if parent == 0 || parent == current || current == 1 {
            break;
        }

        stack.push(parent);
        current = parent;
    }

    Ok(stack)
}

/// Check if caller_pid is a descendant of root_pid
pub fn is_in_process_tree(caller_pid: i32, root_pid: i32) -> bool {
    match get_parent_pids_macos(Some(caller_pid)) {
        Ok(ancestry) => ancestry.contains(&root_pid),
        Err(_) => false,
    }
}

/// Get the parent PID of the current process
pub fn get_parent_pid() -> Result<u32, ProcessError> {
    let ancestry = get_parent_pids_macos(None)?;
    ancestry
        .get(1)
        .map(|&pid| pid as u32)
        .ok_or_else(|| ProcessError::InfoError {
            pid: std::process::id() as i32,
            message: "No parent process found".to_string(),
        })
}

/// Watches a set of process IDs for exit events using kqueue.
///
/// On macOS, kqueue can efficiently monitor process exit via EVFILT_PROC
/// with NOTE_EXIT flag.
pub struct ProcessWatcher {
    kq: OwnedFd,
    watched_pids: std::collections::HashSet<u32>,
}

impl ProcessWatcher {
    /// Create a new process watcher.
    pub fn new() -> io::Result<Self> {
        use std::os::unix::io::FromRawFd;
        
        let kq = unsafe {
            let fd = libc::kqueue();
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }
            OwnedFd::from_raw_fd(fd)
        };
        
        Ok(Self {
            kq,
            watched_pids: std::collections::HashSet::new(),
        })
    }
    
    /// Add a process ID to watch for exit.
    ///
    /// Returns Ok(true) if the PID was added, Ok(false) if already watched,
    /// or Err if the process doesn't exist or can't be watched.
    pub fn watch(&mut self, pid: u32) -> io::Result<bool> {
        if self.watched_pids.contains(&pid) {
            return Ok(false);
        }
        
        let event = libc::kevent {
            ident: pid as usize,
            filter: libc::EVFILT_PROC,
            flags: libc::EV_ADD | libc::EV_ONESHOT,
            fflags: libc::NOTE_EXIT,
            data: 0,
            udata: std::ptr::null_mut(),
        };
        
        let result = unsafe {
            libc::kevent(
                self.kq.as_raw_fd(),
                &event,
                1,
                std::ptr::null_mut(),
                0,
                std::ptr::null(),
            )
        };
        
        if result < 0 {
            let err = io::Error::last_os_error();
            // ESRCH means process doesn't exist - not an error, just already exited
            if err.raw_os_error() == Some(libc::ESRCH) {
                return Ok(false);
            }
            return Err(err);
        }
        
        self.watched_pids.insert(pid);
        Ok(true)
    }
    
    /// Stop watching a process ID.
    pub fn unwatch(&mut self, pid: u32) -> bool {
        if !self.watched_pids.remove(&pid) {
            return false;
        }
        
        // Remove from kqueue (best effort - process may have already exited)
        let event = libc::kevent {
            ident: pid as usize,
            filter: libc::EVFILT_PROC,
            flags: libc::EV_DELETE,
            fflags: 0,
            data: 0,
            udata: std::ptr::null_mut(),
        };
        
        unsafe {
            libc::kevent(
                self.kq.as_raw_fd(),
                &event,
                1,
                std::ptr::null_mut(),
                0,
                std::ptr::null(),
            );
        }
        
        true
    }
    
    /// Wait for any watched process to exit.
    ///
    /// Returns the PID that exited, or None on timeout.
    pub fn wait_for_exit(&mut self, timeout: std::time::Duration) -> io::Result<Option<u32>> {
        let timeout_spec = libc::timespec {
            tv_sec: timeout.as_secs() as i64,
            tv_nsec: timeout.subsec_nanos() as i64,
        };
        
        let mut event = libc::kevent {
            ident: 0,
            filter: 0,
            flags: 0,
            fflags: 0,
            data: 0,
            udata: std::ptr::null_mut(),
        };
        
        let result = unsafe {
            libc::kevent(
                self.kq.as_raw_fd(),
                std::ptr::null(),
                0,
                &mut event,
                1,
                &timeout_spec,
            )
        };
        
        if result < 0 {
            return Err(io::Error::last_os_error());
        }
        
        if result == 0 {
            // Timeout
            return Ok(None);
        }
        
        let pid = event.ident as u32;
        self.watched_pids.remove(&pid);
        Ok(Some(pid))
    }
    
    /// Check if a specific process is still running.
    pub fn is_process_alive(pid: u32) -> bool {
        unsafe {
            libc::kill(pid as i32, 0) == 0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // NOTE: These tests require process inspection permissions on macOS.
    // When running in a restricted environment (e.g., sandboxed terminal,
    // certain CI systems), libproc may return "Operation not permitted".
    // We handle this by skipping the test rather than failing.

    fn skip_if_no_permission<T>(result: Result<T, ProcessError>) -> Option<T> {
        match result {
            Ok(v) => Some(v),
            Err(ProcessError::InfoError { message, .. })
                if message.contains("Operation not permitted") =>
            {
                eprintln!("Skipping test: process inspection not permitted in this environment");
                None
            }
            Err(e) => panic!("Unexpected error: {e}"),
        }
    }

    #[test]
    fn test_current_process_ancestry() {
        let Some(ancestry) = skip_if_no_permission(get_parent_pids_macos(None)) else {
            return;
        };
        assert!(!ancestry.is_empty());
        assert_eq!(ancestry[0], std::process::id() as i32);
    }

    #[test]
    fn test_ancestry_reaches_launchd() {
        let Some(ancestry) = skip_if_no_permission(get_parent_pids_macos(None)) else {
            return;
        };
        let last = *ancestry.last().unwrap();
        assert!(last == 1 || ancestry.len() == 100);
    }

    #[test]
    fn test_is_in_process_tree_self() {
        // is_in_process_tree returns false on permission error, which is fine
        // We can only test that the function doesn't panic
        let pid = std::process::id() as i32;
        let _ = is_in_process_tree(pid, pid);
        // If we have permission, verify it works correctly
        if let Some(ancestry) = skip_if_no_permission(get_parent_pids_macos(Some(pid))) {
            assert!(ancestry.contains(&pid));
        }
    }

    #[test]
    fn test_is_in_process_tree_parent() {
        let Some(ancestry) = skip_if_no_permission(get_parent_pids_macos(None)) else {
            return;
        };
        if ancestry.len() > 1 {
            let current = ancestry[0];
            let parent = ancestry[1];
            assert!(is_in_process_tree(current, parent));
        }
    }

    #[test]
    fn test_get_parent_pid() {
        let Some(parent) = skip_if_no_permission(get_parent_pid()) else {
            return;
        };
        assert!(parent > 0);
    }

    #[test]
    fn test_process_watcher_watch_current_process() {
        let mut watcher = ProcessWatcher::new().unwrap();
        let pid = std::process::id();
        // Should succeed - we're watching ourselves
        assert!(watcher.watch(pid).unwrap());
        // Should return false - already watching
        assert!(!watcher.watch(pid).unwrap());
    }
    
    #[test]
    fn test_process_watcher_watch_nonexistent_process() {
        let mut watcher = ProcessWatcher::new().unwrap();
        // Use a PID that's very unlikely to exist
        let fake_pid = 999999;
        // Should return false (process doesn't exist) without error
        assert!(!watcher.watch(fake_pid).unwrap_or(false));
    }
    
    #[test]
    fn test_process_watcher_is_process_alive() {
        assert!(ProcessWatcher::is_process_alive(std::process::id()));
        assert!(!ProcessWatcher::is_process_alive(999999));
    }
}
