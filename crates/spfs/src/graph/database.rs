// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::{HashMap, VecDeque};
use std::pin::Pin;
use std::task::Poll;

use chrono::{DateTime, Utc};
use futures::{Future, FutureExt, Stream, StreamExt, TryFutureExt, TryStreamExt};

use super::{FlatObject, Object, ObjectProto};
use crate::{Error, Result, encoding};

/// Walks an object tree depth-first starting at some root digest
#[allow(clippy::type_complexity)]
pub struct DatabaseWalker<'db> {
    db: &'db dyn DatabaseView,
    next: Option<(
        encoding::Digest,
        Pin<Box<dyn Future<Output = Result<Object>> + Send + 'db>>,
    )>,
    queue: VecDeque<encoding::Digest>,
}

impl<'db> DatabaseWalker<'db> {
    /// Create an iterator that yields all child objects starting at root
    /// from the given database.
    ///
    /// # Errors
    /// The same as [`DatabaseView::read_object`]
    pub fn new(db: &'db dyn DatabaseView, root: encoding::Digest) -> Self {
        let mut queue = VecDeque::new();
        queue.push_back(root);
        DatabaseWalker {
            db,
            queue,
            next: None,
        }
    }
}

impl Stream for DatabaseWalker<'_> {
    type Item = Result<(encoding::Digest, Object)>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let (digest, mut current_future) = match self.next.take() {
            Some(f) => f,
            None => match self.queue.pop_front() {
                None => return Poll::Ready(None),
                Some(digest) => (digest, self.db.read_object(digest)),
            },
        };

        match Pin::new(&mut current_future).poll(cx) {
            Poll::Pending => {
                self.next = Some((digest, current_future));
                Poll::Pending
            }
            Poll::Ready(obj) => Poll::Ready(match obj {
                Ok(obj) => {
                    for digest in obj.child_objects() {
                        self.queue.push_back(digest);
                    }
                    Some(Ok((digest, obj)))
                }
                Err(err) => Some(Err(err)),
            }),
        }
    }
}

/// Iterates all objects in a database, in no particular order
#[allow(clippy::type_complexity)]
pub struct DatabaseIterator<'db> {
    db: &'db dyn DatabaseView,
    next: Option<Pin<Box<dyn Future<Output = Result<DatabaseItem>> + Send + 'db>>>,
    inner: Pin<Box<dyn Stream<Item = Result<FoundDigest>> + Send>>,
}

impl<'db> DatabaseIterator<'db> {
    /// Create an iterator that yields all child objects starting at root
    /// from the given database.
    ///
    /// # Errors
    /// The same as [`DatabaseView::read_object`]
    pub fn new(db: &'db dyn DatabaseView) -> Self {
        let iter = db.find_digests(&crate::graph::DigestSearchCriteria::All);
        DatabaseIterator {
            db,
            inner: iter,
            next: None,
        }
    }
}

/// An item returned by [`DatabaseIterator`].
pub enum DatabaseItem {
    Object(encoding::Digest, Object),
    Payload(encoding::Digest),
}

impl DatabaseItem {
    /// Borrow the inner digest.
    #[inline]
    pub fn digest(&self) -> &encoding::Digest {
        match self {
            DatabaseItem::Object(d, _) => d,
            DatabaseItem::Payload(d) => d,
        }
    }
}

impl Stream for DatabaseIterator<'_> {
    type Item = Result<DatabaseItem>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let mut current_future = match self.next.take() {
            Some(f) => f,
            None => match Pin::new(&mut self.inner).poll_next(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(inner_next) => match inner_next {
                    None => return Poll::Ready(None),
                    Some(Err(err)) => return Poll::Ready(Some(Err(err))),
                    Some(Ok(FoundDigest::Object(digest))) => self
                        .db
                        .read_object(digest)
                        .map_ok(move |x| DatabaseItem::Object(digest, x))
                        .map_err(move |err| format!("Error reading object {digest}: {err}").into())
                        .boxed(),
                    Some(Ok(FoundDigest::Payload(digest))) => {
                        futures::future::ready(Ok(DatabaseItem::Payload(digest))).boxed()
                    }
                },
            },
        };
        match Pin::new(&mut current_future).poll(cx) {
            Poll::Pending => {
                self.next = Some(current_future);
                Poll::Pending
            }
            Poll::Ready(res) => Poll::Ready(match res {
                Ok(obj) => Some(Ok(obj)),
                Err(err) => Some(Err(err)),
            }),
        }
    }
}

