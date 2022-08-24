// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::{collections::HashSet, os::linux::fs::MetadataExt, sync::Arc};

use tokio_stream::StreamExt;

use crate::{encoding, storage, Error, Result};

#[cfg(test)]
#[path = "./clean_test.rs"]
mod clean_test;

/// Clean all untagged objects from the given repo.
pub async fn clean_untagged_objects(repo: &storage::RepositoryHandle) -> Result<()> {
    let unattached = get_all_unattached_objects(repo).await?;
    if unattached.is_empty() {
        tracing::info!("nothing to clean!");
    } else {
        tracing::info!("removing orphaned data");
        let count = unattached.len();
        purge_objects(&unattached.iter().collect::<Vec<_>>(), repo).await?;
        tracing::info!("cleaned {count} objects");
    }
    Ok(())
}

/// Remove the identified objects from the given repository.
///
/// # Errors
/// - [`Error::IncompleteClean`]: An accumulation of any errors hit during the prune process
pub async fn purge_objects(
    objects: &[&encoding::Digest],
    repo: &storage::RepositoryHandle,
) -> Result<()> {
    let repo = &repo.address();
    let style = indicatif::ProgressStyle::default_bar()
        .template("       {msg:<21} [{bar:40}] {pos:>7}/{len:7}")
        .progress_chars("=>-");
    let obj_count = objects.len() as u64;
    let multibar = std::sync::Arc::new(indicatif::MultiProgress::new());
    let obj_bar = multibar.add(indicatif::ProgressBar::new(obj_count));
    obj_bar.set_style(style.clone());
    obj_bar.set_message("cleaning objects");
    let payload_bar = multibar.add(indicatif::ProgressBar::new(obj_count));
    payload_bar.set_style(style.clone());
    payload_bar.set_message("cleaning payloads");
    let render_bar = multibar.add(indicatif::ProgressBar::new(obj_count));
    render_bar.set_style(style);
    render_bar.set_message("cleaning renders");
    let mut errors = Vec::new();

    let bars_future = tokio::task::spawn_blocking(move || multibar.join());
    let map_err = |e| Error::String(format!("Unexpected error in clean process: {e}"));

    let repo = Arc::new(crate::open_repository(repo).await?);
    let renders_for_all_users = Arc::new(repo.renders_for_all_users()?);

    // we still do each of these pieces separately, because we'd like
    // to ensure that objects are removed successfully before any
    // related payloads, etc...
    let mut futures: futures::stream::FuturesUnordered<_> = objects
        .iter()
        .map(|digest| tokio::spawn(clean_object(Arc::clone(&repo), **digest)))
        .collect();
    while let Some(result) = futures.next().await {
        if let Err(err) = result.map_err(map_err).and_then(|e| e) {
            errors.push(err);
        }
        obj_bar.inc(1);
    }
    obj_bar.finish();

    let mut futures: futures::stream::FuturesUnordered<_> = objects
        .iter()
        .map(|digest| tokio::spawn(clean_payload(Arc::clone(&repo), **digest)))
        .collect();
    while let Some(result) = futures.next().await {
        if let Err(err) = result.map_err(map_err).and_then(|e| e) {
            errors.push(err);
        }
        payload_bar.inc(1);
    }
    payload_bar.finish();

    let mut futures: futures::stream::FuturesUnordered<_> = objects
        .iter()
        .map(|digest| tokio::spawn(clean_render(Arc::clone(&renders_for_all_users), **digest)))
        .collect();
    while let Some(result) = futures.next().await {
        if let Err(err) = result.map_err(map_err).and_then(|e| e) {
            errors.push(err);
        }
        render_bar.inc(1);
    }
    render_bar.finish();

    let mut futures: futures::stream::FuturesUnordered<_> = renders_for_all_users
        .iter()
        .filter_map(|bastion| {
            bastion
                .bastion_path()
                .map(|bastion_path| tokio::spawn(clean_bastion(bastion_path.to_owned())))
        })
        .collect();
    while let Some(result) = futures.next().await {
        if let Err(err) = result.map_err(map_err).and_then(|e| e) {
            errors.push(err);
        }
    }

    match bars_future.await {
        Err(err) => tracing::warn!("{err}"),
        Ok(Err(err)) => tracing::warn!("{err}"),
        _ => (),
    }

    if !errors.is_empty() {
        Err(Error::IncompleteClean { errors })
    } else {
        Ok(())
    }
}

