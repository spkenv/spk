// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashSet;

use tokio_stream::StreamExt;

use super::database::DatabaseView;
use crate::storage::PayloadStorage;
use crate::Error;

/// Validate that all objects can be loaded and their children are accessible.
pub async fn check_database_integrity<'db>(
    db: impl DatabaseView + PayloadStorage + 'db,
) -> Vec<Error> {
    let mut errors = Vec::new();
    let mut visited = HashSet::new();
    let mut objects = db.iter_objects();
    while let Some(obj) = objects.next().await {
        match obj {
            Err(err) => errors.push(format!("Error in iter_objects: {}", err).into()),
            Ok((_digest, obj)) => {
                for digest in obj.child_objects() {
                    if visited.contains(&digest) {
                        continue;
                    }
                    visited.insert(digest);
                    match db.read_object(&digest).await {
                        Err(err) => {
                            errors.push(format!("Error reading object {}: {}", &digest, err).into())
                        }
                        Ok(obj) if obj.has_payload() => match db.open_payload(&digest) {
                            Err(Error::UnknownObject(_)) => errors.push(
                                format!("{} object missing payload: {}", obj.to_string(), digest)
                                    .into(),
                            ),
                            Err(err) => errors
                                .push(format!("Error opening payload {}: {}", &digest, err).into()),
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