#[derive(Clone, Debug)]
pub enum DigestSearchCriteria {
    All,
    StartsWith(encoding::PartialDigest),
}

/// The types of digests that can exist in a database.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FoundDigest {
    Object(encoding::Digest),
    Payload(encoding::Digest),
}

impl FoundDigest {
    /// Borrow the inner digest.
    #[inline]
    pub fn digest(&self) -> &encoding::Digest {
        match self {
            FoundDigest::Object(d) => d,
            FoundDigest::Payload(d) => d,
        }
    }

    /// Return the inner digest.
    #[inline]
    pub fn into_digest(self) -> encoding::Digest {
        match self {
            FoundDigest::Object(d) => d,
            FoundDigest::Payload(d) => d,
        }
    }

    /// Return the digest as a byte slice.
    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            FoundDigest::Object(d) => d.as_bytes(),
            FoundDigest::Payload(d) => d.as_bytes(),
        }
    }
}

/// The types of items a partial digest can reference.
#[derive(Clone, Copy)]
pub enum PartialDigestType {
    Object,
    Payload,
    Unknown,
}

impl PartialDigestType {
    /// Return true if the `FoundDigest` is a different item type.
    ///
    /// Unknown always return false.
    #[inline]
    pub fn conflicts_with(&self, fd: &FoundDigest) -> bool {
        match (self, fd) {
            (PartialDigestType::Unknown, _)
            | (PartialDigestType::Object, FoundDigest::Object(_))
            | (PartialDigestType::Payload, FoundDigest::Payload(_)) => false,
            (PartialDigestType::Object, FoundDigest::Payload(_))
            | (PartialDigestType::Payload, FoundDigest::Object(_)) => true,
        }
    }
}

/// A read-only object database.
#[async_trait::async_trait]
pub trait DatabaseView: Sync + Send {
    /// Read information about the given object from the database.
    ///
    /// # Errors:
    /// - [`Error::UnknownObject`]: if the object is not in this database
    async fn read_object(&self, digest: encoding::Digest) -> Result<Object>;

    /// Find the digests in this database matching a search criteria.
    ///
    /// This can include both object digests and payload digests.
    fn find_digests<'a>(
        &self,
        search_criteria: &'a DigestSearchCriteria,
    ) -> Pin<Box<dyn Stream<Item = Result<FoundDigest>> + Send + 'a>>;

    /// Return true if this database contains the identified object.
    ///
    /// This does not check for payloads.
    async fn has_object(&self, digest: encoding::Digest) -> bool;

    /// Iterate all the objects and payloads in this database.
    fn iter_objects(&self) -> DatabaseIterator<'_>;

    /// Walk all objects and payloads connected to the given root object.
    fn walk_objects<'db>(&'db self, root: &encoding::Digest) -> DatabaseWalker<'db>;

    /// Return the shortened version of the given digest.
    ///
    /// By default this is an O(n) operation defined by the number of objects.
    /// Other implementations may provide better results.
    async fn get_shortened_digest(&self, digest: encoding::Digest) -> String {
        const SIZE_STEP: usize = 5; // creates 8 char string at base 32
        let mut shortest_size: usize = SIZE_STEP;
        let mut shortest = &digest.as_bytes()[..shortest_size];
        let search_criteria =
            DigestSearchCriteria::StartsWith(encoding::PartialDigest::from(shortest));
        let mut digests = self.find_digests(&search_criteria);
        while let Some(other) = digests.next().await {
            match other {
                Err(_) => continue,
                Ok(other) => {
                    if &other.as_bytes()[0..shortest_size] != shortest {
                        continue;
                    }
                    if *other.digest() == digest {
                        continue;
                    }
                    while &other.as_bytes()[..shortest_size] == shortest {
                        shortest_size += SIZE_STEP;
                        shortest = &digest.as_bytes()[..shortest_size];
                    }
                }
            }
        }
        data_encoding::BASE32.encode(shortest)
    }

    /// Resolve the complete item digest from a shortened one.
    ///
    /// The type of item expected can be specified with `partial_digest_type`-
    /// If `PartialDigestType::Unknown` is specified and both an object and
    /// payload are found with the same digest, this will resolve to the payload
    /// instead of fail with `AmbiguousReferenceError`, for interoperability
    /// with repos containing legacy blob object files. Otherwise the type is
    /// used to disambiguate in the unlikely case a non-blob object and payload
    /// have the same digest.
    ///
    /// By default this is an O(n) operation defined by the number of items.
    /// Other implementations may provide better results.
    ///
    /// # Errors
    /// - UnknownReferenceError: if the digest cannot be resolved
    /// - AmbiguousReferenceError: if the digest could point to multiple items
    async fn resolve_full_digest(
        &self,
        partial: &encoding::PartialDigest,
        partial_digest_type: PartialDigestType,
    ) -> Result<FoundDigest> {
        #[derive(Debug)]
        struct UpgradeToPayload(FoundDigest);

        impl UpgradeToPayload {
            /// Replace the FoundDigest if fd is a Payload.
            ///
            /// If we observe both a legacy blob object and a payload with the
            /// same digest we will only remember seeing the payload. This
            /// method assumes it is only called with a FoundDigest that has the
            /// same digest as the present one.
            fn upgrade(&mut self, fd: FoundDigest) {
                if matches!(fd, FoundDigest::Payload(_)) {
                    self.0 = fd;
                }
            }
        }

        let mut options = HashMap::<_, UpgradeToPayload>::new();
        let filter = crate::graph::DigestSearchCriteria::StartsWith(partial.clone());
        let mut stream = self.find_digests(&filter);
        while let Some(fd) = stream.try_next().await? {
            if partial_digest_type.conflicts_with(&fd) {
                continue;
            }

            // Hash on the raw digest to avoid double-counting legacy blobs and
            // their payloads.
            options
                .entry(*fd.digest())
                .and_modify(|utp| utp.upgrade(fd))
                .or_insert_with(|| UpgradeToPayload(fd));
        }

        match options.len() {
            0 => Err(Error::UnknownReference(partial.to_string())),
            1 => Ok(options
                .into_iter()
                .next()
                .map(|(_, UpgradeToPayload(fd))| fd)
                .unwrap()),
            _ => Err(Error::AmbiguousReference(partial.to_string())),
        }
    }
}

