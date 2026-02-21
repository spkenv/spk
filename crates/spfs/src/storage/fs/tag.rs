// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::borrow::Cow;
use std::convert::TryInto;
use std::ffi::OsStr;
use std::mem::size_of;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::task::Poll;

use close_err::Closable;
use encoding::{Decodable, Encodable};
use futures::future::ready;
use futures::{Future, Stream, StreamExt, TryFutureExt};
use relative_path::RelativePath;
use tokio::io::{AsyncRead, AsyncSeek, AsyncWriteExt, ReadBuf};

use super::{MaybeOpenFsRepository, OpenFsRepository};
use crate::storage::tag::{EntryType, TagSpecAndTagStream, TagStream};
use crate::storage::{
    TAG_NAMESPACE_MARKER,
    TagNamespace,
    TagNamespaceBuf,
    TagStorage,
    TagStorageMut,
};
use crate::{Error, OsError, OsErrorExt, Result, encoding, tracking};

const TAG_EXT: &str = "tag";

#[async_trait::async_trait]
impl TagStorage for MaybeOpenFsRepository {
    #[inline]
    fn get_tag_namespace(&self) -> Option<Cow<'_, TagNamespace>> {
        self.fs_impl.get_tag_namespace()
    }

    fn ls_tags_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        path: &RelativePath,
    ) -> Pin<Box<dyn Stream<Item = Result<EntryType>> + Send>> {
        let path = path.to_owned();
        let namespace = namespace.map(ToOwned::to_owned);
        self.opened()
            .map_ok(move |opened| opened.ls_tags_in_namespace(namespace.as_deref(), &path))
            .try_flatten_stream()
            .boxed()
    }

    /// Find tags that point to the given digest.
    ///
    /// This is an O(n) operation based on the number of all
    /// tag versions in each tag stream.
    fn find_tags_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        digest: &encoding::Digest,
    ) -> Pin<Box<dyn Stream<Item = Result<tracking::TagSpec>> + Send>> {
        let digest = *digest;
        let namespace = namespace.map(ToOwned::to_owned);
        self.opened()
            .map_ok(move |opened| opened.find_tags_in_namespace(namespace.as_deref(), &digest))
            .try_flatten_stream()
            .boxed()
    }

    /// Iterate through the available tags in this storage.
    fn iter_tag_streams_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
    ) -> Pin<Box<dyn Stream<Item = Result<TagSpecAndTagStream>> + Send>> {
        let namespace = namespace.map(ToOwned::to_owned);
        self.opened()
            .and_then(move |opened| {
                ready(Ok(
                    opened.iter_tag_streams_in_namespace(namespace.as_deref())
                ))
            })
            .try_flatten_stream()
            .boxed()
    }

    async fn read_tag_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        tag: &tracking::TagSpec,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<tracking::Tag>> + Send>>> {
        self.opened()
            .await?
            .read_tag_in_namespace(namespace, tag)
            .await
    }

    async fn insert_tag_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        tag: &tracking::Tag,
    ) -> Result<()> {
        self.opened()
            .await?
            .insert_tag_in_namespace(namespace, tag)
            .await
    }

    async fn remove_tag_stream_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        tag: &tracking::TagSpec,
    ) -> Result<()> {
        self.opened()
            .await?
            .remove_tag_stream_in_namespace(namespace, tag)
            .await
    }

    async fn remove_tag_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        tag: &tracking::Tag,
    ) -> Result<()> {
        self.opened()
            .await?
            .remove_tag_in_namespace(namespace, tag)
            .await
    }
}

impl MaybeOpenFsRepository {
    /// Forcefully remove any lock file for the identified tag.
    ///
    /// # Safety
    /// This function is unsafe because it removes the lock file without
    /// ensuring that the tag file is not being written to, which can cause
    /// corruption in the tag file.
    pub async unsafe fn unlock_tag_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        tag: tracking::TagSpec,
    ) -> Result<()> {
        // Safety: we do not ensure the tag file is not being written to
        // but pass the responsibility to the caller.
        unsafe {
            self.opened()
                .await?
                .unlock_tag_in_namespace(namespace, tag)
                .await
        }
    }
}

