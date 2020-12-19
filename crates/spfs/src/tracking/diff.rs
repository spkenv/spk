use std::collections::HashSet;

use relative_path::RelativePathBuf;

use super::{Entry, Manifest};

#[cfg(test)]
#[path = "./diff_test.rs"]
mod diff_test;

/// Identifies the style of difference between two file system entries
#[derive(Debug, Eq, PartialEq)]
pub enum DiffMode {
    Unchanged,
    Changed,
    Added,
    Removed,
}

impl std::fmt::Display for DiffMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unchanged => f.write_str("="),
            Self::Changed => f.write_str("~"),
            Self::Added => f.write_str("+"),
            Self::Removed => f.write_str("-"),
        }
    }
}

impl DiffMode {
    pub fn is_unchanged(&self) -> bool {
        if let Self::Unchanged = self {
            true
        } else {
            false
        }
    }
    pub fn is_changed(&self) -> bool {
        if let Self::Changed = self {
            true
        } else {
            false
        }
    }
    pub fn is_added(&self) -> bool {
        if let Self::Added = self {
            true
        } else {
            false
        }
    }
    pub fn is_removed(&self) -> bool {
        if let Self::Removed = self {
            true
        } else {
            false
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct Diff {
    pub mode: DiffMode,
    pub path: RelativePathBuf,
    pub entries: Option<(Entry, Entry)>,
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
        match self.entries.as_ref() {
            None => (),
            Some((a, b)) => {
                if a.mode != b.mode {
                    details = format!("{} {{{:06o} => {:06o}}}", details, a.mode, b.mode);
                }
                if a.kind != b.kind {
                    details = format!("{} {{{} => {}}}", details, a.kind, b.kind);
                }
                if a.object != b.object {
                    details = format!("{} {{!object!}}", details);
                }
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
            changes.push(diff_path(a, b, &entry.path));
        }
    }

    changes
}

fn diff_path(a: &Manifest, b: &Manifest, path: &RelativePathBuf) -> Diff {
    match (a.get_path(&path), b.get_path(&path)) {
        (None, None) => Diff {
            mode: DiffMode::Unchanged,
            path: path.clone(),
            entries: None,
        },

        (None, Some(_)) => Diff {
            mode: DiffMode::Added,
            path: path.clone(),
            entries: None,
        },

        (Some(_), None) => Diff {
            mode: DiffMode::Removed,
            path: path.clone(),
            entries: None,
        },

        (Some(a_entry), Some(b_entry)) => {
            if a_entry == b_entry {
                Diff {
                    mode: DiffMode::Unchanged,
                    path: path.clone(),
                    entries: None,
                }
            } else {
                Diff {
                    mode: DiffMode::Changed,
                    path: path.clone(),
                    entries: Some((a_entry.clone(), b_entry.clone())),
                }
            }
        }
    }
}
