// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::{HashMap, HashSet};
use std::os::unix::fs::MetadataExt;
use std::os::unix::prelude::FileTypeExt;
use std::pin::Pin;
use std::sync::Arc;

use futures::future::ready;
use futures::{FutureExt, StreamExt, TryFutureExt, TryStreamExt};
use itertools::Itertools;
use relative_path::{RelativePath, RelativePathBuf};
use tokio::fs::DirEntry;
use tokio::sync::Semaphore;

use super::entry::{Entry, EntryKind};
use super::{BlobRead, BlobReadExt, Diff};
use crate::{encoding, runtime, Error, Result};

#[cfg(test)]
#[path = "./manifest_test.rs"]
mod manifest_test;

/// The default limit for concurrent blobs when computing manifests.
/// See: [`ManifestBuilder::with_max_concurrent_blobs`]
pub const DEFAULT_MAX_CONCURRENT_BLOBS: usize = 1000;

/// The default limit for concurrent branches when computing manifests.
/// See: [`ManifestBuilder::with_max_concurrent_branches`]
pub const DEFAULT_MAX_CONCURRENT_BRANCHES: usize = 5;

#[derive(Default, Clone)]
pub struct Manifest<T = ()> {
    root: Entry<T>,
}

impl<T> std::fmt::Debug for Manifest<T>
where
    T: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Manifest")
            .field("root", &self.root)
            .finish()
    }
}

impl<T> std::cmp::PartialEq for Manifest<T>
where
    T: std::cmp::PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.root == other.root
    }
}

impl<T> std::cmp::Eq for Manifest<T> where T: std::cmp::Eq {}

impl<T> Manifest<T> {
    pub fn new(root: Entry<T>) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Entry<T> {
        &self.root
    }

    pub fn take_root(self) -> Entry<T> {
        self.root
    }

    /// Return true if this manifest has no contents.
    pub fn is_empty(&self) -> bool {
        self.root.entries.len() == 0
    }

    /// Get an entry in this manifest given it's filepath.
    pub fn get_path<P: AsRef<str>>(&self, path: P) -> Option<&Entry<T>> {
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
        for step in path.split('/') {
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

    /// List the names in a directory in this manifest.
    ///
    /// None is returned if the directory does not exist or the provided entry is
    /// not a directory
    pub fn list_dir(&self, path: &str) -> Option<impl Iterator<Item = &String>> {
        let entry = self.get_path(path)?;
        match entry.kind {
            EntryKind::Tree => Some(entry.entries.keys()),
            _ => None,
        }
    }

    /// List the contents of a directory in this manifest.
    ///
    /// None is returned if the directory does not exist or the provided entry is
    /// not a directory
    pub fn read_dir(&self, path: &str) -> Option<impl Iterator<Item = (&String, &Entry<T>)>> {
        let entry = self.get_path(path)?;
        match entry.kind {
            EntryKind::Tree => Some(entry.entries.iter()),
            _ => None,
        }
    }
}

impl<T> Manifest<T>
where
    T: std::cmp::Eq + std::cmp::PartialEq,
{
    /// Convert this manifest into its encodable,
    /// hashable form for storage.
    pub fn to_graph_manifest(&self) -> crate::graph::Manifest {
        self.into()
    }
}

impl<T> Manifest<T>
where
    T: Clone,
{
    /// Same as list_dir() but instead,
    /// lists the entries that exist inside the directory.
    ///
    /// None is also returned if the entry is not a directory.
    pub fn list_entries_in_dir(&self, path: &str) -> Option<&HashMap<String, Entry<T>>> {
        let entry = self.get_path(path)?;
        match entry.kind {
            EntryKind::Tree => Some(&entry.entries),
            _ => None,
        }
    }

    /// Layer another manifest on top of this one
    pub fn update(&mut self, other: &Self) {
        self.root.update(&other.root)
    }
}

impl<T> Manifest<T>
where
    T: Eq + PartialEq,
{
    /// Walk the contents of this manifest top-down and depth-first.
    pub fn walk(&self) -> ManifestWalker<'_, T> {
        ManifestWalker::new(&self.root)
    }

    /// Same as walk(), but joins all entry paths to the given root.
    pub fn walk_abs<P: Into<RelativePathBuf>>(&self, root: P) -> ManifestWalker<'_, T> {
        self.walk().with_prefix(root)
    }
}