impl OpenFsRepository {
    fn tags_root_in_namespace(&self, namespace: Option<&TagNamespace>) -> PathBuf {
        let mut tags_root = self.root().join("tags");
        if let Some(tag_namespace) = namespace {
            for component in tag_namespace.as_rel_path().components() {
                // Assuming the tag namespace is only made up of `Normal`
                // elements (validated elsewhere).
                let relative_path::Component::Normal(component) = component else {
                    continue;
                };

                // Add a suffix in the form of `"#ns"` to distinguish
                // tag namespace subdirectories from normal tag subdirectories.
                tags_root = tags_root.join(format!("{component}{TAG_NAMESPACE_MARKER}"));
            }
        }
        tags_root
    }

    /// Forcefully remove any lock file for the identified tag.
    ///
    /// # Safety
    /// This function is unsafe because it removes the lock file without
    /// ensuring that the tag file is not being written to, which can cause
    /// corruption in the tag file.
    pub async unsafe fn unlock_tag_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        tag: tracking::TagSpec,
    ) -> Result<()> {
        let path = tag.to_path(self.tags_root_in_namespace(namespace));
        // Safety: we do not ensure the tag file is not being written to
        // but pass the responsibility to the caller.
        unsafe { TagLock::remove(path) }
    }
}

#[async_trait::async_trait]
impl TagStorage for OpenFsRepository {
    #[inline]
    fn get_tag_namespace(&self) -> Option<Cow<'_, TagNamespace>> {
        self.fs_impl.get_tag_namespace()
    }

    fn ls_tags_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        path: &RelativePath,
    ) -> Pin<Box<dyn Stream<Item = Result<EntryType>> + Send>> {
        let filepath = path.to_path(self.tags_root_in_namespace(namespace));
        let read_dir = match std::fs::read_dir(&filepath) {
            Ok(r) => r,
            Err(err) => match err.kind() {
                std::io::ErrorKind::NotFound => return Box::pin(futures::stream::empty()),
                _ => {
                    return Box::pin(futures::stream::once(async {
                        Err(Error::StorageReadError(
                            "read_dir on tags path",
                            filepath,
                            err,
                        ))
                    }));
                }
            },
        };

        let iter = read_dir.filter_map(move |entry| {
            let entry = match entry {
                Err(err) => {
                    return Some(Err(Error::StorageReadError(
                        "entry of tags path",
                        filepath.clone(),
                        err,
                    )));
                }
                Ok(entry) => entry,
            };
            let path = entry.path();
            if path.extension() == Some(std::ffi::OsStr::new(TAG_EXT)) {
                path.file_stem()
                    .map(|s| Ok(EntryType::Tag(s.to_string_lossy().to_string())))
            } else if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                path.file_name().map(|s| {
                    let s = s.to_string_lossy();
                    match s.split_once(TAG_NAMESPACE_MARKER) {
                        Some((name, _)) => Ok(EntryType::Namespace(name.into())),
                        None => Ok(EntryType::Folder(s.to_string())),
                    }
                })
            } else {
                None
            }
        });
        Box::pin(futures::stream::iter(iter))
    }

    /// Find tags that point to the given digest.
    ///
    /// This is an O(n) operation based on the number of all
    /// tag versions in each tag stream.
    fn find_tags_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        digest: &encoding::Digest,
    ) -> Pin<Box<dyn Stream<Item = Result<tracking::TagSpec>> + Send>> {
        let digest = *digest;
        let stream = self.iter_tag_streams_in_namespace(namespace);
        let mapped = futures::StreamExt::filter_map(stream, move |res| async move {
            let (spec, stream) = match res {
                Ok(res) => res,
                Err(err) => return Some(Err(err)),
            };
            let mut stream = futures::StreamExt::enumerate(stream);
            while let Some((i, tag)) = stream.next().await {
                match tag {
                    Ok(tag) if tag.target == digest => {
                        return Some(Ok(spec.with_version(i as u64)));
                    }
                    Ok(_) => continue,
                    Err(err) => return Some(Err(err)),
                }
            }
            None
        });
        Box::pin(mapped)
    }

    /// Iterate through the available tags in this storage.
    fn iter_tag_streams_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
    ) -> Pin<Box<dyn Stream<Item = Result<TagSpecAndTagStream>> + Send>> {
        Box::pin(TagStreamIter::new(self.tags_root_in_namespace(namespace)))
    }

    async fn read_tag_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        tag: &tracking::TagSpec,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<tracking::Tag>> + Send>>> {
        let path = tag.to_path(self.tags_root_in_namespace(namespace));
        match read_tag_file(path).await {
            Err(err) if err.is_os_not_found() => Err(Error::UnknownReference(tag.to_string())),
            Err(err) => Err(err),
            Ok(stream) => Ok(Box::pin(stream)),
        }
    }

    async fn insert_tag_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        tag: &tracking::Tag,
    ) -> Result<()> {
        let tag_spec = tracking::build_tag_spec(tag.org(), tag.name(), 0)?;
        let filepath = tag_spec.to_path(self.tags_root_in_namespace(namespace));
        crate::runtime::makedirs_with_perms(filepath.parent().unwrap(), 0o777).map_err(|err| {
            Error::StorageWriteError("insert_tag::create_parent", filepath.clone(), err)
        })?;
        let working_file = TagWorkingFile::new(&filepath).await?;

        let mut tags: Vec<tracking::Tag> = vec![];
        match self.read_tag_in_namespace(namespace, &tag_spec).await {
            Ok(mut stream) => {
                let mut inserted = false;
                while let Some(next) = stream.next().await {
                    let next = next?;
                    if inserted {
                        tags.insert(0, next);
                        continue;
                    }
                    if next == *tag {
                        // this tag already exists in the stream,
                        // and will be dropped
                        return Ok(());
                    }
                    // Compare by time first to ensure correct
                    // chronological ordering, then by the remaining
                    // fields as tiebreakers. This is important because
                    // Tag::cmp() compares org/name before time, which
                    // causes incorrect ordering when a tag stream
                    // contains entries with different org values (e.g.
                    // after renaming a tag path for version
                    // normalization).
                    use std::cmp::Ordering::*;
                    let time_ord = next.time.cmp(&tag.time);
                    match if time_ord != Equal {
                        time_ord
                    } else {
                        next.cmp(tag)
                    } {
                        Less => {
                            tags.insert(0, tag.clone());
                            tags.insert(0, next);
                            inserted = true;
                        }
                        Greater => {
                            tags.insert(0, next);
                        }
                        Equal => {
                            unreachable!("Tag equality was already checked above");
                        }
                    };
                }
                if !inserted {
                    // The target tag was not inserted so it needs to be appended to the end
                    tags.insert(0, tag.clone());
                }
                Ok(())
            }
            Err(Error::UnknownReference(_)) => {
                tags.push(tag.clone());
                Ok(())
            }
            Err(err) => Err(err),
        }?;

        working_file.write_tags(&tags).await
    }

    async fn remove_tag_stream_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        tag: &tracking::TagSpec,
    ) -> Result<()> {
        let tag_spec = tracking::build_tag_spec(tag.org(), tag.name(), 0)?;
        let filepath = tag_spec.to_path(self.tags_root_in_namespace(namespace));
        let lock = match TagLock::new(&filepath).await {
            Ok(lock) => lock,
            Err(err) => match err.os_error() {
                Some(libc::ENOENT) | Some(libc::ENOTDIR) => {
                    return Err(Error::UnknownReference(tag.to_string()));
                }
                _ => return Err(err),
            },
        };
        match tokio::fs::remove_file(&filepath).await {
            Ok(_) => (),
            Err(err) => {
                return if err.is_os_not_found() {
                    Err(Error::UnknownReference(tag.to_string()))
                } else {
                    Err(Error::StorageWriteError(
                        "remove_file on tag stream file",
                        filepath,
                        err,
                    ))
                };
            }
        }
        // the lock file needs to be removed if the directory has any hope of being empty
        drop(lock);

        let tags_root = self.tags_root_in_namespace(namespace);
        let mut filepath = filepath.as_path();
        while filepath.starts_with(&tags_root) {
            let Some(parent) = filepath.parent() else {
                break;
            };
            if parent == tags_root {
                break;
            }
            tracing::trace!(?parent, "seeing if parent needs removing");
            match tokio::fs::remove_dir(&parent).await {
                Ok(_) => {
                    tracing::debug!(path = ?parent, "removed tag parent dir");
                    filepath = parent;
                }
                Err(err) if err.is_os_not_found() => return Ok(()),
                Err(err) => match err.os_error() {
                    Some(libc::ENOTEMPTY) => return Ok(()),
                    _ => {
                        return Err(Error::StorageWriteError(
                            "remove_dir on tag stream parent dir",
                            parent.to_owned(),
                            err,
                        ));
                    }
                },
            }
        }
        Ok(())
    }

    async fn remove_tag_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        tag: &tracking::Tag,
    ) -> Result<()> {
        let tag_spec = tracking::build_tag_spec(tag.org(), tag.name(), 0)?;
        let filepath = tag_spec.to_path(self.tags_root_in_namespace(namespace));
        let working_file = TagWorkingFile::new(&filepath).await?;

        let mut tags: Vec<tracking::Tag> = vec![];
        match self.read_tag(&tag_spec).await {
            Ok(mut stream) => {
                while let Some(next) = stream.next().await {
                    let next = next?;
                    if &next != tag {
                        tags.insert(0, next);
                    }
                }
                Ok(())
            }
            Err(err) => Err(err),
        }?;

        working_file.write_tags(&tags).await
    }
}

