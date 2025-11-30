// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::HashSet;
#[cfg(target_os = "linux")]
use std::io::{BufRead, BufReader};
use std::os::unix::fs::MetadataExt;

#[cfg(target_os = "linux")]
use crate::env::OVERLAY_ARGS_LOWERDIR_APPEND;
#[cfg(target_os = "linux")]
use crate::{Error, Result};

#[cfg(test)]
#[path = "./overlayfs_test.rs"]
mod overlayfs_test;

pub fn is_removed_entry(meta: &std::fs::Metadata) -> bool {
    // overlayfs uses character device files to denote
    // a file that was removed, using this special file
    // as a whiteout file of the same name.
    // Cast to u32 to handle platform differences (mode_t is u16 on macOS, u32 on Linux)
    if meta.mode() as u32 & libc::S_IFCHR as u32 == 0 {
        return false;
    }
    // - the device is always 0/0 for a whiteout file
    meta.rdev() == 0
}

/// Get the set of supported overlayfs arguments on this machine
#[cfg(target_os = "linux")]
#[cached::proc_macro::once(sync_writes = true)]
pub fn overlayfs_available_options() -> HashSet<String> {
    query_overlayfs_available_options().unwrap_or_else(|err| {
        if std::env::var("SPFS_SUPPRESS_OVERLAYFS_PARAMS_WARNING").is_err() {
            tracing::warn!("Failed to detect supported overlayfs params: {err}");
            tracing::warn!(" > Falling back to the most conservative set, which is undesirable");
            tracing::warn!(
                " > To suppress this warning, set SPFS_SUPPRESS_OVERLAYFS_PARAMS_WARNING=1"
            );
        }
        Default::default()
    })
}

/// Get the set of supported overlayfs arguments on this machine.
///
/// On macOS, overlayfs is not supported, so this returns an empty set.
#[cfg(target_os = "macos")]
pub fn overlayfs_available_options() -> HashSet<String> {
    // overlayfs is not available on macOS
    HashSet::new()
}

/// Read available overlayfs settings from the kernel
#[cfg(target_os = "linux")]
fn query_overlayfs_available_options() -> Result<HashSet<String>> {
    let output = std::process::Command::new("/sbin/modinfo")
        .arg("overlay")
        .output()
        .map_err(|err| Error::process_spawn_error("/sbin/modinfo", err, None))?;

    if output.status.code().unwrap_or(1) != 0 {
        return Err(Error::OverlayFsNotInstalled);
    }

    parse_modinfo_params(&mut BufReader::new(output.stdout.as_slice()))
}

/// Parses the available parameters from the output of `modinfo` for a kernel module
#[cfg(target_os = "linux")]
fn parse_modinfo_params<R: BufRead>(reader: &mut R) -> Result<HashSet<String>> {
    let mut params = HashSet::new();
    let mut vermagic_seen = false;
    for line in reader.lines() {
        let line = line.map_err(|err| {
            Error::String(format!("Failed to read kernel module information: {err}"))
        })?;

        // The output from "modinfo overlay" looks like this:
        // ...
        // vermagic:       6.12.9-amd64 SMP preempt mod_unload modversions
        // ...
        // parm:           metacopy:Default to on or off for the metadata only copy up feature (bool)
        //
        // The "vermagic:" line appears before the "parm:" lines.

        if !vermagic_seen {
            let version_string = match line.strip_prefix("vermagic:") {
                Some(remainder) => remainder.trim(),
                None => continue,
            };
            vermagic_seen = true;
            let mut parts = version_string.splitn(3, '.'); // ("6", "12", "9-...")
            let major_version = parts.next().unwrap_or("0").parse().unwrap_or(0);
            let minor_version = parts.next().unwrap_or("0").parse().unwrap_or(0);
            // The "lowerdir+" option was added in Linux v6.8.
            // https://docs.kernel.org/6.8/filesystems/overlayfs.html#multiple-lower-layers
            if major_version >= 7 || (major_version == 6 && minor_version >= 8) {
                params.insert(OVERLAY_ARGS_LOWERDIR_APPEND.to_string());
            }
            continue;
        }
        let param = match line.strip_prefix("parm:") {
            Some(remainder) => remainder.trim(),
            None => continue,
        };
        let name = match param.split_once(':') {
            Some((name, _remainder)) => name,
            None => param,
        };
        params.insert(name.to_owned());
    }

    Ok(params)
}
