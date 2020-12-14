use std::collections::VecDeque;

use super::{AmbiguousReferenceError, Error, InvalidReferenceError, Result, UnknownReferenceError};
use crate::{encoding, tracking};

/// Walks an object tree depth-first starting at some root digest
pub struct DatabaseWalker<'db, DB>
where
    DB: DatabaseView + ?Sized,
{
    db: &'db DB,
    queue: VecDeque<encoding::Digest>,
}

impl<'db, DB> DatabaseWalker<'db, DB>
where
    DB: DatabaseView + ?Sized,
{
    /// Create an iterator that yields all child objects starting at root
    /// from the given database.
    ///
    /// # Errors
    /// The same as [`DatabaseView::read_object`]
    pub fn new(db: &'db DB, root: encoding::Digest) -> Self {
        let mut queue = VecDeque::new();
        queue.push_back(root);
        DatabaseWalker {
            db: db,
            queue: queue,
        }
    }
}

impl<'db, DB> Iterator for DatabaseWalker<'db, DB>
where
    DB: DatabaseView + Sized,
{
    type Item = Result<(encoding::Digest, &'db tracking::Object)>;

    fn next(&mut self) -> Option<Self::Item> {
        let next = self.queue.pop_front();
        match &next {
            None => None,
            Some(next) => {
                let obj = self.db.read_object(&next);
                match obj {
                    Ok(obj) => {
                        for digest in obj.child_objects() {
                            self.queue.push_back(*digest);
                        }
                        Some(Ok((next.clone(), &obj)))
                    }
                    Err(err) => Some(Err(err)),
                }
            }
        }
    }
}

/// Iterates all objects in a database, in no particular order
pub struct DatabaseIterator<'db, DB>
where
    DB: DatabaseView + ?Sized,
{
    db: &'db DB,
    inner: Box<dyn Iterator<Item = &'db encoding::Digest>>,
}

impl<'db, DB> DatabaseIterator<'db, DB>
where
    DB: DatabaseView + ?Sized,
{
    /// Create an iterator that yields all child objects starting at root
    /// from the given database.
    ///
    /// # Errors
    /// The same as [`DatabaseView::read_object`]
    pub fn new(db: &'db DB) -> Self {
        DatabaseIterator {
            db: db,
            inner: db.iter_digests(),
        }
    }
}

impl<'db, DB> Iterator for DatabaseIterator<'db, DB>
where
    DB: DatabaseView + Sized,
{
    type Item = Result<(encoding::Digest, &'db tracking::Object)>;

    fn next(&mut self) -> Option<Self::Item> {
        let next = self.inner.next();
        match next {
            None => None,
            Some(next) => {
                let obj = self.db.read_object(&next);
                match obj {
                    Ok(obj) => Some(Ok((next.clone(), &obj))),
                    Err(err) => Some(Err(err)),
                }
            }
        }
    }
}

/// A read-only object database.
pub trait DatabaseView {
    /// Read information about the given object from the database.
    ///
    /// # Errors:
    /// - [`super::UnknownObjectError`]: if the object is not in this database
    fn read_object<'db>(&'db self, digest: &encoding::Digest) -> Result<&'db tracking::Object>;

    /// Iterate all the object digests in this database.
    fn iter_digests<'db>(&'db self) -> Box<dyn Iterator<Item = &'db encoding::Digest>>;

    /// Return true if this database contains the identified object
    fn has_object(&self, digest: &encoding::Digest) -> bool {
        if let Ok(_) = self.read_object(digest) {
            true
        } else {
            false
        }
    }

    /// Iterate all the object in this database.
    fn iter_objects<'db>(&'db self) -> DatabaseIterator<'db, Self> {
        DatabaseIterator::new(self)
    }

    /// Walk all objects connected to the given root object.
    fn walk_objects<'db>(&'db self, root: &encoding::Digest) -> DatabaseWalker<'db, Self> {
        DatabaseWalker::new(self, root.clone())
    }

    /// Return the shortened version of the given digest.
    ///
    /// By default this is an O(n) operation defined by the number of objects.
    /// Other implemntations may provide better results.
    fn get_shortened_digest(&self, digest: &encoding::Digest) -> String {
        const SIZE_STEP: usize = 5; // creates 8 char string at base 32
        let mut shortest_size: usize = SIZE_STEP;
        let mut shortest = &digest.as_ref()[..shortest_size];
        for other in self.iter_digests() {
            if &other.as_ref()[..shortest_size] != shortest {
                continue;
            }
            if other == digest {
                continue;
            }
            while &other.as_ref()[..shortest_size] == shortest {
                shortest_size += SIZE_STEP;
                shortest = &digest.as_ref()[..shortest_size];
            }
        }
        data_encoding::BASE32.encode(shortest)
    }

    /// Resolve the complete object digest from a shortened one.
    ///
    /// By default this is an O(n) operation defined by the number of objects.
    /// Other implemntations may provide better results.
    ///
    /// # Errors
    /// - UnknownReferenceError: if the digest cannot be resolved
    /// - AmbiguousReferenceError: if the digest could point to multiple objects
    fn resolve_full_digest(&self, short_digest: &str) -> Result<encoding::Digest> {
        let decoded = data_encoding::BASE32
            .decode(short_digest.as_bytes())
            .map_err(|_| Error::InvalidReferenceError(InvalidReferenceError::new(short_digest)))?;
        let mut options = Vec::new();
        for digest in self.iter_digests() {
            if &digest.as_ref()[..decoded.len()] == decoded {
                options.push(digest)
            }
        }

        match options.len() {
            0 => Err(UnknownReferenceError::new(short_digest.to_string()).into()),
            1 => Ok(*options.get(0).unwrap().to_owned()),
            _ => Err(AmbiguousReferenceError::new(short_digest.to_string()).into()),
        }
    }
}

pub trait Database: DatabaseView {
    /// Write an object to the database, for later retrieval.
    fn write_object(&mut self, obj: &tracking::Object);

    /// Remove an object from the database.
    fn remove_object(&mut self, digest: &encoding::Digest);
}