#[async_trait::async_trait]
impl<T: DatabaseView> DatabaseView for &T {
    async fn has_object(&self, digest: encoding::Digest) -> bool {
        DatabaseView::has_object(&**self, digest).await
    }

    async fn read_object(&self, digest: encoding::Digest) -> Result<Object> {
        DatabaseView::read_object(&**self, digest).await
    }

    fn find_digests<'a>(
        &self,
        search_criteria: &'a DigestSearchCriteria,
    ) -> Pin<Box<dyn Stream<Item = Result<FoundDigest>> + Send + 'a>> {
        DatabaseView::find_digests(&**self, search_criteria)
    }

    fn iter_objects(&self) -> DatabaseIterator<'_> {
        DatabaseView::iter_objects(&**self)
    }

    fn walk_objects<'db>(&'db self, root: &encoding::Digest) -> DatabaseWalker<'db> {
        DatabaseView::walk_objects(&**self, root)
    }
}

#[async_trait::async_trait]
pub trait Database: DatabaseView {
    /// Remove an object from the database.
    async fn remove_object(&self, digest: encoding::Digest) -> Result<()>;

    /// Remove an object from the database if older than some threshold.
    ///
    /// Return true if the object was deleted, or false if the object was not
    /// old enough.
    async fn remove_object_if_older_than(
        &self,
        older_than: DateTime<Utc>,
        digest: encoding::Digest,
    ) -> Result<bool>;
}

#[async_trait::async_trait]
impl<T: Database> Database for &T {
    async fn remove_object(&self, digest: encoding::Digest) -> Result<()> {
        Database::remove_object(&**self, digest).await
    }

    async fn remove_object_if_older_than(
        &self,
        older_than: DateTime<Utc>,
        digest: encoding::Digest,
    ) -> Result<bool> {
        Database::remove_object_if_older_than(&**self, older_than, digest).await
    }
}

#[async_trait::async_trait]
pub trait DatabaseExt: Send + Sync {
    /// Write an object to the database, for later retrieval.
    async fn write_object<T: ObjectProto>(&self, obj: &FlatObject<T>) -> Result<()>;
}

#[async_trait::async_trait]
impl<T: DatabaseExt + Send + Sync> DatabaseExt for &T {
    async fn write_object<O: ObjectProto>(&self, obj: &FlatObject<O>) -> Result<()> {
        DatabaseExt::write_object(&**self, obj).await
    }
}
