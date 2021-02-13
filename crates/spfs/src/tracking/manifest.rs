use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;

use itertools::Itertools;
use relative_path::RelativePathBuf;

use super::entry::{Entry, EntryKind};
use crate::encoding;
use crate::runtime;
use crate::Result;

#[cfg(test)]
#[path = "./manifest_test.rs"]
mod manifest_test;

#[derive(Default, Debug, Eq, PartialEq, Clone)]
pub struct Manifest {
    root: Entry,
}

impl Manifest {
    pub fn new(root: Entry) -> Self {
        Self { root: root }
    }

    pub fn root<'a>(&'a self) -> &'a Entry {
        &self.root
    }

    pub fn take_root(self) -> Entry {
        self.root
    }

    /// Return true if this manifest has no contents.
    pub fn is_empty(&self) -> bool {
        self.root.entries.len() == 0
    }

    /// Get an entry in this manifest given it's filepath.
    pub fn get_path<'a, P: AsRef<str>>(&'a self, path: P) -> Option<&'a Entry> {
        const TRIM_START: &[char] = &['/', '.'];
        const TRIM_END: &[char] = &['/'];
        let path = path
            .as_ref()
            .trim_start_matches(TRIM_START)
            .trim_end_matches(TRIM_END);
        let mut entry = &self.root;
        if path.is_empty() {
            return Some(entry);
        }
        for step in path.split("/").into_iter() {
            if let EntryKind::Tree = entry.kind {
                let next = entry.entries.get(step);
                entry = match next {
                    Some(entry) => entry,
                    None => return None,
                };
            } else {
                return None;
            }
        }

        Some(entry)
    }

    /// List the contents of a directory in this manifest.
    ///
    /// None is returned if the directory does not exist or the provided entry is
    /// not a directory
    pub fn list_dir(&self, path: &str) -> Option<Vec<String>> {
        let entry = self.get_path(path)?;
        match entry.kind {
            EntryKind::Tree => Some(entry.entries.keys().map(|k| k.clone()).collect()),
            _ => None,
        }
    }

    /// Walk the contents of this manifest top-down and depth-first.
    pub fn walk<'m>(&'m self) -> ManifestWalker<'m> {
        ManifestWalker::new(&self.root)
    }

    /// Same as walk(), but joins all entry paths to the given root.
    pub fn walk_abs<'m, P: AsRef<str>>(&'m self, root: P) -> ManifestWalker<'m> {
        self.walk().with_prefix(root)
    }

    /// Walk the contents of this manifest bottom-up and depth-first.
    pub fn walk_up<'m>(&'m self) -> ManifestWalker<'m> {
        self.walk().set_upwards(true)
    }

    /// Same as walk_up(), but joins all entry paths to the given root.
    pub fn walk_up_abs<'m, P: AsRef<str>>(&'m self, root: P) -> ManifestWalker<'m> {
        self.walk_abs(root).set_upwards(true)
    }

    /// Add a new directory entry to this manifest
    pub fn mkdir<'m, P: AsRef<str>>(&'m mut self, path: P) -> Result<&'m mut Entry> {
        let entry = Entry::default();
        self.mknod(path, entry)
    }

    /// Ensure that all levels of the given directory name exist.
    ///
    /// Entries that do not exist are created with a resonable default
    /// file mode, but can and should be replaced by a new entry in the
    /// case where this is not desired.
    pub fn mkdirs<'m, P: AsRef<str>>(&'m mut self, path: P) -> Result<&'m mut Entry> {
        static TRIM_PAT: &[char] = &['/', '.'];
        let path = path.as_ref().trim_start_matches(TRIM_PAT);
        if path.is_empty() {
            return Err(nix::errno::Errno::EEXIST.into());
        }
        let path = RelativePathBuf::from(path).normalize();
        let mut entry = &mut self.root;
        for step in path.components() {
            match step {
                relative_path::Component::Normal(step) => {
                    let entries = &mut entry.entries;
                    if let None = entries.get_mut(step) {
                        entries.insert(step.to_string(), Entry::default());
                    }
                    entry = entries.get_mut(step).unwrap();
                    if !entry.kind.is_tree() {
                        return Err(nix::errno::Errno::ENOTDIR.into());
                    }
                }
                // do not expect any other components after normalizing
                _ => continue,
            }
        }
        Ok(entry)
    }

    /// Make a new file entry in this manifest
    pub fn mkfile<'m>(&'m mut self, path: &str) -> Result<&'m mut Entry> {
        let mut entry = Entry::default();
        entry.kind = EntryKind::Blob;
        self.mknod(path, entry)
    }

    pub fn mknod<'m, P: AsRef<str>>(
        &'m mut self,
        path: P,
        new_entry: Entry,
    ) -> Result<&'m mut Entry> {
        use relative_path::Component;
        static TRIM_PAT: &[char] = &['/', '.'];

        let path = path.as_ref().trim_start_matches(TRIM_PAT);
        if path.is_empty() {
            return Err(nix::errno::Errno::EEXIST.into());
        }
        let path = RelativePathBuf::from(path).normalize();
        let mut entry = &mut self.root;
        let mut components = path.components();
        let last = components.next_back();
        for step in components {
            match step {
                Component::Normal(step) => match entry.entries.get_mut(step) {
                    None => {
                        return Err(nix::errno::Errno::ENOENT.into());
                    }
                    Some(e) => {
                        if !e.kind.is_tree() {
                            return Err(nix::errno::Errno::ENOTDIR.into());
                        }
                        entry = e;
                    }
                },
                // do not expect any other components after normalizing
                _ => continue,
            }
        }
        match last {
            None => Err(nix::errno::Errno::ENOENT.into()),
            Some(Component::Normal(step)) => {
                entry.entries.insert(step.to_string(), new_entry);
                Ok(entry.entries.get_mut(step).unwrap())
            }
            _ => Err(nix::errno::Errno::EIO.into()),
        }
    }

    /// Layer another manifest on top of this one
    pub fn update(&mut self, other: &Self) {
        self.root.update(&other.root)
    }
}

