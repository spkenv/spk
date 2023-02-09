// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashSet;
use std::os::linux::fs::MetadataExt;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use futures::FutureExt;
use tokio_stream::StreamExt;

use crate::storage::RepositoryHandle;
use crate::{encoding, storage, Error, Result};

#[cfg(test)]
#[path = "./clean_test.rs"]
mod clean_test;

/// Clean all untagged objects from the given repo.
pub async fn clean_untagged_objects(
    older_than: DateTime<Utc>,
    repo: &storage::RepositoryHandle,
    dry_run: bool,
) -> Result<()> {
    let unattached = get_all_unattached_objects(repo).await?;
    if unattached.is_empty() {
        tracing::info!("nothing to clean!");
    } else {
        tracing::info!("removing orphaned data");
        let count = unattached.len();
        purge_objects(
            older_than,
            &unattached.iter().collect::<Vec<_>>(),
            repo,
            None,
            dry_run,
        )
        .await?;
        tracing::info!("cleaned {count} objects");
    }
    Ok(())
}

/// Remove the identified objects from the given repository.
///
/// Objects younger than the given threshold will not be touched.
///
/// If the set of all attached objects is provided, also purge renders of
/// objects that are no longer attached.
///
/// # Errors
/// - [`Error::IncompleteClean`]: An accumulation of any errors hit during the prune process
pub async fn purge_objects(
    older_than: DateTime<Utc>,
    objects: &[&encoding::Digest],
    repo: &storage::RepositoryHandle,
    attached_objects: Option<&HashSet<encoding::Digest>>,
    dry_run: bool,
) -> Result<()> {
    let repo = &repo.address();
    let repo = Arc::new(crate::open_repository(repo).await?);
    let renders_for_all_users = Arc::new(match &*repo {
        RepositoryHandle::FS(r) => r.renders_for_all_users()?,
        _ => Vec::new(),
    });

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
    render_bar.set_style(style.clone());
    render_bar.set_message("cleaning renders");
    let user_render_bar = attached_objects.as_ref().map(|_| {
        let bar = multibar.add(indicatif::ProgressBar::new(
            renders_for_all_users.len().try_into().unwrap_or(u64::MAX),
        ));
        bar.set_style(style);
        bar.set_message("cleaning user renders");
        bar
    });

    let mut errors = Vec::new();

    let bars_future = tokio::task::spawn_blocking(move || multibar.join());
    let map_err = |e| Error::String(format!("Unexpected error in clean process: {e}"));

    let mut cleaned_objects = Vec::new();

    // we still do each of these pieces separately, because we'd like
    // to ensure that objects are removed successfully before any
    // related payloads, etc...
    let mut futures: futures::stream::FuturesUnordered<_> = objects
        .iter()
        .map(|digest| {
            tokio::spawn(clean_object(
                older_than,
                Arc::clone(&repo),
                **digest,
                dry_run,
            ))
            .map(move |f| (*digest, f))
        })
        .collect();
    while let Some((digest, result)) = futures.next().await {
        match result.map_err(map_err).and_then(|e| e) {
            Ok(deleted) if deleted => cleaned_objects.push(digest),
            Ok(_) => {}
            Err(err) => errors.push(err),
        };
        obj_bar.inc(1);
    }
    obj_bar.finish();

    // `cleaned_objects` contain the objects that were old enough to be cleaned,
    // only remove the payloads and renders for those.

    let mut futures: futures::stream::FuturesUnordered<_> = cleaned_objects
        .iter()
        .map(|digest| tokio::spawn(clean_payload(Arc::clone(&repo), **digest, dry_run)))
        .collect();
    while let Some(result) = futures.next().await {
        if let Err(err) = result.map_err(map_err).and_then(|e| e) {
            errors.push(err);
        }
        payload_bar.inc(1);
    }
    payload_bar.finish();

    let mut futures: futures::stream::FuturesUnordered<_> = cleaned_objects
        .iter()
        .map(|digest| {
            tokio::spawn(clean_render(
                older_than,
                Arc::clone(&renders_for_all_users),
                **digest,
                dry_run,
            ))
        })
        .collect();
    while let Some(result) = futures.next().await {
        if let Err(err) = result.map_err(map_err).and_then(|e| e) {
            errors.push(err);
        }
        render_bar.inc(1);
    }
    render_bar.finish();

    if let (Some(attached_objects), Some(user_render_bar)) =
        (attached_objects, user_render_bar.as_ref())
    {
        for (username, manifest_viewer) in renders_for_all_users.iter() {
            let mut iter = manifest_viewer.iter_rendered_manifests();
            while let Some(digest) = iter.next().await {
                let digest = match digest {
                    Ok(digest) => digest,
                    Err(Error::NoRenderStorage(_)) => {
                        // This can happen if the renders/<username> directory
                        // is empty.
                        //
                        // Go to next user.
                        break;
                    }
                    Err(err) => {
                        errors.push(Error::String(format!(
                            "Error iterating rendered manifests for user {username}: {err}"
                        )));
                        // Go to next user.
                        break;
                    }
                };

                // Note that if there are a small number of these trace lines
                // output, they might be covered up by the progress bars.
                tracing::trace!(?username, ?digest, "rendered object");

                if attached_objects.contains(&digest) {
                    tracing::trace!(?username, ?digest, "still attached");
                    continue;
                }

                if let Err(err) =
                    clean_render_for_user(older_than, username, manifest_viewer, digest, dry_run)
                        .await
                {
                    errors.push(err);
                }
            }
            user_render_bar.inc(1);
        }
    }
    if let Some(bar) = user_render_bar {
        bar.finish()
    }

    let mut futures: futures::stream::FuturesUnordered<_> = renders_for_all_users
        .iter()
        .filter_map(|(_, proxy)| {
            proxy
                .proxy_path()
                .map(|proxy_path| tokio::spawn(clean_proxy(proxy_path.to_owned(), dry_run)))
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
    older_than: DateTime<Utc>,
    repo: Arc<storage::RepositoryHandle>,
    digest: encoding::Digest,
    dry_run: bool,
) -> Result<bool> {
    if dry_run {
        tracing::info!("remove object: {digest}");
        return Ok(true);
    }

    let res = repo.remove_object_if_older_than(older_than, digest).await;
    if let Err(Error::UnknownObject(_)) = res {
        // Treat this as if the object was deleted, so there is an attempt to
        // delete the payload, etc.
        Ok(true)
    } else {
        res
    }
}

async fn clean_payload(
    repo: Arc<storage::RepositoryHandle>,
    digest: encoding::Digest,
    dry_run: bool,
) -> Result<()> {
    if dry_run {
        tracing::info!("remove payload: {digest}");
        return Ok(());
    }

    let res = repo.remove_payload(digest).await;
    if let Err(Error::UnknownObject(_)) = res {
        Ok(())
    } else {
        res
    }
}

/// Remove any unused proxy files.
///
/// Return true if any files were deleted, or if an empty directory was found.
#[async_recursion::async_recursion]
async fn clean_proxy(proxy_path: std::path::PathBuf, dry_run: bool) -> Result<bool> {
    // Any files in the proxy area that have a st_nlink count of 1 are unused
    // and can be removed.
    let mut files_exist = false;
    let mut files_were_deleted = false;
    let mut iter = tokio::fs::read_dir(&proxy_path).await.map_err(|err| {
        Error::StorageReadError("read_dir on proxy path", proxy_path.clone(), err)
    })?;
    while let Some(entry) = iter.next_entry().await.map_err(|err| {
        Error::StorageReadError("next_entry on proxy path", proxy_path.clone(), err)
    })? {
        files_exist = true;

        let file_type = entry.file_type().await.map_err(|err| {
            Error::StorageReadError("file_type on proxy path entry", entry.path(), err)
        })?;

        if file_type.is_dir() {
            if clean_proxy(entry.path(), dry_run).await? {
                // If some files were deleted, attempt to delete the directory
                // itself. It may now be empty. Ignore any failures.
                if dry_run {
                    tracing::info!("rmdir {}", entry.path().display());
                } else if (tokio::fs::remove_dir(entry.path()).await).is_ok() {
                    files_were_deleted = true;
                }
            }
        } else if file_type.is_file() {
            let metadata = entry.metadata().await.map_err(|err| {
                Error::StorageReadError("metadata on proxy file", entry.path(), err)
            })?;

            if metadata.st_nlink() != 1 {
                continue;
            }

            // This file with st_nlink count of 1 is "safe" to remove. There
            // may be some other process that is about to create a hard link
            // to this file, and will fail if it goes missing.
            if dry_run {
                tracing::info!("rm {}", entry.path().display());
            } else {
                tokio::fs::remove_file(entry.path()).await.map_err(|err| {
                    Error::StorageReadError("remove_file on proxy path entry", entry.path(), err)
                })?;
            }

            files_were_deleted = true;
        }
    }
    Ok(files_were_deleted || !files_exist)
}

async fn clean_render_for_user(
    older_than: DateTime<Utc>,
    username: &String,
    viewer: &storage::fs::FSRepository,
    digest: encoding::Digest,
    dry_run: bool,
) -> Result<()> {
    if dry_run {
        tracing::info!("remove render for user {username}: {digest}");
        return Ok(());
    }

    match viewer
        .remove_rendered_manifest_if_older_than(older_than, digest)
        .await
    {
        Ok(_) | Err(crate::Error::UnknownObject(_)) => Ok(()),
        Err(err) => Err(err),
    }
}

async fn clean_render(
    older_than: DateTime<Utc>,
    renders_for_all_users: Arc<Vec<(String, storage::fs::FSRepository)>>,
    digest: encoding::Digest,
    dry_run: bool,
) -> Result<()> {
    let mut result = None;
    for (username, viewer) in renders_for_all_users.iter() {
        match clean_render_for_user(older_than, username, viewer, digest, dry_run).await {
            Ok(_) => continue,
            err @ Err(_) => {
                // Remember this error but attempt to clean all the users.
                result = Some(err);
            }
        }
    }
    result.unwrap_or(Ok(()))
}

#[derive(Debug)]
pub struct AttachedAndUnattachedObjects {
    pub attached: HashSet<encoding::Digest>,
    pub unattached: HashSet<encoding::Digest>,
}

pub async fn get_all_attached_and_unattached_objects(
    repo: &storage::RepositoryHandle,
) -> Result<AttachedAndUnattachedObjects> {
    tracing::info!("evaluating repository digraph");
    let mut digests = HashSet::new();
    let mut digest_stream = repo.find_digests(crate::graph::DigestSearchCriteria::All);
    while let Some(digest) = digest_stream.next().await {
        digests.insert(digest?);
    }
    let attached = get_all_attached_objects(repo).await?;
    let unattached = digests.difference(&attached).copied().collect();
    Ok(AttachedAndUnattachedObjects {
        attached,
        unattached,
    })
}

pub async fn get_all_unattached_objects(
    repo: &storage::RepositoryHandle,
) -> Result<HashSet<encoding::Digest>> {
    get_all_attached_and_unattached_objects(repo)
        .await
        .map(|objects| objects.unattached)
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
            Err(Error::ObjectNotABlob(..)) => {
                tracing::warn!("Found payload with object that was not a blob: {digest}");
                continue;
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
