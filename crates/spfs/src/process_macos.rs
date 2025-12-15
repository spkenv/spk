// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! macOS process ancestry tracking using libproc

use std::collections::HashSet;
use std::io;
use std::mem;
use std::os::fd::{AsRawFd, OwnedFd};
use std::ptr;

use libproc::bsd_info::BSDInfo;
use libproc::proc_pid::pidinfo;

/// Error type for process ancestry operations
#[derive(Debug, thiserror::Error)]
pub enum ProcessError {
    /// Failed to get process info
    #[error("Failed to get process info for PID {pid}: {message}")]
    InfoError { pid: i32, message: String },

    /// IO error
    #[error("IO error: {0}")]
    IoError(#[from] io::Error),
}

/// Get the parent PID of a given PID using libproc
pub fn get_parent_pid_for(pid: i32) -> Result<i32, ProcessError> {
    match pidinfo::<BSDInfo>(pid, 0) {
        Ok(info) => Ok(info.pbi_ppid as i32),
        Err(message) => Err(ProcessError::InfoError { pid, message }),
    }
}

/// Get the process ancestry chain from a given PID up to launchd (PID 1)
pub fn get_parent_pids_macos(root: Option<i32>) -> Result<Vec<i32>, ProcessError> {
    let mut current = root.unwrap_or_else(|| std::process::id() as i32);

    let mut stack = vec![current];
    const MAX_DEPTH: usize = 100;

    for _ in 0..MAX_DEPTH {
        let parent = match get_parent_pid_for(current) {
            Ok(ppid) => ppid,
            Err(_) => break,
        };

        if parent == 0 || parent == current || current == 1 {
            break;
        }

        stack.push(parent);
        current = parent;
    }

    Ok(stack)
}

/// Get the parent PID of *the current process*
pub fn get_parent_pid() -> Result<u32, ProcessError> {
    let ancestry = get_parent_pids_macos(None)?;

    ancestry
        .get(1)
        .map(|&pid| pid as u32)
        .ok_or_else(|| ProcessError::InfoError {
            pid: std::process::id() as i32,
            message: "No parent process found".into(),
        })
}

/// Check if caller_pid is within the ancestor tree of root_pid
pub fn is_in_process_tree(caller_pid: i32, root_pid: i32) -> bool {
    match get_parent_pids_macos(Some(caller_pid)) {
        Ok(ancestry) => ancestry.contains(&root_pid),
        Err(_) => false,
    }
}

/// Get all descendant PIDs of a given PID.
///
/// Uses libproc to walk the process tree and find all children, grandchildren, etc.
pub fn get_descendant_pids(root_pid: i32) -> Result<Vec<i32>, ProcessError> {
    let mut descendants = Vec::new();
    let mut to_check = vec![root_pid];
    let mut checked = HashSet::new();
    
    const MAX_ITERATIONS: usize = 1000; // Prevent infinite loops
    let mut iterations = 0;
    
    while let Some(pid) = to_check.pop() {
        if iterations >= MAX_ITERATIONS {
            tracing::warn!("get_descendant_pids hit iteration limit");
            break;
        }
        iterations += 1;
        
        if checked.contains(&pid) {
            continue;
        }
        checked.insert(pid);
        
        // Find all processes and check if they are children of this PID
        // Unfortunately libproc doesn't have a "get children" function,
        // so we need to scan all processes
        let all_pids = get_all_pids()?;
        
        for other_pid in all_pids {
            if let Ok(parent) = get_parent_pid_for(other_pid) {
                if parent == pid {
                    descendants.push(other_pid);
                    to_check.push(other_pid);
                }
            }
        }
    }
    
    Ok(descendants)
}

/// Get all process IDs on the system.
fn get_all_pids() -> Result<Vec<i32>, ProcessError> {
    // Get the number of PIDs - first call with null buffer to get count
    let ret = unsafe {
        libc::proc_listallpids(ptr::null_mut(), 0)
    };
    
    if ret < 0 {
        return Err(ProcessError::IoError(io::Error::last_os_error()));
    }
    
    let count = ret as usize;
    
    // Allocate buffer and get all PIDs
    let mut pids = vec![0i32; count];
    let bufsize = (pids.len() * mem::size_of::<i32>()) as i32;
    let ret = unsafe {
        libc::proc_listallpids(
            pids.as_mut_ptr() as *mut libc::c_void,
            bufsize,
        )
    };
    
    if ret < 0 {
        return Err(ProcessError::IoError(io::Error::last_os_error()));
    }
    
    pids.truncate(ret as usize);
    Ok(pids)
}

/// Watches PIDs for exit events using kqueue
pub struct ProcessWatcher {
    kq: OwnedFd,
    watched_pids: std::collections::HashSet<u32>,
}

impl ProcessWatcher {
    pub fn new() -> io::Result<Self> {
        use std::os::unix::io::FromRawFd;

        let fd = unsafe { libc::kqueue() };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }

        Ok(Self {
            kq: unsafe { OwnedFd::from_raw_fd(fd) },
            watched_pids: std::collections::HashSet::new(),
        })
    }

    /// Watch a PID for exit
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

        let res = unsafe {
            libc::kevent(
                self.kq.as_raw_fd(),
                &event,
                1,
                std::ptr::null_mut(),
                0,
                std::ptr::null(),
            )
        };

        if res < 0 {
            let err = io::Error::last_os_error();

            if err.raw_os_error() == Some(libc::ESRCH) {
                return Ok(false);
            }

            return Err(err);
        }

        self.watched_pids.insert(pid);
        Ok(true)
    }

    /// Remove watch
    pub fn unwatch(&mut self, pid: u32) -> bool {
        if !self.watched_pids.remove(&pid) {
            return false;
        }

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

    /// Block until some watched PID exits
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

        let res = unsafe {
            libc::kevent(
                self.kq.as_raw_fd(),
                std::ptr::null(),
                0,
                &mut event,
                1,
                &timeout_spec,
            )
        };

        if res < 0 {
            return Err(io::Error::last_os_error());
        }

        if res == 0 {
            return Ok(None);
        }

        let pid = event.ident as u32;
        self.watched_pids.remove(&pid);
        Ok(Some(pid))
    }

    pub fn is_process_alive(pid: u32) -> bool {
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skip_if_no_permission<T>(r: Result<T, ProcessError>) -> Option<T> {
        match r {
            Ok(v) => Some(v),
            Err(ProcessError::InfoError { message, .. })
                if message.contains("Operation not permitted")
                    || message.contains("permission denied")
                    || message.contains("EPERM") =>
            {
                eprintln!("Skipping test: no permission");
                None
            }
            Err(ProcessError::IoError(e)) if e.raw_os_error() == Some(libc::EPERM) => {
                eprintln!("Skipping test: no permission");
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
    fn test_is_in_process_tree_self() {
        let pid = std::process::id() as i32;
        let _ = is_in_process_tree(pid, pid); // should not panic
        if let Some(ancestry) = skip_if_no_permission(get_parent_pids_macos(Some(pid))) {
            assert!(ancestry.contains(&pid));
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
        assert!(watcher.watch(pid).unwrap());
        assert!(!watcher.watch(pid).unwrap());
    }

    #[test]
    fn test_process_watcher_watch_nonexistent_process() {
        let mut watcher = ProcessWatcher::new().unwrap();
        assert!(!watcher.watch(999_999).unwrap_or(false));
    }

    #[test]
    fn test_process_watcher_is_process_alive() {
        assert!(ProcessWatcher::is_process_alive(std::process::id()));
        assert!(!ProcessWatcher::is_process_alive(999_999));
    }

    #[test]
    fn test_child_process_ancestry() {
        let mut child = std::process::Command::new("sleep")
            .arg("0.1")
            .spawn()
            .expect("spawn failed");

        let child_pid = child.id() as i32;

        std::thread::sleep(std::time::Duration::from_millis(10));

        match get_parent_pids_macos(Some(child_pid)) {
            Ok(ancestry) => {
                assert!(ancestry.contains(&child_pid));
                let parent_pid = std::process::id() as i32;
                assert!(ancestry.contains(&parent_pid));
            }
            Err(ProcessError::InfoError { message, .. })
                if message.contains("Operation not permitted")
                    || message.contains("permission denied")
                    || message.contains("EPERM") =>
            {
                eprintln!("Skipping: no permission");
            }
            Err(e) => panic!("{e}"),
        }

        let _ = child.wait();
    }
}
