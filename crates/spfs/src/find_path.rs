// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use async_recursion::async_recursion;
use relative_path::RelativePath;
use spfs_encoding::{Digest, Encodable};

use crate::graph::{self, Object};
use crate::{env, status, storage, tracking, Error, Result};

/// Used for items in a list of spfs objects that contain a filepath.
/// The parent containers down to the filepath will be graph objects.
/// The filepath itself will be a manifest node entry.
#[derive(Debug, Clone)]
pub enum ObjectPathEntry {
    /// A parent container along the spfs object path to a file
    Parent(graph::Object),

    /// A filepath (dir or file) at the end of an spfs object
    /// path. This contains a tracking Entry, not a graph Entry,
    /// because that's what walking a graph::Manifest after an
    /// unlock() call will return.
    FilePath(tracking::Entry),
}

impl ObjectPathEntry {
    pub fn digest(&self) -> Result<Digest> {
        match self {
            ObjectPathEntry::Parent(obj) => match obj {
                Object::Platform(obj) => obj.digest(),
                Object::Layer(obj) => obj.digest(),
                Object::Manifest(obj) => obj.digest(),
                Object::Blob(obj) => Ok(obj.digest()),
                Object::Tree(obj) => obj.digest(),
                Object::Mask => Err(Error::String("spfs Mask object has no digest".to_string())),
            },
            ObjectPathEntry::FilePath(entry) => Ok(entry.object),
        }
    }
}

pub type ObjectPath = Vec<ObjectPathEntry>;

/// Finds all the spfs object paths to the objects that provide the
/// entry for the given filepaths in the current spfs runtime.
/// Returns tuple of a boolean for whether we are in an active spfs
/// runtime or not, and a list of all the spfs object paths (as lists)
/// that end in the entry for the given filepath.
pub async fn find_path_providers_in_spfs_runtime(
    filepath: &str,
    repo: &storage::RepositoryHandle,
) -> Result<(bool, Vec<ObjectPath>)> {
    let mut found: Vec<ObjectPath> = Vec::new();
    let mut in_a_runtime = true;

    if let Ok(runtime) = status::active_runtime().await {
        for digest in runtime.status.stack.iter() {
            let item = repo.read_object(*digest).await?;
            let file_data = find_path_in_spfs_item(filepath, &item, repo).await?;
            if !file_data.is_empty() {
                found.extend(file_data);
            }
        }
    } else {
        in_a_runtime = false;
    }

    Ok((in_a_runtime, found))
}

/// Returns a list of spfs object paths (as lists) from the given spfs
/// object that lead to the an entry for the given filepath. Returns
/// an empty list if the filepath is not found in (provided by) the
/// spfs object or any of its child objects.
#[async_recursion]
async fn find_path_in_spfs_item(
    filepath: &str,
    obj: &Object,
    repo: &storage::RepositoryHandle,
) -> Result<Vec<ObjectPath>> {
    let mut paths: Vec<ObjectPath> = Vec::new();

    match obj {
        Object::Platform(obj) => {
            for reference in obj.stack.iter() {
                let item = repo.read_object(*reference).await?;
                let paths_to_file = find_path_in_spfs_item(filepath, &item, repo).await?;
                for path in paths_to_file {
                    let mut new_path: ObjectPath = Vec::new();
                    new_path.push(ObjectPathEntry::Parent(Object::Platform(obj.clone())));
                    new_path.extend(path);
                    paths.push(new_path);
                }
            }
        }

        Object::Layer(obj) => {
            let item = repo.read_object(obj.manifest).await?;
            let paths_to_file = find_path_in_spfs_item(filepath, &item, repo).await?;
            for path in paths_to_file {
                let mut new_path: ObjectPath = Vec::new();
                new_path.push(ObjectPathEntry::Parent(Object::Layer(obj.clone())));
                new_path.extend(path);
                paths.push(new_path);
            }
        }

        Object::Manifest(obj) => {
            let path = RelativePath::new(filepath);

            for node in obj.to_tracking_manifest().walk_abs(env::SPFS_DIR) {
                if node.path == path {
                    let new_path = vec![
                        ObjectPathEntry::Parent(Object::Manifest(obj.clone())),
                        ObjectPathEntry::FilePath(node.entry.clone()),
                    ];
                    paths.push(new_path);
                    break;
                }
            }
        }

        Object::Blob(_) | Object::Tree(_) | Object::Mask => {
            // These are not examined here when searching for the
            // filepath because the filepath will be found by walking
            // Manifest objects.
        }
    };

    Ok(paths)
}
