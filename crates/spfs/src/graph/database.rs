// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::{HashSet, VecDeque};
use std::pin::Pin;
use std::task::Poll;

use chrono::{DateTime, Utc};
use futures::{Future, FutureExt, Stream, StreamExt, TryFutureExt, TryStreamExt};

use super::{FlatObject, Object, ObjectProto};
use crate::graph::object::ChildItem;
use crate::{Error, Result, encoding};

/// Walks an object tree depth-first starting at some root digest
#[allow(clippy::type_complexity)]
pub struct DatabaseWalker<'db> {
    db: &'db dyn DatabaseView,
    next: Option<(
        ChildItem,
        Pin<Box<dyn Future<Output = Result<DatabaseItem>> + Send + 'db>>,
    )>,
    queue: VecDeque<ChildItem>,
}

impl<'db> DatabaseWalker<'db> {
    /// Create an iterator that yields all child objects starting at root
    /// from the given database.
    ///
    /// # Errors
    /// The same as [`DatabaseView::read_object`]
    pub fn new(db: &'db dyn DatabaseView, root: RichDigest) -> Self {
        let mut queue = VecDeque::new();
        queue.push_back(ChildItem {
            parent: *root.digest(),
            child: root,
        });
        DatabaseWalker {
            db,
            queue,
            next: None,
        }
    }
}

/// An item returned by [`DatabaseWalker`].
pub struct DatabaseWalkerItem {
    /// The parent object digest of the currently walked item.
    pub parent: encoding::Digest,
    /// The currently walked item.
    pub child: DatabaseItem,
}

impl Stream for DatabaseWalker<'_> {
    type Item = Result<DatabaseWalkerItem>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let (child_object, mut current_future) = match self.next.take() {
            Some(f) => f,
            None => match self.queue.pop_front() {
                None => return Poll::Ready(None),
                Some(ChildItem {
                    parent,
                    child: RichDigest::Object(digest),
                }) => (
                    ChildItem {
                        parent,
                        child: RichDigest::Object(digest),
                    },
                    self.db
                        .read_object(digest)
                        .map_ok(move |obj| DatabaseItem::Object(digest, obj))
                        .boxed(),
                ),
                Some(ChildItem {
                    parent,
                    child: RichDigest::Payload(digest),
                }) => (
                    ChildItem {
                        parent,
                        child: RichDigest::Payload(digest),
                    },
                    async move { Ok(DatabaseItem::Payload(digest)) }.boxed(),
                ),
            },
        };

        match Pin::new(&mut current_future).poll(cx) {
            Poll::Pending => {
                self.next = Some((child_object, current_future));
                Poll::Pending
            }
            Poll::Ready(obj) => Poll::Ready(match obj {
                Ok(DatabaseItem::Object(_, obj)) => {
                    for digest in obj.child_items() {
                        self.queue.push_back(ChildItem {
                            parent: *child_object.child.digest(),
                            child: digest,
                        });
                    }
                    Some(Ok(DatabaseWalkerItem {
                        parent: child_object.parent,
                        child: DatabaseItem::Object(*child_object.child.digest(), obj),
                    }))
                }
                Ok(DatabaseItem::Payload(digest)) => Some(Ok(DatabaseWalkerItem {
                    parent: child_object.parent,
                    child: DatabaseItem::Payload(digest),
                })),
                Err(err) => Some(Err(err)),
            }),
        }
    }
}

type FindDigestStream = Pin<Box<dyn Stream<Item = Result<RichDigest>> + Send>>;
type WalkObjectsStream<'db> = Pin<Box<dyn Stream<Item = Result<DatabaseWalkerItem>> + Send + 'db>>;

#[derive(Default)]
enum DatabaseIterState<'db> {
    /// Ready to work on the next digest from find_digests
    NextDigest(FindDigestStream),
    /// Walking the latest object returned from read_object
    WalkingObject {
        digest: encoding::Digest,
        stream: WalkObjectsStream<'db>,
        find_digest_stream: FindDigestStream,
    },
    #[default]
    Unit,
}

/// Iterates all items in a database, in no particular order.
///
/// Items will be repeated if they are reachable by multiple paths, allowing
/// the caller to build a graph if desired. This iterator may not return objects
/// that were added concurrently after the iterator was created.
#[allow(clippy::type_complexity)]
pub struct DatabaseIterator<'db> {
    db: &'db dyn DatabaseView,
    state: DatabaseIterState<'db>,
    /// Digests are "walked" as they are found, which can visit digests that
    /// will also be later found by `find_digests`. Track what digests have been
    /// walked to avoid walking them redundantly.
    walked_digests: HashSet<encoding::Digest>,
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
            state: DatabaseIterState::NextDigest(iter),
            walked_digests: HashSet::new(),
        }
    }
}

/// An item returned by [`DatabaseIterator`].
#[derive(Debug)]
pub struct DatabaseIterItem {
    /// The parent object of this item, if any.
    ///
    /// A child object may have multiple parents; this is just one of them.
    /// [`DatabaseIterator`] will yield the same item at least as many times as
    /// it has different parents, but may also yield it with `None` if no parent
    /// has been determined yet.
    ///
    /// It is possible for objects to be written concurrently while iterating,
    /// so this may be `None` even for objects that do have a parent.
    pub parent: Option<encoding::Digest>,
    /// The item itself.
    pub item: DatabaseItem,
}

/// Discriminate digests between objects and payloads.
#[derive(Debug)]
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

impl From<&DatabaseItem> for RichDigest {
    fn from(value: &DatabaseItem) -> Self {
        match value {
            DatabaseItem::Object(digest, _) => RichDigest::Object(*digest),
            DatabaseItem::Payload(digest) => RichDigest::Payload(*digest),
        }
    }
}