/// Walks all entries in a manifest depth-first
pub struct ManifestWalker<'m> {
    upwards: bool,
    prefix: RelativePathBuf,
    children: std::collections::hash_map::Iter<'m, String, Entry>,
    active_child: Option<Box<ManifestWalker<'m>>>,
}

impl<'m> ManifestWalker<'m> {
    fn new(root: &'m Entry) -> Self {
        ManifestWalker {
            upwards: false,
            prefix: RelativePathBuf::from("/"),
            children: root.entries.iter(),
            active_child: None,
        }
    }

    /// Makes this walker walk upwards, yielding parent directories only after
    /// visiting all children
    fn set_upwards(mut self, upwards: bool) -> Self {
        self.upwards = upwards;
        self
    }

    fn with_prefix<P: AsRef<str>>(mut self, prefix: P) -> Self {
        self.prefix = RelativePathBuf::from(prefix.as_ref());
        self
    }
}

impl<'m> Iterator for ManifestWalker<'m> {
    type Item = ManifestNode<'m>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(active_child) = self.active_child.as_mut() {
            match active_child.next() {
                Some(next) => return Some(next),
                None => {
                    self.active_child = None;
                }
            }
        }

        match self.children.next() {
            None => None,
            Some((name, child)) => {
                if child.kind.is_tree() {
                    self.active_child = Some(
                        ManifestWalker::new(child)
                            .with_prefix(&self.prefix.join(name))
                            .into(),
                    );
                }
                Some(ManifestNode {
                    path: self.prefix.join(name),
                    entry: &child,
                })
            }
        }
    }
}

pub fn compute_manifest<P: AsRef<std::path::Path>>(path: P) -> Result<Manifest> {
    let mut builder = ManifestBuilder::default();
    builder.compute_manifest(path)
}

pub struct ManifestBuilder<'h> {
    hasher: Box<dyn FnMut(&mut std::fs::File) -> Result<encoding::Digest> + 'h>,
}

impl<'h> Default for ManifestBuilder<'h> {
    fn default() -> Self {
        Self::new(encoding::Digest::from_reader)
    }
}

impl<'h> ManifestBuilder<'h> {
    pub fn new(hasher: impl FnMut(&mut std::fs::File) -> Result<encoding::Digest> + 'h) -> Self {
        Self {
            hasher: Box::new(hasher),
        }
    }

