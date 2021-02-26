use std::{
    collections::HashSet,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use futures::future::{FutureExt, LocalBoxFuture};

use crate::{encoding, storage, Error, Result};

#[cfg(test)]
#[path = "./clean_test.rs"]
mod clean_test;

static _CLEAN_LOG_UPDATE_INTERVAL_SECONDS: Duration = Duration::from_secs(2);

/// Clean all untagged objects from the given repo.
pub async fn clean_untagged_objects(repo: &storage::RepositoryHandle) -> Result<()> {
    let unattached = get_all_unattached_objects(repo)?;
    if unattached.len() == 0 {
        tracing::info!("nothing to clean!");
    } else {
        tracing::info!("removing orphaned data...");
        let count = unattached.len();
        purge_objects(unattached.iter(), repo).await?;
        tracing::info!("cleaned {} objects", count);
    }
    Ok(())
}

/// Remove the identified objects from the given repository.
pub async fn purge_objects(
    objects: impl Iterator<Item = &encoding::Digest>,
    repo: &storage::RepositoryHandle,
) -> Result<()> {
    let mut spawn_count: u64 = 0;
    let current_count = Arc::new(AtomicU64::new(0));
    let mut futures: Vec<LocalBoxFuture<Result<()>>> = Vec::new();
    for digest in objects {
        {
            let current_count = current_count.clone();
            let fut = async move {
                let res = clean_object(repo.address(), digest.clone()).await;
                current_count.fetch_add(1, Ordering::Relaxed);
                if let Ok(_) = res {
                    tracing::trace!(?digest, "successfully removed object");
                }
                res
            }
            .boxed_local();
            futures.push(fut);
        }
        {
            let current_count = current_count.clone();
            let fut = async move {
                let res = clean_payload(repo.address(), digest.clone()).await;
                current_count.fetch_add(1, Ordering::Relaxed);
                if let Ok(_) = res {
                    tracing::trace!(?digest, "successfully removed payload");
                }
                res
            }
            .boxed_local();
            futures.push(fut);
        }
        {
            let current_count = current_count.clone();
            let fut = async move {
                let res = clean_render(repo.address(), digest.clone()).await;
                current_count.fetch_add(1, Ordering::Relaxed);
                if let Ok(_) = res {
                    tracing::trace!(?digest, "successfully removed render");
                }
                res
            }
            .boxed_local();
            futures.push(fut);
        }
        spawn_count += 3;
    }

    futures.insert(
        0,
        async move {
            let mut last_report = Instant::now();
            while current_count.load(Ordering::Relaxed) < spawn_count {
                tokio::time::sleep(Duration::from_millis(100)).await;
                let current_count = current_count.load(Ordering::Relaxed);
                let now = Instant::now();

                if now - last_report > _CLEAN_LOG_UPDATE_INTERVAL_SECONDS {
                    let percent_done = (current_count as f64 / spawn_count as f64) * 100.0;
                    let progress_message =
                        format!("{:.02}% ({}/{})", percent_done, current_count, spawn_count);
                    tracing::info!(progress = %progress_message, "cleaning orphaned data...");
                    last_report = now;
                }
            }
            Ok(())
        }
        .boxed_local(),
    );

    let results = futures::future::join_all(futures).await;
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

async fn clean_object(repo_addr: url::Url, digest: encoding::Digest) -> Result<()> {
    let mut repo = storage::open_repository(repo_addr)?;
    let res = tokio::task::spawn_blocking(move || repo.remove_object(&digest)).await?;
    if let Err(Error::UnknownObject(_)) = res {
        Ok(())
    } else {
        res
    }
}

async fn clean_payload(repo_addr: url::Url, digest: encoding::Digest) -> Result<()> {
    let mut repo = storage::open_repository(repo_addr)?;
    let res = tokio::task::spawn_blocking(move || repo.remove_payload(&digest)).await?;
    if let Err(Error::UnknownObject(_)) = res {
        Ok(())
    } else {
        res
    }
}

async fn clean_render(repo_addr: url::Url, digest: encoding::Digest) -> Result<()> {
    tokio::task::spawn_blocking(move || {
        let repo = storage::open_repository(repo_addr)?;
        let viewer = repo.renders()?;
        let res = viewer.remove_rendered_manifest(&digest);
        if let Err(crate::Error::UnknownObject(_)) = res {
            Ok(())
        } else {
            res
        }
    })
    .await?
}

pub fn get_all_unattached_objects(
    repo: &storage::RepositoryHandle,
) -> Result<HashSet<encoding::Digest>> {
    tracing::info!("evaluating repository digraph...");
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
    tracing::info!("searching for orphaned payloads...");
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
                tracing::debug!(digest = ?digest, "walking...");
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