impl TagStorageMut for MaybeOpenFsRepository {
    fn try_set_tag_namespace(
        &mut self,
        tag_namespace: Option<TagNamespaceBuf>,
    ) -> Result<Option<TagNamespaceBuf>> {
        Ok(Arc::make_mut(&mut self.fs_impl).set_tag_namespace(tag_namespace))
    }
}

enum TagStreamIterState {
    WalkingTree,
    LoadingTag {
        spec: tracking::TagSpec,
        future: Pin<Box<dyn Future<Output = Result<TagIter>> + Send>>,
    },
}

struct TagStreamIter {
    root: PathBuf,
    inner: walkdir::IntoIter,
    state: Option<TagStreamIterState>,
}

impl TagStreamIter {
    fn new<P: AsRef<std::path::Path>>(root: P) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
            inner: walkdir::WalkDir::new(root).into_iter(),
            state: Some(TagStreamIterState::WalkingTree),
        }
    }
}

impl Stream for TagStreamIter {
    type Item = Result<(tracking::TagSpec, TagStream)>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        use Poll::*;
        use TagStreamIterState::*;
        match self.state.take() {
            // TODO: this walkdir loop is not actually async and should be fixed
            Some(WalkingTree) => 'entry: loop {
                let entry = self.inner.next();
                match entry {
                    None => break Ready(None),
                    Some(Err(err)) => {
                        break Ready(Some(Err(Error::StorageReadError(
                            "entry in tags stream",
                            self.root.clone(),
                            err.into(),
                        ))));
                    }
                    Some(Ok(entry)) => {
                        if !entry.file_type().is_file() {
                            continue;
                        }
                        let path = entry.path().to_owned();
                        if path.extension() != Some(OsStr::new(TAG_EXT)) {
                            continue;
                        }

                        // This iterator skips over any namespaces; since the
                        // walkdir iterator will descend into any directory,
                        // the whole parent hierarchy needs to be checked for
                        // the namespace marker.
                        //
                        // The root itself may be a namespace.
                        let mut parent = path.parent();
                        while let Some(p) = parent {
                            if p == self.root {
                                break;
                            }
                            if p.file_name()
                                .and_then(|s| s.to_str())
                                .map(|p| p.ends_with(TAG_NAMESPACE_MARKER))
                                .unwrap_or_default()
                            {
                                continue 'entry;
                            }
                            parent = p.parent();
                        }

                        let spec = match tag_from_path(&path, &self.root) {
                            Err(err) => break Ready(Some(Err(err))),
                            Ok(spec) => spec,
                        };
                        self.state = Some(LoadingTag {
                            spec,
                            future: Box::pin(read_tag_file(path)),
                        });
                        break self.poll_next(cx);
                    }
                }
            },
            Some(LoadingTag { spec, mut future }) => match Pin::new(&mut future).poll(cx) {
                Pending => {
                    self.state = Some(LoadingTag { spec, future });
                    Pending
                }
                Ready(Err(err)) => Ready(Some(Err(err))),
                Ready(Ok(stream)) => {
                    self.state = Some(WalkingTree);
                    Ready(Some(Ok((spec, Box::pin(stream)))))
                }
            },
            None => Ready(None),
        }
    }
}

