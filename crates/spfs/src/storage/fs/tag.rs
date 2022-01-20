// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::{
    convert::TryInto,
    ffi::OsStr,
    mem::size_of,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    pin::Pin,
    task::Poll,
};

use futures::{Future, Stream};
use relative_path::RelativePath;
use tokio::io::{AsyncRead, AsyncSeek, AsyncWriteExt, ReadBuf};
use tokio_stream::StreamExt;

use super::FSRepository;
use crate::{
    encoding,
    storage::{
        tag::{TagSpecAndTagStream, TagStream},
        TagStorage,
    },
    tracking, Error, Result,
};
use encoding::{Decodable, Encodable};

#[cfg(test)]
#[path = "./tag_test.rs"]
mod tag_test;

const TAG_EXT: &str = "tag";

impl FSRepository {
    fn tags_root(&self) -> PathBuf {
        self.root().join("tags")
    }

    async fn push_raw_tag_without_lock(&self, tag: &tracking::Tag) -> Result<()> {
        let tag_spec = tracking::build_tag_spec(tag.org(), tag.name(), 0)?;
        let filepath = tag_spec.to_path(self.tags_root());

        let mut buf = Vec::new();
        tag.encode(&mut buf)?;
        let size = buf.len();

        crate::runtime::makedirs_with_perms(filepath.parent().unwrap(), 0o777)?;

        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .append(true)
            .create(true)
            .open(&filepath)
            .await?;
        file.write_i64(size as i64).await?;
        tokio::io::copy(&mut buf.as_slice(), &mut file).await?;
        if let Err(err) = file.sync_all().await {
            return Err(Error::wrap_io(err, "Failed to finalize tag data file"));
        }

        let perms = std::fs::Permissions::from_mode(0o777);
        if let Err(err) = tokio::fs::set_permissions(&filepath, perms).await {
            tracing::warn!(?err, ?filepath, "Failed to set tag permissions");
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl TagStorage for FSRepository {
    fn ls_tags(&self, path: &RelativePath) -> Pin<Box<dyn Stream<Item = Result<String>> + Send>> {
        let filepath = path.to_path(self.tags_root());
        let read_dir = match std::fs::read_dir(&filepath) {
            Ok(r) => r,
            Err(err) => match err.kind() {
                std::io::ErrorKind::NotFound => return Box::pin(futures::stream::empty()),
                _ => return Box::pin(futures::stream::once(async { Err(err.into()) })),
            },
        };

        let mut entries = std::collections::HashSet::new();
        let iter = read_dir.filter_map(move |entry| {
            let entry = match entry {
                Err(err) => return Some(Err(err.into())),
                Ok(entry) => entry,
            };
            let path = entry.path();
            if path.extension() == Some(std::ffi::OsStr::new(TAG_EXT)) {
                match path.file_stem().map(|s| s.to_string_lossy().to_string()) {
                    None => None,
                    Some(tag_name) => {
                        if entries.insert(tag_name.clone()) {
                            Some(Ok(tag_name))
                        } else {
                            None
                        }
                    }
                }
            } else {
                match path
                    .file_name()
                    .map(|s| s.to_string_lossy() + "/")
                    .map(|s| s.to_string())
                {
                    None => None,
                    Some(tag_dir) => {
                        if entries.insert(tag_dir.clone()) {
                            Some(Ok(tag_dir))
                        } else {
                            None
                        }
                    }
                }
            }
        });
        Box::pin(futures::stream::iter(iter))
    }

    /// Find tags that point to the given digest.
    ///
    /// This is an O(n) operation based on the number of all
    /// tag versions in each tag stream.
    fn find_tags(
        &self,
        digest: &encoding::Digest,
    ) -> Pin<Box<dyn Stream<Item = Result<tracking::TagSpec>> + Send>> {
        let digest = *digest;
        let stream = self.iter_tag_streams();
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
    fn iter_tag_streams(&self) -> Pin<Box<dyn Stream<Item = Result<TagSpecAndTagStream>> + Send>> {
        Box::pin(TagStreamIter::new(&self.tags_root()))
    }

    async fn read_tag(
        &self,
        tag: &tracking::TagSpec,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<tracking::Tag>> + Send>>> {
        let path = tag.to_path(self.tags_root());
        match read_tag_file(path).await {
            Err(err) => match err.raw_os_error() {
                Some(libc::ENOENT) => Err(Error::UnknownReference(tag.to_string())),
                _ => Err(err),
            },
            Ok(stream) => Ok(Box::pin(stream)),
        }
    }

    async fn push_raw_tag(&mut self, tag: &tracking::Tag) -> Result<()> {
        let tag_spec = tracking::build_tag_spec(tag.org(), tag.name(), 0)?;
        let filepath = tag_spec.to_path(self.tags_root());
        crate::runtime::makedirs_with_perms(filepath.parent().unwrap(), 0o777)?;
        let _lock = TagLock::new(&filepath).await?;
        self.push_raw_tag_without_lock(tag).await
    }

    async fn remove_tag_stream(&mut self, tag: &tracking::TagSpec) -> Result<()> {
        let tag_spec = tracking::build_tag_spec(tag.org(), tag.name(), 0)?;
        let filepath = tag_spec.to_path(self.tags_root());
        let lock = match TagLock::new(&filepath).await {
            Ok(lock) => lock,
            Err(err) => match err.raw_os_error() {
                Some(libc::ENOENT) | Some(libc::ENOTDIR) => return Ok(()),
                _ => return Err(err),
            },
        };
        match tokio::fs::remove_file(&filepath).await {
            Ok(_) => (),
            Err(err) => {
                return match err.raw_os_error() {
                    Some(libc::ENOENT) => Err(Error::UnknownReference(tag.to_string())),
                    _ => Err(err.into()),
                }
            }
        }
        // the lock file needs to be removed if the directory has any hope of being empty
        drop(lock);

        let mut filepath = filepath.as_path();
        while filepath.starts_with(self.tags_root()) {
            if let Some(parent) = filepath.parent() {
                tracing::trace!(?parent, "seeing if parent needs removing");
                match tokio::fs::remove_dir(self.tags_root().join(parent)).await {
                    Ok(_) => {
                        tracing::debug!(path = ?parent, "removed tag parent dir");
                        filepath = parent;
                    }
                    Err(err) => match err.raw_os_error() {
                        Some(libc::ENOTEMPTY) => return Ok(()),
                        Some(libc::ENOENT) => return Ok(()),
                        _ => return Err(err.into()),
                    },
                }
            }
        }
        Ok(())
    }

    async fn remove_tag(&mut self, tag: &tracking::Tag) -> Result<()> {
        let tag_spec = tracking::build_tag_spec(tag.org(), tag.name(), 0)?;
        let filepath = tag_spec.to_path(self.tags_root());
        let _lock = TagLock::new(&filepath).await?;
        let mut filtered = Vec::new();
        let mut stream = self.read_tag(&tag_spec).await?;
        while let Some(version) = stream.next().await {
            let version = version?;
            if &version != tag {
                filtered.push(version);
            }
        }
        let backup_path = &filepath.with_extension("tag.backup");
        tokio::fs::rename(&filepath, &backup_path).await?;
        let mut res = Ok(());
        for version in filtered.iter().rev() {
            // we are already holding the lock for this operation
            if let Err(err) = self.push_raw_tag_without_lock(version).await {
                res = Err(err);
                break;
            }
        }
        if let Err(err) = res {
            tokio::fs::rename(&backup_path, &filepath).await?;
            Err(err)
        } else if let Err(err) = tokio::fs::remove_file(&backup_path).await {
            tracing::warn!(?err, "failed to cleanup tag backup file");
            Ok(())
        } else {
            Ok(())
        }
    }
}

enum TagStreamIterState {
    WalkingTree,
    LoadingTag {
        spec: tracking::TagSpec,
        future: Pin<Box<dyn Future<Output = Result<TagIter<tokio::fs::File>>> + Send>>,
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
            Some(WalkingTree) => loop {
                let entry = self.inner.next();
                match entry {
                    None => break Ready(None),
                    Some(Err(err)) => break Ready(Some(Err(err.into()))),
                    Some(Ok(entry)) => {
                        if !entry.file_type().is_file() {
                            continue;
                        }
                        let path = entry.path().to_owned();
                        if path.extension() != Some(OsStr::new(TAG_EXT)) {
                            continue;
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

/// Return an iterator over all tags in the identified tag file
///
/// This iterator outputs tags from latest to earliest, ie backwards
/// stating at the latest version of the tag.
async fn read_tag_file<P: AsRef<Path>>(path: P) -> Result<TagIter<tokio::fs::File>> {
    let reader = tokio::fs::File::open(path.as_ref()).await?;
    Ok(TagIter::new(reader))
}

enum TagIterState<R>
where
    R: AsyncRead + AsyncSeek + Send + Unpin,
{
    /// Currently reading the size of a tag in bytes for the index
    ReadingIndex { reader: R, bytes_read: usize },
    /// Currently seeking to the end of a tag to read the next tag's size
    SeekingIndex { reader: R },
    /// Currently seeking backwards to the next tag to be yielded
    SeekingTag { reader: R, size: u64 },
    /// Currently reading the tag bytes so that they can be decoded
    ReadingTag { reader: R, bytes_read: usize },
}

impl<R> std::fmt::Debug for TagIterState<R>
where
    R: AsyncRead + AsyncSeek + Send + Unpin,
{
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
/// a tag file asynchronusly, and then iterates backwards
/// through each entry. This yields tags in a newest-first order
/// starting with the latest version of tag
///
/// Tag files are written
struct TagIter<R>
where
    R: AsyncRead + AsyncSeek + Send + Unpin,
{
    buf: Vec<u8>,
    sizes: Vec<u64>,
    state: Option<TagIterState<R>>,
}

impl<R> TagIter<R>
where
    R: AsyncRead + AsyncSeek + Send + Unpin,
{
    fn new(reader: R) -> Self {
        Self {
            sizes: Vec::new(),
            buf: vec![0; size_of::<i64>()],
            state: Some(TagIterState::ReadingIndex {
                reader,
                bytes_read: 0,
            }),
        }
    }
}

impl<R> Stream for TagIter<R>
where
    R: AsyncRead + AsyncSeek + Send + Unpin,
{
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
                    Ready(Err(err)) => Ready(Some(Err(err.into()))),
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
                                        Err(err) => Ready(Some(Err(err.into()))),
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
                                    "tag file contains invalid size index: {}",
                                    err
                                )))))
                            }
                        }
                        match Pin::new(&mut reader).start_seek(SeekFrom::Current(size)) {
                            Err(err) => Ready(Some(Err(err.into()))),
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
                Ready(Err(err)) => Ready(Some(Err(err.into()))),
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
                    Ready(Err(err)) => Ready(Some(Err(err.into()))),
                    Ready(Ok(_)) => {
                        match size.try_into() {
                            Ok(size) => self.buf.resize(size, 0),
                            Err(err) => {
                                return Ready(Some(Err(Error::String(format!(
                                    "tag is too large to be loaded: {}",
                                    err
                                )))))
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
                    Ready(Err(err)) => Ready(Some(Err(err.into()))),
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
                                        Err(err) => return Ready(Some(Err(err.into()))),
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
            return Err(format!("Path must end with '.{}' to be considered a tag", TAG_EXT).into())
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
        let mut filepath = root.as_ref().join(self.path());
        let new_name = self.name() + "." + TAG_EXT;
        filepath.set_file_name(new_name);
        filepath
    }
}

struct TagLock(PathBuf);

impl TagLock {
    pub async fn new<P: AsRef<Path>>(tag_file: P) -> Result<TagLock> {
        let mut lock_file = tag_file.as_ref().to_path_buf();
        lock_file.set_extension("tag.lock");

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
                    if std::time::Instant::now() < timeout {
                        continue;
                    }
                    break match err.raw_os_error() {
                        Some(libc::EEXIST) => Err("Tag already locked, cannot edit".into()),
                        _ => Err(err.into()),
                    };
                }
            }
        }
    }
}

impl Drop for TagLock {
    fn drop(&mut self) {
        if let Err(err) = std::fs::remove_file(&self.0) {
            if err.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(?err, path = ?self.0, "Failed to remove tag lock file");
            }
        }
    }
}
