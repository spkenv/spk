use std::collections::{HashSet, VecDeque};

use super::{AmbiguousReferenceError, Result, UnknownReferenceError};
use crate::encoding;
use crate::tracking;

/// Walks an object tree depth-first starting at some root digest
pub struct DatabaseWalker<'db, DB>
where
    DB: DatabaseView<'db> + ?Sized,
{
    db: &'db DB,
    queue: VecDeque<encoding::Digest>,
}

impl<'db, DB> DatabaseWalker<'db, DB>
where
    DB: DatabaseView<'db> + ?Sized,
{
    /// Create an iterator that yields all child objects starting at root
    /// from the given database.
    ///
    /// # Errors
    /// The same as [`DatabaseView::read_object`]
    pub fn new(db: &'db DB, root: encoding::Digest) -> Self {
        let queue = VecDeque::new();
        queue.push_back(root);
        DatabaseWalker {
            db: db,
            queue: queue,
        }
    }
}

impl<'db, DB> Iterator for DatabaseWalker<'db, DB>
where
    DB: DatabaseView<'db> + Sized,
{
    type Item = Result<(encoding::Digest, tracking::Object)>;

    fn next(&mut self) -> Option<&'db Self::Item> {
        let next = self.queue.pop_front();
        match next {
            None => None,
            Some(next) => {
                let obj = self.db.read_object(next);
                if let Ok(obj) = obj {
                    for digest in obj.child_objects() {
                        self.queue.push_back(digest);
                    }
                }
                Some((next, obj))
            }
        }
    }
}

/// A read-only object database.
pub trait DatabaseView<'db> {
    type Iterator: Iterator<Item = &'db encoding::Digest>;

    /// Read information about the given object from the database.
    ///
    /// # Errors:
    /// - [`super::UnknownObjectError`]: if the object is not in this database
    fn read_object(&self, digest: encoding::Digest) -> Result<&'db tracking::Object>;

    /// Iterate all the object digests in this database.
    fn iter_digests(&self) -> Self::Iterator;

    /// Return true if this database contains the identified object
    fn has_object(&self, digest: encoding::Digest) -> bool {
        if let Ok(_) = self.read_object(digest) {
            true
        } else {
            false
        }
    }

    /// Iterate all the object in this database.
    fn iter_objects(
        &self,
    ) -> Box<dyn Iterator<Item = Result<(encoding::Digest, tracking::Object)>>> {
        self.iter_digests()
            .map(|digest| (digest, self.read_object(digest)))
    }

    /// Walk all objects connected to the given root object.
    fn walk_objects(&'db self, root: encoding::Digest) -> DatabaseWalker<'db, Self> {
        DatabaseWalker::new(self, root)
    }

    /// Return the shortened version of the given digest.
    ///
    /// By default this is an O(n) operation defined by the number of objects.
    /// Other implemntations may provide better results.
    fn get_shortened_digest(&self, digest: encoding::Digest) -> String {
        const size_step: usize = 5; // creates 8 char string at base 32
        let shortest_size: usize = size_step;
        let shortest = &digest.as_ref()[..shortest_size];
        for other in self.iter_digests() {
            if &other.as_ref()[..shortest_size] != shortest {
                continue;
            }
            if other == &digest {
                continue;
            }
            while &other.as_ref()[..shortest_size] == shortest {
                shortest_size += size_step;
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
    fn resolve_full_digest(&self, short_digest: String) -> Result<encoding::Digest> {
        let decoded = data_encoding::BASE32.decode(short_digest.as_bytes())?;
        let options = Vec::new();
        for digest in self.iter_digests() {
            if &digest.as_ref()[..decoded.len()] == decoded {
                options.push(digest)
            }
        }

        match options.len() {
            0 => Err(UnknownReferenceError::new(short_digest).into()),
            1 => Ok(options.get(0).unwrap().clone()),
            _ => Err(AmbiguousReferenceError::new(short_digest).into()),
        }
    }
}

pub trait Database<'db>: DatabaseView<'db> {
    /// Write an object to the database, for later retrieval.
    fn write_object(&mut self, obj: &tracking::Object);

    /// Remove an object from the database.
    fn remove_object(&mut self, digest: &encoding::Digest);
}