trait TagReader: AsyncRead + AsyncSeek + Send + Unpin {}

impl TagReader for tokio::io::BufReader<tokio::fs::File> {}

async fn write_tags_to_path(filepath: &PathBuf, tags: &[tracking::Tag]) -> Result<()> {
    crate::runtime::makedirs_with_perms(filepath.parent().unwrap(), 0o777)
        .map_err(|err| Error::StorageWriteError("write_tags_to_path", filepath.clone(), err))?;
    let mut file = tokio::io::BufWriter::new(
        tokio::fs::OpenOptions::new()
            .write(true)
            .append(true)
            .create(true)
            .open(&filepath)
            .await
            .map_err(|err| {
                Error::StorageWriteError("open tag file for append", filepath.to_owned(), err)
            })?,
    );

    for tag in tags.iter() {
        let buf = tag.encode_to_bytes()?;
        let size = buf.len();
        file.write_i64(size as i64).await.map_err(|err| {
            Error::StorageWriteError("write_i64 on tag file", filepath.clone(), err)
        })?;
        file.write_all_buf(&mut buf.as_slice())
            .await
            .map_err(|err| {
                Error::StorageWriteError("write_all_buf on tag file", filepath.clone(), err)
            })?;
    }
    if let Err(err) = file.flush().await {
        return Err(Error::StorageWriteError(
            "flush on tag file",
            filepath.clone(),
            err,
        ));
    }
    if let Err(err) = file.into_inner().into_std().await.close() {
        return Err(Error::StorageWriteError(
            "close on tag file",
            filepath.clone(),
            err,
        ));
    }

    #[cfg(unix)]
    {
        let perms = std::fs::Permissions::from_mode(0o666);
        if let Err(err) = tokio::fs::set_permissions(&filepath, perms).await {
            tracing::warn!(?err, ?filepath, "Failed to set tag permissions");
        }
    }
    Ok(())
}

