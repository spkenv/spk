// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashSet;

use relative_path::RelativePathBuf;

use super::{Entry, EntryKind, Manifest};

#[cfg(test)]
#[path = "./diff_test.rs"]
mod diff_test;

/// Identifies a difference between two file system entries
#[derive(Debug, Eq, PartialEq, Clone)]
pub enum DiffMode {
    Unchanged(Entry),
    Changed(Entry, Entry),
    Added(Entry),
    Removed(Entry),
}

impl std::fmt::Display for DiffMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unchanged(..) => f.write_str("="),
            Self::Changed(..) => f.write_str("~"),
            Self::Added(..) => f.write_str("+"),
            Self::Removed(..) => f.write_str("-"),
        }
    }
}

impl DiffMode {
    pub fn is_unchanged(&self) -> bool {
        matches!(self, Self::Unchanged(..))
    }
    pub fn is_changed(&self) -> bool {
        matches!(self, Self::Changed(..))
    }
    pub fn is_added(&self) -> bool {
        matches!(self, Self::Added(..))
    }
    pub fn is_removed(&self) -> bool {
        matches!(self, Self::Removed(..))
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct Diff {
    pub mode: DiffMode,
    pub path: RelativePathBuf,
}

impl std::fmt::Display for Diff {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "{} {}{}",
            self.mode,
            self.path,
            self.details()
        ))
    }
}

impl Diff {
    fn details(&self) -> String {
        let mut details = String::new();
        if let DiffMode::Changed(a, b) = &self.mode {
            if a.mode != b.mode {
                details = format!("{details} {{{:06o} => {:06o}}}", a.mode, b.mode);
            }
            if a.kind != b.kind {
                details = format!("{details} {{{} => {}}}", a.kind, b.kind);
            }
            if a.object != b.object {
                details = format!("{details} {{!content!}}");
            }
        }
        details
    }
}

pub fn compute_diff(a: &Manifest, b: &Manifest) -> Vec<Diff> {
    let mut changes = Vec::new();
    let mut all_entries: Vec<_> = a.walk().chain(b.walk()).collect();
    all_entries.sort();

    let mut visited = HashSet::new();
    for entry in all_entries.iter() {
        if visited.contains(&entry.path) {
            continue;
        } else {
            visited.insert(&entry.path);
            match diff_path(a, b, &entry.path) {
                Some(d) => changes.push(d),
                None => tracing::debug!(
                    "path was missing from both manifests during diff, this should be impossible"
                ),
            }
        }
    }

    changes
}

fn diff_path(a: &Manifest, b: &Manifest, path: &RelativePathBuf) -> Option<Diff> {
    match (a.get_path(path), b.get_path(path)) {
        (None, None) => None,

        (_, Some(b_entry)) if b_entry.kind == EntryKind::Mask => Some(Diff {
            mode: DiffMode::Removed(b_entry.clone()),
            path: path.clone(),
        }),

        (None, Some(e)) => Some(Diff {
            mode: DiffMode::Added(e.clone()),
            path: path.clone(),
        }),

        (Some(e), None) => Some(Diff {
            mode: DiffMode::Removed(e.clone()),
            path: path.clone(),
        }),

        (Some(a_entry), Some(b_entry)) => Some({
            if a_entry == b_entry {
                Diff {
                    mode: DiffMode::Unchanged(b_entry.clone()),
                    path: path.clone(),
                }
            } else {
                Diff {
                    mode: DiffMode::Changed(a_entry.clone(), b_entry.clone()),
                    path: path.clone(),
                }
            }
        }),
    }
}