impl<T> Manifest<T>
where
    T: Default,
{
    pub fn list_entries_in_dir(&self, path: &str) -> Vec<&String> {
        let target_entry = self.find_entry_by_string(path);
        let entries_in_dir = target_entry.entries.keys().collect_vec();

        entries_in_dir
    }

    /// Finds entry given entry name. If nothing is found will return the root entry.
    pub fn find_entry_by_string(&self, entry: &str) -> &Entry {
        let paths: Vec<String> = entry.split('/').map(str::to_string).collect();
        let mut matched_entry = &self.root;
        for path in paths.iter() {
            matched_entry = match matched_entry.entries.get(path) {
                Some(entry) => entry,
                _ => continue,
            };
        }

        matched_entry
    }

    /// Add a new directory entry to this manifest
    pub fn mkdir<P: AsRef<str>>(&mut self, path: P) -> Result<&mut Entry<T>> {
        let entry = Entry::default();
        self.mknod(path, entry)
    }

    /// Ensure that all levels of the given directory name exist.
    ///
    /// Entries that do not exist are created with a reasonable default
    /// file mode, but can and should be replaced by a new entry in the
    /// case where this is not desired.
    pub fn mkdirs<P: AsRef<str>>(&mut self, path: P) -> Result<&mut Entry<T>> {
        const TRIM_PAT: &[char] = &['/', '.'];
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
                    if entries.get_mut(step).is_none() {
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
    pub fn mkfile<'m>(&'m mut self, path: &str) -> Result<&'m mut Entry<T>> {
        let entry = Entry {
            kind: EntryKind::Blob,
            ..Default::default()
        };
        self.mknod(path, entry)
    }
}

impl<T> Manifest<T> {
    pub fn mknod<P: AsRef<str>>(&mut self, path: P, new_entry: Entry<T>) -> Result<&mut Entry<T>> {
        use relative_path::Component;
        const TRIM_PAT: &[char] = &['/', '.'];

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
}

/// Walks all entries in a manifest depth-first
pub struct ManifestWalker<'m, T = ()> {
    prefix: RelativePathBuf,
    children: std::collections::hash_map::Iter<'m, String, Entry<T>>,
    active_child: Option<Box<ManifestWalker<'m, T>>>,
}

impl<'m, T> ManifestWalker<'m, T> {
    fn new(root: &'m Entry<T>) -> Self {
        ManifestWalker {
            prefix: RelativePathBuf::from("/"),
            children: root.entries.iter(),
            active_child: None,
        }
    }

    fn with_prefix<P: Into<RelativePathBuf>>(mut self, prefix: P) -> Self {
        self.prefix = prefix.into();
        self
    }
}

impl<'m, T> Iterator for ManifestWalker<'m, T>
where
    T: Eq + PartialEq,
{
    type Item = ManifestNode<'m, T>;

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
                    entry: child,
                })
            }
        }
    }
}

#[async_trait::async_trait]
pub trait BlobHasher {
    /// Read the contents of `reader` to completion, returning
    /// the digest of the contents.
    async fn hash_blob(&self, reader: Pin<Box<dyn BlobRead>>) -> Result<encoding::Digest>;
}

#[tonic::async_trait]
impl BlobHasher for () {
    async fn hash_blob(&self, reader: Pin<Box<dyn BlobRead>>) -> Result<encoding::Digest> {
        Ok(encoding::Digest::from_async_reader(reader).await?)
    }
}

pub async fn compute_manifest<P: AsRef<std::path::Path> + Send>(path: P) -> Result<Manifest> {
    let builder = ManifestBuilder::new();
    builder.compute_manifest(path).await
}

/// Used to include/exclude paths from a manifest
/// while it's being constructed
pub trait PathFilter {
    fn should_include_path(&self, path: &RelativePath) -> bool;
}

impl PathFilter for () {
    fn should_include_path(&self, _path: &RelativePath) -> bool {
        true
    }
}