/// Return an iterator over all tags in the identified tag file
///
/// This iterator outputs tags from latest to earliest, ie backwards
/// stating at the latest version of the tag.
async fn read_tag_file<P>(path: P) -> Result<TagIter>
where
    P: AsRef<Path>,
{
    let reader = tokio::fs::File::open(path.as_ref()).await.map_err(|err| {
        Error::StorageReadError("open of tag file", path.as_ref().to_owned(), err)
    })?;
    Ok(TagIter::new(
        Box::new(tokio::io::BufReader::new(reader)),
        path.as_ref().to_owned(),
    ))
}

enum TagIterState {
    /// Currently reading the size of a tag in bytes for the index
    ReadingIndex {
        reader: Box<dyn TagReader>,
        bytes_read: usize,
    },
    /// Currently seeking to the end of a tag to read the next tag's size
    SeekingIndex { reader: Box<dyn TagReader> },
    /// Currently seeking backwards to the next tag to be yielded
    SeekingTag {
        reader: Box<dyn TagReader>,
        size: u64,
    },
    /// Currently reading the tag bytes so that they can be decoded
    ReadingTag {
        reader: Box<dyn TagReader>,
        bytes_read: usize,
    },
}

impl std::fmt::Debug for TagIterState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReadingIndex { bytes_read, .. } => f
                .debug_struct("ReadingIndex")
                .field("bytes_read", bytes_read)
                .finish(),
            Self::SeekingIndex { .. } => f.debug_struct("SeekingIndex").finish(),
            Self::SeekingTag { .. } => f.debug_struct("SeekingTag").finish(),
            Self::ReadingTag { bytes_read, .. } => f
                .debug_struct("ReadingTag")
                .field("bytes_read", bytes_read)
                .finish(),
        }
    }
}

