// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use colored::*;

use crate::{encoding, storage, tracking, Result};

/// Specifies how a digest should be formatted
///
/// Some choices require access to the repository that
/// the digest was loaded from in order to resolve further
/// information
pub enum DigestFormat<'repo> {
    Full,
    Shortened(&'repo storage::RepositoryHandle),
    ShortenedWithTags(&'repo storage::RepositoryHandle),
}

impl<'repo> DigestFormat<'repo> {
    /// The repository handle contained in this enum, if any
    pub fn repository(&self) -> Option<&'repo storage::RepositoryHandle> {
        match self {
            Self::Full => None,
            Self::Shortened(r) => Some(r),
            Self::ShortenedWithTags(r) => Some(r),
        }
    }
}

/// Return a nicely formatted string representation of the given reference.
pub async fn format_digest(digest: encoding::Digest, format: DigestFormat<'_>) -> Result<String> {
    let mut all = match format.repository() {
        Some(repo) => {
            vec![repo.get_shortened_digest(digest).await]
        }
        None => {
            vec![digest.to_string()]
        }
    };

    if let DigestFormat::ShortenedWithTags(repo) = format {
        let mut aliases: Vec<_> = match repo.find_aliases(&digest.to_string()).await {
            Ok(aliases) => aliases
                .into_iter()
                .map(|r| r.to_string().dimmed().to_string())
                .collect(),
            // Formatting an invalid reference is strange, but not a good enough
            // reason to return an error at this point
            Err(crate::Error::InvalidReference(_)) => Default::default(),
            // this is unlikely to happen, if things are setup and called correctly
            // but in cases where this error does come up it's not a good enough reason
            // to fail this formatting process
            Err(crate::Error::UnknownReference(_)) => Default::default(),
            // we won't be able to find aliases, but can still continue
            // with formatting what we do have
            Err(crate::Error::AmbiguousReference(_)) => Default::default(),
            // this hints at deeper data integrity issues in the repository,
            // but that is not a good enough reason to bail out here
            Err(crate::Error::UnknownObject(_)) => Default::default(),
            Err(err) => return Err(err),
        };

        all.append(&mut aliases);
    }

    Ok(all.join(&" -> ".cyan().to_string()))
}

/// Return a human readable string rendering of the given diffs.
pub fn format_diffs<'a>(diffs: impl Iterator<Item = &'a tracking::Diff>) -> String {
    let mut outputs = Vec::new();
    for diff in diffs {
        let mut abouts = Vec::new();
        if let tracking::DiffMode::Changed(a, b) = &diff.mode {
            if a.mode != b.mode {
                abouts.push(format!("mode {{{:06o}=>{:06o}}}", a.mode, b.mode));
            }
            if a.object != b.object {
                abouts.push("content".to_string());
            }
            if a.size != b.size {
                abouts.push(format!("size {{{}=>{}}}", a.size, b.size));
            }
        }
        let about = if !abouts.is_empty() {
            format!(" [{}]", abouts.join(", ")).dimmed().to_string()
        } else {
            "".to_string()
        };
        let mut out = String::new();
        out += format!("{:>8}", diff.mode).bold().as_ref();
        out += format!("/spfs{}{about}", diff.path).as_ref();
        let out = match diff.mode {
            tracking::DiffMode::Added(..) => out.green(),
            tracking::DiffMode::Removed(..) => out.red(),
            tracking::DiffMode::Changed(..) => out.bright_blue(),
            _ => out.dimmed(),
        };
        outputs.push(out.to_string())
    }

    outputs.join("\n")
}

/// Return a string rendering of any given diffs which represent change.
pub fn format_changes<'a>(diffs: impl Iterator<Item = &'a tracking::Diff>) -> String {
    format_diffs(diffs.filter(|x| !x.mode.is_unchanged()))
}

/// Return a human-readable representation of the sync summary data.
pub fn format_sync_summary(summary: &super::sync::SyncSummary) -> String {
    let super::sync::SyncSummary {
        skipped_objects,
        synced_objects,
        skipped_payloads,
        synced_payloads,
        synced_payload_bytes,
    } = summary;

    format!(
        "synced {synced_objects} objects ({}) and {synced_payloads} payloads ({}) ({})",
        format!("{skipped_objects} skipped").dimmed(),
        format!("{skipped_payloads} skipped").dimmed(),
        format_size(*synced_payload_bytes)
    )
}

/// Return a human-readable file size in bytes.
pub fn format_size(size: u64) -> String {
    let mut size = size as f64;
    for unit in &["B", "Ki", "Mi", "Gi", "Ti"] {
        if size < 1024.0 {
            return format!("{size:3.1} {unit}");
        }
        size /= 1024.0;
    }
    format!("{size:3.1} Pi")
}