impl PathFilter for HashSet<&RelativePath> {
    fn should_include_path(&self, path: &RelativePath) -> bool {
        self.contains(path)
    }
}

impl<F> PathFilter for F
where
    F: Fn(&RelativePath) -> bool,
{
    fn should_include_path(&self, path: &RelativePath) -> bool {
        (self)(path)
    }
}

impl PathFilter for &[Diff] {
    fn should_include_path(&self, path: &RelativePath) -> bool {
        for diff in self.iter() {
            if diff.path == path || diff.path.starts_with(path) {
                return true;
            }
        }
        false
    }
}

/// Computes manifests from directory structures on disk
pub struct ManifestBuilder<H = (), F = (), R = ()>
where
    H: BlobHasher + Send + Sync,
    F: PathFilter + Send + Sync,
    R: ComputeManifestReporter,
{
    hasher: H,
    filter: F,
    reporter: R,
    blob_semaphore: Arc<Semaphore>,
    max_concurrent_branches: usize,
}

impl ManifestBuilder<(), (), ()> {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for ManifestBuilder<(), (), ()> {
    fn default() -> Self {
        Self {
            hasher: (),
            filter: (),
            reporter: (),
            blob_semaphore: Arc::new(Semaphore::new(DEFAULT_MAX_CONCURRENT_BLOBS)),
            max_concurrent_branches: DEFAULT_MAX_CONCURRENT_BRANCHES,
        }
    }
}

impl<H, F, R> ManifestBuilder<H, F, R>
where
    H: BlobHasher + Send + Sync,
    F: PathFilter + Send + Sync,
    R: ComputeManifestReporter,
{
    /// Set how many blobs should be processed at once.
    pub fn with_max_concurrent_blobs(mut self, max_concurrent_blobs: usize) -> Self {
        self.blob_semaphore = Arc::new(Semaphore::new(max_concurrent_blobs));
        self
    }

    /// Set how many branches should be processed at once.
    ///
    /// Each tree/folder that is processed can have any number of subtrees. This number
    /// limits the number of subtrees that can be processed at once for any given tree. This
    /// means that the number compounds exponentially based on the depth of the manifest
    /// being computed. Eg: a limit of 2 allows two directories to be processed in the root
    /// simultaneously and a further 2 within each of those two for a total of 4 branches, and so
    /// on. When computing for extremely deep trees, a smaller, conservative number is better
    /// to avoid open file limits.
    pub fn with_max_concurrent_branches(mut self, max_concurrent_branches: usize) -> Self {
        self.max_concurrent_branches = max_concurrent_branches;
        self
    }

    /// Use the provided hasher when building the manifest.
    ///
    /// The hasher turns blob contents into a digest to be included
    /// in the manifest. This is useful in commit-like operations where
    /// it might be beneficial to write the data while hashing and
    /// avoid needing to read the content again later.
    pub fn with_blob_hasher<H2>(self, hasher: H2) -> ManifestBuilder<H2, F, R>
    where
        H2: BlobHasher + Send + Sync,
    {
        ManifestBuilder {
            hasher,
            filter: self.filter,
            reporter: self.reporter,
            blob_semaphore: self.blob_semaphore,
            max_concurrent_branches: self.max_concurrent_branches,
        }
    }

    /// Set a filter on the builder so that only files matched by the filter
    /// will be included in the manifest.
    ///
    /// The filter is expected to match paths that are relative to the
    /// `$PREFIX` root, eg: `directory/filename` rather than
    /// `/spfs/directory/filename`.
    pub fn with_path_filter<F2>(self, filter: F2) -> ManifestBuilder<H, F2, R>
    where
        F2: PathFilter + Send + Sync,
    {
        ManifestBuilder {
            hasher: self.hasher,
            filter,
            reporter: self.reporter,
            blob_semaphore: self.blob_semaphore,
            max_concurrent_branches: self.max_concurrent_branches,
        }
    }

    /// Use the given [`ComputeManifestReporter`] when running, replacing any existing one.
    pub fn with_reporter<R2>(self, reporter: R2) -> ManifestBuilder<H, F, R2>
    where
        R2: ComputeManifestReporter,
    {
        ManifestBuilder {
            hasher: self.hasher,
            filter: self.filter,
            reporter,
            blob_semaphore: self.blob_semaphore,
            max_concurrent_branches: self.max_concurrent_branches,
        }
    }

    /// Build a manifest that describes a directory's contents.
    pub async fn compute_manifest<P: AsRef<std::path::Path> + Send>(
        &self,
        path: P,
    ) -> Result<Manifest> {
        tracing::trace!("computing manifest for {:?}", path.as_ref());
        let mut manifest = Manifest::default();
        manifest.root = self
            .compute_tree_node(
                Arc::new(path.as_ref().to_owned()),
                path.as_ref(),
                manifest.root,
            )
            .await?;
        Ok(manifest)
    }

    #[async_recursion::async_recursion]
    async fn compute_tree_node<P: AsRef<std::path::Path> + Send>(
        &self,
        root: Arc<std::path::PathBuf>,
        dirname: P,
        mut tree_node: Entry,
    ) -> Result<Entry> {
        tree_node.kind = EntryKind::Tree;
        let base = dirname.as_ref();
        let read_dir = tokio::fs::read_dir(base).await.map_err(|err| {
            Error::StorageReadError("read_dir of tree node", base.to_owned(), err)
        })?;
        let mut stream = tokio_stream::wrappers::ReadDirStream::new(read_dir)
            .map_err(|err| {
                Error::StorageReadError("next_entry of tree node dir", base.to_owned(), err)
            })
            .try_filter_map(|dir_entry| {
                let dir_entry = Arc::new(dir_entry);
                let path = base.join(dir_entry.file_name());

                // Skip entries that are not matched by our filter
                if let Ok(rel_path) = path.strip_prefix(&*root) {
                    let cow = rel_path.to_string_lossy();
                    let rel_path = RelativePath::new(&cow);
                    if !self.filter.should_include_path(rel_path) {
                        // Move on the next directory entry.
                        return ready(Ok(None));
                    }
                }

                let root = Arc::clone(&root);
                let dir_entry = Arc::clone(&dir_entry);
                let file_name = dir_entry.file_name().to_string_lossy().to_string();
                ready(Ok(Some(
                    self.compute_node(root, path, dir_entry, Entry::default())
                        .map_ok(|e| (file_name, e))
                        .boxed(),
                )))
            })
            .try_buffer_unordered(self.max_concurrent_branches)
            .boxed();
        while let Some((file_name, entry)) = stream.try_next().await? {
            tree_node.entries.insert(file_name, entry);
        }
        tree_node.size = tree_node.entries.len() as u64;
        Ok(tree_node)
    }

    async fn compute_node<P: AsRef<std::path::Path> + Send>(
        &self,
        root: Arc<std::path::PathBuf>,
        path: P,
        dir_entry: Arc<DirEntry>,
        mut entry: Entry,
    ) -> Result<Entry> {
        self.reporter.visit_entry(path.as_ref());
        let stat_result = match tokio::fs::symlink_metadata(&path).await {
            Ok(r) => r,
            Err(lstat_err) if lstat_err.kind() == std::io::ErrorKind::NotFound => {
                // Heuristic: if lstat fails with ENOENT, but `dir_entry` exists,
                // then the directory entry exists but it might be a whiteout file.
                // Assume so if `dir_entry` says it is a character device.
                match dir_entry.file_type().await {
                    Ok(ft) if ft.is_char_device() => {
                        // XXX: mode and size?
                        entry.kind = EntryKind::Mask;
                        entry.object = encoding::NULL_DIGEST.into();
                        self.reporter.computed_entry(&entry);
                        return Ok(entry);
                    }
                    Ok(_) => {
                        return Err(Error::String(format!(
                            "Unexpected non-char device file: {}",
                            path.as_ref().display()
                        )))
                    }
                    Err(err) => {
                        return Err(Error::StorageReadError(
                            "file_type of node dir_entry",
                            path.as_ref().to_owned(),
                            err,
                        ))
                    }
                }
            }
            Err(err) => {
                return Err(Error::StorageReadError(
                    "symlink_metadata of node path",
                    path.as_ref().to_owned(),
                    err,
                ))
            }
        };

        entry.mode = stat_result.mode();
        entry.size = stat_result.size();

        let file_type = stat_result.file_type();
        if file_type.is_symlink() {
            let _permit = self.blob_semaphore.acquire().await;
            debug_assert!(
                matches!(_permit, Ok(_)),
                "We never close the semaphore and so should never see errors"
            );
            tracing::trace!(" > symlink: {:?}", path.as_ref());
            let link_target = tokio::fs::read_link(&path)
                .await
                .map_err(|err| {
                    Error::StorageReadError("read_link of node", path.as_ref().to_owned(), err)
                })?
                .into_os_string()
                .into_string()
                .map_err(|_| {
                    crate::Error::String("Symlinks must point to a valid utf-8 path".to_string())
                })?
                .into_bytes();
            entry.kind = EntryKind::Blob;
            entry.object = self
                .hasher
                .hash_blob(Box::pin(std::io::Cursor::new(link_target)))
                .await?;
        } else if file_type.is_dir() {
            entry = self.compute_tree_node(root, path, entry).await?;
        } else if runtime::is_removed_entry(&stat_result) {
            entry.kind = EntryKind::Mask;
            entry.object = encoding::NULL_DIGEST.into();
        } else if !stat_result.is_file() {
            return Err(format!("unsupported special file: {:?}", path.as_ref()).into());
        } else {
            let _permit = self.blob_semaphore.acquire().await;
            debug_assert!(
                matches!(_permit, Ok(_)),
                "We never close the semaphore and so should never see errors"
            );
            tracing::trace!(" >    file: {:?}", path.as_ref());
            entry.kind = EntryKind::Blob;
            let reader =
                tokio::io::BufReader::new(tokio::fs::File::open(&path).await.map_err(|err| {
                    Error::StorageReadError("open of blob", path.as_ref().to_owned(), err)
                })?)
                .with_permissions(entry.mode);

            entry.object = self.hasher.hash_blob(Box::pin(reader)).await?;
        }
        self.reporter.computed_entry(&entry);
        Ok(entry)
    }
}

#[derive(Debug)]
pub struct ManifestNode<'a, T = ()> {
    pub path: RelativePathBuf,
    pub entry: &'a Entry<T>,
}