/// Using a series of states, the TagIter indexes
/// a tag file asynchronously, and then iterates backwards
/// through each entry. This yields tags in a newest-first order
/// starting with the latest version of tag
///
/// Tag files are written
struct TagIter {
    buf: Vec<u8>,
    sizes: Vec<u64>,
    state: Option<TagIterState>,
    filename: PathBuf,
}

impl TagIter {
    fn new(reader: Box<dyn TagReader>, filename: PathBuf) -> Self {
        Self {
            sizes: Vec::new(),
            buf: vec![0; size_of::<i64>()],
            state: Some(TagIterState::ReadingIndex {
                reader,
                bytes_read: 0,
            }),
            filename,
        }
    }
}

impl Stream for TagIter {
    type Item = Result<tracking::Tag>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        use std::io::SeekFrom;

        use Poll::*;
        use TagIterState::*;

        match self.state.take() {
            Some(ReadingIndex {
                mut reader,
                mut bytes_read,
            }) => {
                let mut buf = ReadBuf::new(&mut self.buf[bytes_read..]);
                match Pin::new(&mut reader).poll_read(cx, &mut buf) {
                    Pending => {
                        self.state = Some(ReadingIndex { reader, bytes_read });
                        Pending
                    }
                    Ready(Err(err)) => Ready(Some(Err(Error::StorageReadError(
                        "read of tag",
                        self.filename.clone(),
                        err,
                    )))),
                    Ready(Ok(())) => {
                        let count = buf.filled().len();
                        if count == 0 {
                            // if the read completed but did not return anything,
                            // we are to interpret it as an EOF and so will move on to
                            // reading back any tags that were indexed
                            return match self.sizes.pop() {
                                Some(size) => {
                                    let last_tag_start =
                                        self.sizes.iter().fold(size_of::<i64>() as u64, |c, s| {
                                            // account for the leading size indicator of each tag
                                            c + *s + size_of::<i64>() as u64
                                        });
                                    match Pin::new(&mut reader)
                                        .start_seek(SeekFrom::Start(last_tag_start))
                                    {
                                        Err(err) => Ready(Some(Err(Error::StorageReadError(
                                            "start_seek on tag",
                                            self.filename.clone(),
                                            err,
                                        )))),
                                        Ok(_) => {
                                            self.state = Some(SeekingTag { reader, size });
                                            self.poll_next(cx)
                                        }
                                    }
                                }
                                None => Ready(None),
                            };
                        }
                        bytes_read += count;
                        if bytes_read < self.buf.len() {
                            self.state = Some(ReadingIndex { reader, bytes_read });
                            return self.poll_next(cx);
                        }
                        // we trust that the buffer was resized for this purpose above
                        let size = i64::from_be_bytes(self.buf[..bytes_read].try_into().unwrap());
                        match size.try_into() {
                            Ok(size) => self.sizes.push(size),
                            Err(err) => {
                                return Ready(Some(Err(Error::String(format!(
                                    "tag file contains invalid size index: {err}",
                                )))));
                            }
                        }
                        match Pin::new(&mut reader).start_seek(SeekFrom::Current(size)) {
                            Err(err) => Ready(Some(Err(Error::StorageReadError(
                                "start_seek on tag",
                                self.filename.clone(),
                                err,
                            )))),
                            Ok(_) => {
                                self.state = Some(SeekingIndex { reader });
                                self.poll_next(cx)
                            }
                        }
                    }
                }
            }
            Some(SeekingIndex { mut reader }) => match Pin::new(&mut reader).poll_complete(cx) {
                Pending => {
                    self.state = Some(SeekingIndex { reader });
                    Pending
                }
                Ready(Err(err)) => Ready(Some(Err(Error::StorageReadError(
                    "SeekingIndex on tag",
                    self.filename.clone(),
                    err,
                )))),
                Ready(Ok(_)) => {
                    self.buf.resize(size_of::<i64>(), 0);
                    self.state = Some(ReadingIndex {
                        reader,
                        bytes_read: 0,
                    });
                    self.poll_next(cx)
                }
            },
            Some(SeekingTag { mut reader, size }) => {
                match Pin::new(&mut reader).poll_complete(cx) {
                    Pending => {
                        self.state = Some(SeekingTag { reader, size });
                        Pending
                    }
                    Ready(Err(err)) => Ready(Some(Err(Error::StorageReadError(
                        "SeekingTag",
                        self.filename.clone(),
                        err,
                    )))),
                    Ready(Ok(_)) => {
                        match size.try_into() {
                            Ok(size) => self.buf.resize(size, 0),
                            Err(err) => {
                                return Ready(Some(Err(Error::String(format!(
                                    "tag is too large to be loaded: {err}",
                                )))));
                            }
                        }
                        self.state = Some(ReadingTag {
                            reader,
                            bytes_read: 0,
                        });
                        self.poll_next(cx)
                    }
                }
            }
            Some(ReadingTag {
                mut reader,
                mut bytes_read,
            }) => {
                let mut buf = ReadBuf::new(&mut self.buf[bytes_read..]);
                match Pin::new(&mut reader).poll_read(cx, &mut buf) {
                    Pending => {
                        self.state = Some(ReadingTag { reader, bytes_read });
                        Pending
                    }
                    Ready(Err(err)) => Ready(Some(Err(Error::StorageReadError(
                        "ReadingTag",
                        self.filename.clone(),
                        err,
                    )))),
                    Ready(Ok(_)) => {
                        let count = buf.filled().len();
                        bytes_read += count;
                        if bytes_read < self.buf.len() {
                            self.state = Some(ReadingTag { reader, bytes_read });
                            return self.poll_next(cx);
                        }
                        match tracking::Tag::decode(&mut self.buf.as_slice()) {
                            Err(err) => Ready(Some(Err(err))),
                            Ok(tag) => {
                                if let Some(size) = self.sizes.pop() {
                                    let next_tag_offset =
                                        bytes_read as u64 + size_of::<i64>() as u64 + size;
                                    match Pin::new(&mut reader)
                                        .start_seek(SeekFrom::Current(-(next_tag_offset as i64)))
                                    {
                                        Err(err) => {
                                            return Ready(Some(Err(Error::StorageReadError(
                                                "start_seek in ReadingTag",
                                                self.filename.clone(),
                                                err,
                                            ))));
                                        }
                                        Ok(_) => self.state = Some(SeekingTag { reader, size }),
                                    }
                                }
                                Ready(Some(Ok(tag)))
                            }
                        }
                    }
                }
            }
            None => Ready(None),
        }
    }
}

