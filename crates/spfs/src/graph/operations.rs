// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashSet;

use tokio_stream::StreamExt;

use super::database::DatabaseView;
use crate::storage::PayloadStorage;
use crate::{Digest, Error};

/// Validate that all objects can be loaded and their children are accessible.
pub async fn check_database_integrity<'db>(
    db: impl DatabaseView + PayloadStorage + 'db,
    refs: Vec<Digest>,
) -> Vec<Error> {
    if refs.is_empty() {
        iter_all_objects(db).await
    } else {
        refs.iter().map(async |r| {
            walk_root_all_objects(db, r).await
        }).collect::<Vec<_>>().await.into_iter().flatten().collect()
    }
}

async fn iter_all_objects<'db>(db: impl DatabaseView + PayloadStorage + 'db) -> Vec<Error> {
    let mut errors = Vec::new();
    let mut visited = HashSet::new();
    let mut objects = db.iter_objects();
    while let Some(obj) = objects.next().await {
        match obj {
            Err(err) => errors.push(format!("Error in iter_objects: {err}").into()),
            Ok((_digest, obj)) => {
                for digest in obj.child_objects() {
                    if visited.contains(&digest) {
                        continue;
                    }
                    visited.insert(digest);
                    match db.read_object(digest).await {
                        Err(err @ Error::UnknownObject(_)) => errors.push(err),
                        Err(err) => {
                            errors.push(format!("Error reading object {digest}: {err}").into())
                        }
                        Ok(obj) if obj.has_payload() => match db.open_payload(digest).await {
                            Err(Error::UnknownObject(_)) => {
                                errors.push(Error::ObjectMissingPayload(obj, digest))
                            }
                            Err(err) => {
                                errors.push(format!("Error opening payload {digest}: {err}").into())
                            }
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

async fn walk_root_all_objects<'db>(
    db: impl DatabaseView + PayloadStorage + 'db,
    root: Digest,
) -> Vec<Error> {
    let mut errors = Vec::new();
    let mut visited = HashSet::new();
    let mut objects = db.walk_objects(&root);
    while let Some(obj) = objects.next().await {
        match obj {
            Err(err) => errors.push(format!("Error in walk_objects: {err}").into()),
            Ok((digest, _)) => {
                if visited.contains(&digest) {
                    continue;
                }
                visited.insert(digest);
                match db.read_object(digest).await {
                    Err(err @ Error::UnknownObject(_)) => errors.push(err),
                    Err(err) => errors.push(format!("Error reading object {digest}: {err}").into()),
                    Ok(obj) if obj.has_payload() => match db.open_payload(digest).await {
                        Err(Error::UnknownObject(_)) => {
                            errors.push(Error::ObjectMissingPayload(obj, digest))
                        }
                        Err(err) => {
                            errors.push(format!("Error opening payload {digest}: {err}").into())
                        }
                        Ok(_) => (),
                    },
                    Ok(_) => (),
                }
            }
        }
    }
    errors
}
