// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! macOS process ancestry tracking using libproc

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
}