    /// Build a manifest that describes a directorie's contents.
    pub fn compute_manifest<P: AsRef<std::path::Path>>(&mut self, path: P) -> Result<Manifest> {
        tracing::trace!("computing manifest for {:?}", path.as_ref());
        let mut manifest = Manifest::default();
        self.compute_tree_node(path, &mut manifest.root)?;
        Ok(manifest)
    }

    fn compute_tree_node<P: AsRef<std::path::Path>>(
        &mut self,
        dirname: P,
        tree_node: &mut Entry,
    ) -> Result<()> {
        tree_node.kind = EntryKind::Tree;
        for dir_entry in std::fs::read_dir(&dirname)? {
            let dir_entry = dir_entry?;
            let path = dirname.as_ref().join(dir_entry.file_name());
            let mut entry = Entry::default();
            self.compute_node(path, &mut entry)?;
            tree_node
                .entries
                .insert(dir_entry.file_name().to_string_lossy().to_string(), entry);
        }
        tree_node.size = tree_node.entries.len() as u64;
        Ok(())
    }

    fn compute_node<P: AsRef<std::path::Path>>(
        &mut self,
        path: P,
        entry: &mut Entry,
    ) -> Result<()> {
        let stat_result = std::fs::symlink_metadata(&path)?;

        entry.mode = stat_result.mode();
        entry.size = stat_result.size();

        let file_type = stat_result.file_type();
        if file_type.is_symlink() {
            let link_target = std::fs::read_link(&path)?;
            entry.kind = EntryKind::Blob;
            entry.object = encoding::Digest::from_reader(&mut link_target.as_os_str().as_bytes())?;
        } else if file_type.is_dir() {
            self.compute_tree_node(path, entry)?;
        } else if runtime::is_removed_entry(&stat_result) {
            entry.kind = EntryKind::Mask;
            entry.object = encoding::NULL_DIGEST.into();
        } else if !stat_result.is_file() {
            return Err(format!("unsupported special file: {:?}", path.as_ref()).into());
        } else {
            entry.kind = EntryKind::Blob;
            let mut reader = std::fs::File::open(path)?;
            entry.object = (self.hasher)(&mut reader)?;
        }
        Ok(())
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct ManifestNode<'a> {
    pub path: RelativePathBuf,
    pub entry: &'a Entry,
}

impl<'a> Ord for ManifestNode<'a> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use itertools::EitherOrBoth::{Both, Left, Right};
        use relative_path::Component::Normal;
        use std::cmp::Ordering;

        let self_path = self.path.normalize();
        let other_path = other.path.normalize();
        let mut path_iter = self_path
            .components()
            .zip_longest(other_path.components())
            .peekable();

        loop {
            let item = path_iter.next();
            if let Some(item) = item {
                // we only expect normal path components here due to the fact that
                // we are normalizing the path before iteration, any '.' or '..' entries
                // will mess with this comparison process.
                match item {
                    Both(Normal(left), Normal(right)) => {
                        let kinds = match path_iter.peek() {
                            Some(Both(Normal(_), Normal(_))) => (EntryKind::Tree, EntryKind::Tree),
                            Some(Left(_)) => (EntryKind::Tree, other.entry.kind),
                            Some(Right(_)) => (self.entry.kind, EntryKind::Tree),
                            _ => (self.entry.kind, other.entry.kind),
                        };
                        // let the entry type take precedence over any name
                        // - this is to ensure directories are sorted first
                        let cmp = match kinds.1.cmp(&kinds.0) {
                            Ordering::Equal => left.cmp(right),
                            cmp => cmp,
                        };
                        if let Ordering::Equal = cmp {
                            continue;
                        }
                        return cmp;
                    }
                    Left(_) => {
                        return std::cmp::Ordering::Greater;
                    }
                    Right(_) => {
                        return std::cmp::Ordering::Less;
                    }
                    _ => continue,
                }
            } else {
                break;
            }
        }
        std::cmp::Ordering::Equal
    }
}

impl<'a> PartialOrd for ManifestNode<'a> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