impl<'a, T> ManifestNode<'a, T>
where
    T: Clone,
{
    /// Create an owned node by cloning the underlying entry data.
    pub fn into_owned(self) -> OwnedManifestNode {
        OwnedManifestNode {
            path: self.path,
            entry: self.entry.clone().strip_user_data(),
        }
    }
}

impl<'a, T> PartialEq for ManifestNode<'a, T>
where
    T: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path && self.entry == other.entry
    }
}

impl<'a, T> Eq for ManifestNode<'a, T> where T: Eq {}

impl<'a, T> Ord for ManifestNode<'a, T>
where
    T: Eq + PartialEq,
{
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;

        use itertools::EitherOrBoth::{Both, Left, Right};
        use relative_path::Component::Normal;

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

impl<'a, T> PartialOrd for ManifestNode<'a, T>
where
    T: Eq + PartialEq,
{
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// The owned version of [`ManifestNode`]
pub struct OwnedManifestNode {
    pub path: RelativePathBuf,
    pub entry: Entry,
}

/// Receives updates from a manifest build process.
pub trait ComputeManifestReporter: Send + Sync {
    /// Called when a path has been identified to be committed
    ///
    /// This is a relative path of the file or directory
    /// within the manifest that it is being computed.
    fn visit_entry(&self, _path: &std::path::Path) {}

    /// Called after and entry has been computed and added
    /// to the manifest.
    fn computed_entry(&self, _entry: &Entry) {}
}

impl ComputeManifestReporter for () {}

impl<T> ComputeManifestReporter for Arc<T>
where
    T: ComputeManifestReporter,
{
    fn visit_entry(&self, path: &std::path::Path) {
        (**self).visit_entry(path)
    }

    fn computed_entry(&self, entry: &Entry) {
        (**self).computed_entry(entry)
    }
}