async fn clean_object(
    repo: Arc<storage::RepositoryHandle>,
    digest: encoding::Digest,
) -> Result<()> {
    let res = repo.remove_object(digest).await;
    if let Err(Error::UnknownObject(_)) = res {
        Ok(())
    } else {
        res
    }
}

async fn clean_payload(
    repo: Arc<storage::RepositoryHandle>,
    digest: encoding::Digest,
) -> Result<()> {
    let res = repo.remove_payload(digest).await;
    if let Err(Error::UnknownObject(_)) = res {
        Ok(())
    } else {
        res
    }
}

async fn clean_render(
    renders_for_all_users: Arc<Vec<Box<dyn storage::ManifestViewer>>>,
    digest: encoding::Digest,
) -> Result<()> {
    let mut result = None;
    for viewer in renders_for_all_users.iter() {
        match viewer.remove_rendered_manifest(digest).await {
            Ok(_) | Err(crate::Error::UnknownObject(_)) => continue,
            err @ Err(_) => {
                // Remember this error but attempt to clean all the users.
                result = Some(err);
            }
        }
    }
    result.unwrap_or(Ok(()))
}

#[async_recursion::async_recursion]
async fn clean_bastion(bastion_path: std::path::PathBuf) -> Result<()> {
    // Any files in the bastion area that have a st_nlink count of 1 are unused
    // and can be removed.
    let mut iter = tokio::fs::read_dir(&bastion_path)
        .await
        .map_err(|err| Error::StorageReadError(bastion_path.clone(), err))?;
    while let Some(entry) = iter
        .next_entry()
        .await
        .map_err(|err| Error::StorageReadError(bastion_path.clone(), err))?
    {
        let file_type = entry
            .file_type()
            .await
            .map_err(|err| Error::StorageReadError(entry.path(), err))?;

        if file_type.is_dir() {
            clean_bastion(entry.path()).await?;
        } else if file_type.is_file() {
            let metadata = entry
                .metadata()
                .await
                .map_err(|err| Error::StorageReadError(entry.path(), err))?;

            if metadata.st_nlink() != 1 {
                continue;
            }

            // This file with st_nlink count of 1 is "safe" to remove. There
            // may be some other process that is about to create a hard link
            // to this file, and will fail if it goes missing.
            tokio::fs::remove_file(entry.path())
                .await
                .map_err(|err| Error::StorageReadError(entry.path(), err))?;
        }
    }
    Ok(())
}

pub async fn get_all_unattached_objects(
    repo: &storage::RepositoryHandle,
) -> Result<HashSet<encoding::Digest>> {
    tracing::info!("evaluating repository digraph");
    let mut digests = HashSet::new();
    let mut digest_stream = repo.find_digests(crate::graph::DigestSearchCriteria::All);
    while let Some(digest) = digest_stream.next().await {
        digests.insert(digest?);
    }
    let attached = &get_all_attached_objects(repo).await?;
    Ok(digests.difference(attached).copied().collect())
}

pub async fn get_all_unattached_payloads(
    repo: &storage::RepositoryHandle,
) -> Result<HashSet<encoding::Digest>> {
    tracing::info!("searching for orphaned payloads");
    let mut orphaned_payloads = HashSet::new();
    let mut payloads = repo.iter_payload_digests();
    while let Some(digest) = payloads.next().await {
        let digest = digest?;
        match repo.read_blob(digest).await {
            Err(Error::UnknownObject(_)) => {
                orphaned_payloads.insert(digest);
            }
            Err(err) => return Err(err),
            Ok(_) => continue,
        }
    }
    Ok(orphaned_payloads)
}

pub async fn get_all_attached_objects(
    repo: &storage::RepositoryHandle,
) -> Result<HashSet<encoding::Digest>> {
    let mut to_process = Vec::new();
    let mut tag_streams = repo.iter_tag_streams();
    while let Some(item) = tag_streams.next().await {
        let (_, mut stream) = item?;
        while let Some(tag) = stream.next().await {
            to_process.push(tag?.target);
        }
    }

    let mut reachable_objects = HashSet::new();
    loop {
        match to_process.pop() {
            None => break,
            Some(digest) => {
                if reachable_objects.contains(&digest) {
                    continue;
                }
                tracing::debug!(?digest, "walking");
                let obj = match repo.read_object(digest).await {
                    Ok(obj) => obj,
                    Err(err) => match err {
                        crate::Error::UnknownObject(err) => {
                            tracing::warn!(?err, "child object missing in database");
                            continue;
                        }
                        _ => return Err(err),
                    },
                };
                to_process.extend(obj.child_objects());
                reachable_objects.insert(digest);
            }
        }
    }

    Ok(reachable_objects)
}