fn tag_from_path<P: AsRef<Path>, R: AsRef<Path>>(path: P, root: R) -> Result<tracking::TagSpec> {
    let mut path = path.as_ref().to_path_buf();
    let filename = match path.file_stem() {
        Some(stem) => stem.to_owned(),
        None => {
            return Err(format!("Path must end with '.{TAG_EXT}' to be considered a tag").into());
        }
    };
    path.set_file_name(filename);
    let path = path.strip_prefix(root)?;
    tracking::TagSpec::parse(path.to_string_lossy())
}
pub trait TagExt {
    fn to_path<P: AsRef<Path>>(&self, root: P) -> PathBuf;
}

impl TagExt for tracking::TagSpec {
    fn to_path<P: AsRef<Path>>(&self, root: P) -> PathBuf {
        let mut filepath = self.path().to_path(root);
        let new_name = self.name() + "." + TAG_EXT;
        filepath.set_file_name(new_name);
        filepath
    }
}

struct TagLock(PathBuf);

impl TagLock {
    const LOCK_EXT: &'static str = "tag.lock";

    pub async fn new<P: AsRef<Path>>(tag_file: P) -> Result<TagLock> {
        let mut lock_file = tag_file.as_ref().to_path_buf();
        lock_file.set_extension(Self::LOCK_EXT);

        let timeout = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            match tokio::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&lock_file)
                .await
            {
                Ok(_file) => {
                    break Ok(TagLock(lock_file));
                }
                Err(err) => {
                    break match err.os_error() {
                        Some(libc::EEXIST) if std::time::Instant::now() < timeout => {
                            // Wait up until the timeout to acquire the lock,
                            // but fail immediately for other [non-temporary]
                            // problems, like the directory not existing.
                            continue;
                        }
                        Some(libc::EEXIST) => Err("Tag already locked, cannot edit".into()),
                        _ => Err(Error::StorageWriteError(
                            "open tag lock file for write exclusively",
                            lock_file,
                            err,
                        )),
                    };
                }
            }
        }
    }

    /// Remove the lock file for a tag
    ///
    /// # Safety:
    /// Tag locks are used to ensure that only one process is writing to a tag file at a time.
    /// Removing the lock file without ensuring that the tag file is not being written to may
    /// cause the data within the file to become corrupt.
    pub unsafe fn remove<P: AsRef<Path>>(tag_file: P) -> Result<()> {
        let mut lock_file = tag_file.as_ref().to_path_buf();
        lock_file.set_extension(Self::LOCK_EXT);
        if let Err(err) = std::fs::remove_file(&lock_file)
            && err.kind() != std::io::ErrorKind::NotFound
        {
            return Err(Error::StorageWriteError("unlock tag", lock_file, err));
        }
        Ok(())
    }
}

