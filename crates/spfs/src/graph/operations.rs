// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashSet;

use super::database::DatabaseView;
use crate::storage::PayloadStorage;
use crate::Error;

/// Validate that all objects can be loaded and their children are accessible.
pub fn check_database_integrity<'db>(db: impl DatabaseView + PayloadStorage + 'db) -> Vec<Error> {
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
                        Ok(obj) if obj.has_payload() => match db.open_payload(&digest) {
                            Err(Error::UnknownObject(_)) => errors.push(
                                format!("{} object missing payload: {}", obj.to_string(), digest)
                                    .into(),
                            ),
                            Err(err) => errors.push(err),
                            Ok(_) => (),
                        },
                        Ok(_) => (),
                    }
                }
            }
        }
    }
    errors
}
