// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::{collections::VecDeque, pin::Pin, task::Poll};

use futures::{Future, Stream, StreamExt, TryStreamExt};

use super::Object;
use crate::{encoding, Error, Result};

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

impl<'db> Stream for DatabaseWalker<'db> {
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
    next: Option<(
        encoding::Digest,
        Pin<Box<dyn Future<Output = Result<Object>> + Send + 'db>>,
    )>,
    inner: Pin<Box<dyn Stream<Item = Result<encoding::Digest>> + Send>>,
}

impl<'db> DatabaseIterator<'db> {
    /// Create an iterator that yields all child objects starting at root
    /// from the given database.
    ///
    /// # Errors
    /// The same as [`DatabaseView::read_object`]
    pub fn new(db: &'db dyn DatabaseView) -> Self {
        let iter = db.find_digests(crate::graph::DigestSearchCriteria::All);
        DatabaseIterator {
            db,
            inner: iter,
            next: None,
        }
    }
}

impl<'db> Stream for DatabaseIterator<'db> {
    type Item = Result<(encoding::Digest, Object)>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let (digest, mut current_future) = match self.next.take() {
            Some(f) => f,
            None => match Pin::new(&mut self.inner).poll_next(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(inner_next) => match inner_next {
                    None => return Poll::Ready(None),
                    Some(Err(err)) => return Poll::Ready(Some(Err(err))),
                    Some(Ok(digest)) => (digest, self.db.read_object(digest)),
                },
            },
        };
        match Pin::new(&mut current_future).poll(cx) {
            Poll::Pending => {
                self.next = Some((digest, current_future));
                Poll::Pending
            }
            Poll::Ready(res) => Poll::Ready(match res {
                Ok(obj) => Some(Ok((digest, obj))),
                Err(err) => Some(Err(format!("Error reading object {digest}: {err}").into())),
            }),
        }
    }
}

#[derive(Debug)]
pub enum DigestSearchCriteria {
    All,
    StartsWith(encoding::PartialDigest),
}

/// A read-only object database.
#[async_trait::async_trait]
pub trait DatabaseView: Sync + Send {
    /// Read information about the given object from the database.
    ///
    /// # Errors:
    /// - [`Error::UnknownObject`]: if the object is not in this database
    async fn read_object(&self, digest: encoding::Digest) -> Result<Object>;

    /// Find the object digests in this database matching a search criteria.
    fn find_digests(
        &self,
        search_criteria: DigestSearchCriteria,
    ) -> Pin<Box<dyn Stream<Item = Result<encoding::Digest>> + Send>>;

    /// Return true if this database contains the identified object
    async fn has_object(&self, digest: encoding::Digest) -> bool {
        self.read_object(digest).await.is_ok()
    }

    /// Iterate all the object in this database.
    fn iter_objects(&self) -> DatabaseIterator<'_>;

    /// Walk all objects connected to the given root object.
    fn walk_objects<'db>(&'db self, root: &encoding::Digest) -> DatabaseWalker<'db>;

    /// Return the shortened version of the given digest.
    ///
    /// By default this is an O(n) operation defined by the number of objects.
    /// Other implementations may provide better results.
    async fn get_shortened_digest(&self, digest: encoding::Digest) -> String {
        const SIZE_STEP: usize = 5; // creates 8 char string at base 32
        let mut shortest_size: usize = SIZE_STEP;
        let mut shortest = &digest.as_bytes()[..shortest_size];
        let mut digests = self.find_digests(DigestSearchCriteria::StartsWith(
            encoding::PartialDigest::from(shortest),
        ));
        while let Some(other) = digests.next().await {
            match other {
                Err(_) => continue,
                Ok(other) => {
                    if &other.as_bytes()[0..shortest_size] != shortest {
                        continue;
                    }
                    if other == digest {
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

    /// Resolve the complete object digest from a shortened one.
    ///
    /// By default this is an O(n) operation defined by the number of objects.
    /// Other implementations may provide better results.
    ///
    /// # Errors
    /// - UnknownReferenceError: if the digest cannot be resolved
    /// - AmbiguousReferenceError: if the digest could point to multiple objects
    async fn resolve_full_digest(
        &self,
        partial: &encoding::PartialDigest,
    ) -> Result<encoding::Digest> {
        if let Some(digest) = partial.to_digest() {
            return Ok(digest);
        }
        let options: Vec<_> = self
            .find_digests(crate::graph::DigestSearchCriteria::StartsWith(
                partial.clone(),
            ))
            .try_collect()
            .await?;

        match options.len() {
            0 => Err(Error::UnknownReference(partial.to_string())),
            1 => Ok(options.get(0).unwrap().to_owned()),
            _ => Err(Error::AmbiguousReference(partial.to_string())),
        }
    }
}

#[async_trait::async_trait]
impl<T: DatabaseView> DatabaseView for &T {
    async fn read_object(&self, digest: encoding::Digest) -> Result<Object> {
        DatabaseView::read_object(&**self, digest).await
    }

    fn find_digests(
        &self,
        search_criteria: DigestSearchCriteria,
    ) -> Pin<Box<dyn Stream<Item = Result<encoding::Digest>> + Send>> {
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
    /// Write an object to the database, for later retrieval.
    async fn write_object(&self, obj: &Object) -> Result<()>;

    /// Remove an object from the database.
    async fn remove_object(&self, digest: encoding::Digest) -> Result<()>;
}

#[async_trait::async_trait]
impl<T: Database> Database for &T {
    async fn write_object(&self, obj: &Object) -> Result<()> {
        Database::write_object(&**self, obj).await
    }

    async fn remove_object(&self, digest: encoding::Digest) -> Result<()> {
        Database::remove_object(&**self, digest).await
    }
}
