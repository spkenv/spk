use std::collections::HashSet;

use super::database::DatabaseView;
use super::Error;
use crate::encoding;

/// Validate that all objects can be loaded and their children are accessible.
pub fn check_database_integrity<'db>(db: impl DatabaseView<'db>) -> Vec<Error> {
    let errors = Vec::new();
    let visited: HashSet<&encoding::Digest> = Default::default();
    for obj in db.iter_objects() {
        for digest in obj.child_objects() {
            if visited.contains(digest) {
                continue;
            }
            visited.insert(digest);
            match db.read_object(digest) {
                Err(err) => errors.push(err),
                Ok(_) => (),
            }
        }
    }
    errors
}