impl Drop for TagLock {
    fn drop(&mut self) {
        if let Err(err) = std::fs::remove_file(&self.0)
            && err.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(?err, path = ?self.0, "Failed to remove tag lock file");
        }
    }
}

/// Enables atomic tag file updates by writing to a working file and replacing the original
///
/// It is expected that the caller will not already hold the tag lock, as this instance
/// will require getting the lock and can be used in its stead
struct TagWorkingFile {
    original: PathBuf,
    _lock: TagLock,
}

impl TagWorkingFile {
    /// Generate a new working file for the provided tag file
    ///
    pub async fn new<P: Into<PathBuf>>(tag_file: P) -> Result<Self> {
        let original = tag_file.into();
        let _lock = TagLock::new(&original).await?;
        Ok(Self { original, _lock })
    }

    /// Write the tags to the underlying tag file via the working file.
    ///
    /// Writing 0 tags will result in the original file being removed
    /// rather than actually replacing it with an empty file.
    pub async fn write_tags(self, tags: &[tracking::Tag]) -> Result<()> {
        let working = self.original.with_extension("tag.work");
        if tags.is_empty() {
            return tokio::fs::remove_file(&self.original).await.map_err(|err| {
                Error::StorageWriteError("remove_file on tag stream file", self.original, err)
            });
        }
        if let Err(err) = write_tags_to_path(&working, tags).await {
            if let Err(err) = tokio::fs::remove_file(&working).await {
                tracing::warn!(
                    "failed to clean up tag working file after failing to write tags to path: {err}"
                );
            }
            return Err(err);
        }
        if let Err(err) = tokio::fs::rename(&working, &self.original).await {
            if let Err(err) = tokio::fs::remove_file(&working).await {
                tracing::warn!(
                    "failed to clean up tag working file after failing to finalize the working file: {err}"
                );
            }
            return Err(Error::StorageWriteError(
                "rename of tag stream file",
                self.original,
                err,
            ));
        }
        Ok(())
    }
}