impl Stream for DatabaseIterator<'_> {
    type Item = Result<DatabaseIterItem>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        match std::mem::take(&mut self.state) {
            DatabaseIterState::NextDigest(mut find_digest_stream) => {
                match Pin::new(&mut find_digest_stream).poll_next(cx) {
                    Poll::Pending => {
                        self.state = DatabaseIterState::NextDigest(find_digest_stream);
                        Poll::Pending
                    }
                    Poll::Ready(inner_next) => match inner_next {
                        None => Poll::Ready(None),
                        Some(Err(err)) => {
                            self.state = DatabaseIterState::NextDigest(find_digest_stream);
                            Poll::Ready(Some(Err(err)))
                        }
                        Some(Ok(RichDigest::Object(digest))) => {
                            if self.walked_digests.contains(&digest) {
                                // Already walked this object, skip it.
                                self.state = DatabaseIterState::NextDigest(find_digest_stream);
                                return self.poll_next(cx);
                            }
                            let stream = self.db.walk_items(RichDigest::Object(digest));
                            self.state = DatabaseIterState::WalkingObject {
                                digest,
                                stream: stream.boxed(),
                                find_digest_stream,
                            };
                            self.poll_next(cx)
                        }
                        Some(Ok(RichDigest::Payload(digest))) => {
                            self.state = DatabaseIterState::NextDigest(find_digest_stream);
                            if self.walked_digests.contains(&digest) {
                                // Already walked this payload, skip it.
                                return self.poll_next(cx);
                            }
                            Poll::Ready(Some(Ok(DatabaseIterItem {
                                parent: None,
                                item: DatabaseItem::Payload(digest),
                            })))
                        }
                    },
                }
            }
            DatabaseIterState::WalkingObject {
                digest,
                mut stream,
                find_digest_stream,
            } => match Stream::poll_next(Pin::new(&mut stream), cx) {
                Poll::Pending => {
                    self.state = DatabaseIterState::WalkingObject {
                        digest,
                        stream,
                        find_digest_stream,
                    };
                    Poll::Pending
                }
                Poll::Ready(inner_next) => match inner_next {
                    None => {
                        self.state = DatabaseIterState::NextDigest(find_digest_stream);
                        self.poll_next(cx)
                    }
                    Some(Err(err)) => {
                        self.state = DatabaseIterState::WalkingObject {
                            digest,
                            stream,
                            find_digest_stream,
                        };
                        Poll::Ready(Some(Err(err)))
                    }
                    Some(Ok(walked_item)) => {
                        self.state = DatabaseIterState::WalkingObject {
                            digest,
                            stream,
                            find_digest_stream,
                        };
                        // Any digests seen here can be considered walked.
                        self.walked_digests.insert(*walked_item.child.digest());
                        Poll::Ready(Some(Ok(DatabaseIterItem {
                            parent: {
                                // walk_objects yields the root object with
                                // itself as parent;
                                (walked_item.parent != *walked_item.child.digest())
                                    .then_some(walked_item.parent)
                            },
                            item: walked_item.child,
                        })))
                    }
                },
            },
            DatabaseIterState::Unit => {
                unreachable!()
            }
        }
    }
}

#[derive(Clone, Debug)]
pub enum DigestSearchCriteria {
    All,
    StartsWith(encoding::PartialDigest),
}

/// The types of digests that can exist in a database.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RichDigest {
    Object(encoding::Digest),
    Payload(encoding::Digest),
}

impl RichDigest {
    /// Borrow the inner digest.
    #[inline]
    pub fn digest(&self) -> &encoding::Digest {
        match self {
            RichDigest::Object(d) => d,
            RichDigest::Payload(d) => d,
        }
    }

    /// Return the inner digest.
    #[inline]
    pub fn into_digest(self) -> encoding::Digest {
        match self {
            RichDigest::Object(d) => d,
            RichDigest::Payload(d) => d,
        }
    }

    /// Return the digest as a byte slice.
    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            RichDigest::Object(d) => d.as_bytes(),
            RichDigest::Payload(d) => d.as_bytes(),
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
    ) -> Pin<Box<dyn Stream<Item = Result<RichDigest>> + Send + 'a>>;

    /// Return true if this database contains the identified object.
    ///
    /// This does not check for payloads.
    async fn has_object(&self, digest: encoding::Digest) -> bool;

    /// Iterate all the objects and payloads in this database.
    fn iter_items(&self) -> DatabaseIterator<'_>;

    /// Walk all objects and payloads connected to the given root item.
    ///
    /// The given item is included in the walk.
    fn walk_items<'db>(&'db self, root: RichDigest) -> DatabaseWalker<'db>;

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
    /// By default this is an O(n) operation defined by the number of items.
    /// Other implementations may provide better results.
    ///
    /// # Errors
    /// - UnknownReferenceError: if the digest cannot be resolved
    /// - AmbiguousReferenceError: if the digest could point to multiple items
    async fn resolve_full_digest(&self, partial: &encoding::PartialDigest) -> Result<RichDigest> {
        let options: Vec<_> = self
            .find_digests(&crate::graph::DigestSearchCriteria::StartsWith(
                partial.clone(),
            ))
            .try_collect()
            .await?;

        match options.len() {
            0 => Err(Error::UnknownReference(partial.to_string())),
            1 => Ok(options.into_iter().next().unwrap()),
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
    ) -> Pin<Box<dyn Stream<Item = Result<RichDigest>> + Send + 'a>> {
        DatabaseView::find_digests(&**self, search_criteria)
    }

    fn iter_items(&self) -> DatabaseIterator<'_> {
        DatabaseView::iter_items(&**self)
    }

    fn walk_items<'db>(&'db self, root: RichDigest) -> DatabaseWalker<'db> {
        DatabaseView::walk_items(&**self, root)
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
