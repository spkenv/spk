// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashSet;

use indicatif::ParallelProgressIterator;
use rayon::prelude::*;

use crate::{encoding, storage, Error, Result};

#[cfg(test)]
#[path = "./clean_test.rs"]
mod clean_test;

/// Clean all untagged objects from the given repo.
pub fn clean_untagged_objects(repo: &storage::RepositoryHandle) -> Result<()> {
    let unattached = get_all_unattached_objects(repo)?;
    if unattached.len() == 0 {
        tracing::info!("nothing to clean!");
    } else {
        tracing::info!("removing orphaned data");
        let count = unattached.len();
        purge_objects(&unattached.iter().collect(), repo)?;
        tracing::info!("cleaned {} objects", count);
    }
    Ok(())
}

/// Remove the identified objects from the given repository.
pub fn purge_objects(
    objects: &Vec<&encoding::Digest>,
    repo: &storage::RepositoryHandle,
) -> Result<()> {
    let repo = &repo.address();
    let style = indicatif::ProgressStyle::default_bar()
        .template("       {msg:<21} [{bar:40}] {pos:>7}/{len:7}")
        .progress_chars("=>-");
    let bar = indicatif::ProgressBar::new(objects.len() as u64).with_style(style.clone());
    bar.set_message("1/3 cleaning objects");
    let mut results: Vec<_> = objects
        .par_iter()
        .progress_with(bar)
        .map(|digest| {
            let res = clean_object(repo, digest.clone());
            if let Ok(_) = res {
                tracing::trace!(?digest, "successfully removed object");
            }
            res
        })
        .collect();
    let bar = indicatif::ProgressBar::new(objects.len() as u64).with_style(style.clone());
    bar.set_message("2/3 cleaning payloads");
    results.append(
        &mut objects
            .par_iter()
            .progress_with(bar)
            .map(|digest| {
                let res = clean_payload(repo, digest.clone());
                if let Ok(_) = res {
                    tracing::trace!(?digest, "successfully removed payload");
                }
                res
            })
            .collect(),
    );
    let bar = indicatif::ProgressBar::new(objects.len() as u64).with_style(style.clone());
    bar.set_message("3/3 cleaning renders");
    results.append(
        &mut objects
            .par_iter()
            .progress_with(bar)
            .map(|digest| {
                let res = clean_render(repo, digest.clone());
                if let Ok(_) = res {
                    tracing::trace!(?digest, "successfully removed render");
                }
                res
            })
            .collect(),
    );

    let errors: Vec<_> = results
        .iter()
        .filter_map(|res| if let Err(err) = res { Some(err) } else { None })
        .collect();

    if errors.len() > 0 {
        let msg = format!(
            "{:?}, and {} more errors during clean",
            errors[0],
            errors.len() - 1
        );
        return Err(msg.into());
    } else {
        Ok(())
    }
}

fn clean_object(repo_addr: &url::Url, digest: &encoding::Digest) -> Result<()> {
    let mut repo = storage::open_repository(repo_addr)?;
    let res = repo.remove_object(&digest);
    if let Err(Error::UnknownObject(_)) = res {
        Ok(())
    } else {
        res
    }
}

fn clean_payload(repo_addr: &url::Url, digest: &encoding::Digest) -> Result<()> {
    let mut repo = storage::open_repository(repo_addr)?;
    let res = repo.remove_payload(&digest);
    if let Err(Error::UnknownObject(_)) = res {
        Ok(())
    } else {
        res
    }
}

fn clean_render(repo_addr: &url::Url, digest: &encoding::Digest) -> Result<()> {
    let repo = storage::open_repository(repo_addr)?;
    let viewer = repo.renders()?;
    let res = viewer.remove_rendered_manifest(&digest);
    if let Err(crate::Error::UnknownObject(_)) = res {
        Ok(())
    } else {
        res
    }
}

pub fn get_all_unattached_objects(
    repo: &storage::RepositoryHandle,
) -> Result<HashSet<encoding::Digest>> {
    tracing::info!("evaluating repository digraph");
    let mut digests = HashSet::new();
    for digest in repo.iter_digests() {
        digests.insert(digest?);
    }
    let attached = &get_all_attached_objects(repo)?;
    Ok(digests.difference(&attached).map(|d| d.clone()).collect())
}

pub fn get_all_unattached_payloads(
    repo: &storage::RepositoryHandle,
) -> Result<HashSet<encoding::Digest>> {
    tracing::info!("searching for orphaned payloads");
    let mut orphaned_payloads = HashSet::new();
    for digest in repo.iter_payload_digests() {
        let digest = digest?;
        match repo.read_blob(&digest) {
            Err(Error::UnknownObject(_)) => {
                orphaned_payloads.insert(digest);
            }
            Err(err) => return Err(err.into()),
            Ok(_) => continue,
        }
    }
    Ok(orphaned_payloads)
}

pub fn get_all_attached_objects(
    repo: &storage::RepositoryHandle,
) -> Result<HashSet<encoding::Digest>> {
    let mut to_process = Vec::new();
    for item in repo.iter_tag_streams() {
        let (_, stream) = item?;
        for tag in stream {
            to_process.push(tag.target);
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
                tracing::debug!(digest = ?digest, "walking");
                let obj = match repo.read_object(&digest) {
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
                reachable_objects.insert(digest.clone());
            }
        }
    }

    Ok(reachable_objects)
}
