use std::collections::HashSet;

use super::database::DatabaseView;
use super::Error;

/// Validate that all objects can be loaded and their children are accessible.
pub fn check_database_integrity<'db>(db: impl DatabaseView + 'db) -> Vec<Error> {
    let mut errors = Vec::new();
    let mut visited = HashSet::new();
    for obj in db.iter_objects() {
        match obj {
            Err(err) => errors.push(err),
            Ok((_digest, obj)) => {
                for digest in obj.child_objects() {
                    if visited.contains(&digest) {
                        continue;
                    }
                    visited.insert(digest.clone());
                    match db.read_object(&digest) {
                        Err(err) => errors.push(err),
                        Ok(_) => (),
                    }
                }
            }
        }
    }
    errors
}
