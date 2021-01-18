use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use futures::future::{FutureExt, LocalBoxFuture};

use super::config::get_config;
use crate::prelude::*;
use crate::{graph, storage, tracking, Error, Result};

#[cfg(test)]
#[path = "./sync_test.rs"]
mod sync_test;

static SYNC_LOG_UPDATE_INTERVAL_SECONDS: std::time::Duration = Duration::from_secs(2);

pub async fn push_ref<R: AsRef<str>>(
    reference: R,
    mut remote: Option<storage::RepositoryHandle>,
) -> Result<graph::Object> {
    let config = get_config()?;
    let local = config.get_repository()?.into();
    let mut remote = match remote.take() {
        Some(remote) => remote,
        None => config.get_remote("origin")?.into(),
    };
    sync_ref(reference, &local, &mut remote).await
}

/// Pull a reference to the local repository, searching all configured remotes.
///
/// Args:
/// - reference: The reference to localize
///
/// Errors:
/// - If the remote reference could not be found
pub async fn pull_ref<R: AsRef<str>>(reference: R) -> Result<graph::Object> {
    let config = get_config()?;
    let mut local = config.get_repository()?.into();
    let names = config.list_remote_names();
    for name in names {
        tracing::debug!(
            reference = %reference.as_ref(),
            remote = %name,
            "looking for reference"
        );
        let remote = match config.get_remote(&name) {
            Ok(remote) => remote,
            Err(err) => {
                tracing::warn!(remote = %name, "failed to load remote repository");
                tracing::warn!(" > {:?}", err);
                continue;
            }
        };
        if remote.has_ref(reference.as_ref()) {
            return sync_ref(reference, &remote, &mut local).await;
        }
    }
    Err(graph::UnknownReferenceError::new(reference))
}

pub async fn sync_ref<R: AsRef<str>>(
    reference: R,
    src: &storage::RepositoryHandle,
    dest: &mut storage::RepositoryHandle,
) -> Result<graph::Object> {
    let tag = if let Ok(tag) = tracking::TagSpec::parse(reference.as_ref()) {
        match src.resolve_tag(&tag) {
            Ok(tag) => Some(tag),
            Err(Error::UnknownObject(_)) => None,
            Err(err) => return Err(err),
        }
    } else {
        None
    };

    let obj = src.read_ref(reference.as_ref())?;
    sync_object(&obj, src, dest).await?;
    if let Some(tag) = tag {
        dest.push_raw_tag(&tag)?;
    }
    Ok(obj)
}

pub fn sync_object<'a>(
    obj: &'a graph::Object,
    src: &'a storage::RepositoryHandle,
    dest: &'a mut storage::RepositoryHandle,
) -> futures::future::LocalBoxFuture<'a, Result<()>> {
    async move {
        use graph::Object;
        match obj {
            Object::Layer(obj) => sync_layer(obj, src, dest).await,
            Object::Platform(obj) => sync_platform(obj, src, dest).await,
            Object::Blob(obj) => {
                tracing::info!(digest = ?obj.digest(), "syncing blob");
                let mut reader = src.open_payload(&obj.digest())?;
                dest.commit_blob(Box::new(&mut *reader))?;
                Ok(())
            }
            Object::Mask | Object::Manifest(_) | Object::Tree(_) => Ok(()),
        }
    }
    .boxed_local()
}

pub async fn sync_platform(
    platform: &graph::Platform,
    src: &storage::RepositoryHandle,
    dest: &mut storage::RepositoryHandle,
) -> Result<()> {
    let digest = platform.digest()?;
    if dest.has_platform(&digest) {
        tracing::debug!(digest = ?digest, "platform already synced");
        return Ok(());
    }
    tracing::info!(digest = ?digest, "syncing platform");
    for digest in &platform.stack {
        let obj = src.read_object(&digest)?;
        sync_object(&obj, src, dest).await?;
    }

    dest.write_object(&graph::Object::Platform(platform.clone()))
}

pub async fn sync_layer(
    layer: &graph::Layer,
    src: &storage::RepositoryHandle,
    dest: &mut storage::RepositoryHandle,
) -> Result<()> {
    let layer_digest = layer.digest()?;
    if dest.has_layer(&layer_digest) {
        tracing::debug!(digest = ?layer_digest, "layer already synced");
        return Ok(());
    }

    tracing::info!(digest = ?layer_digest, "syncing layer");
    let manifest = src.read_manifest(&layer.manifest)?;

    let entries: Vec<_> = manifest
        .iter_entries()
        .into_iter()
        .filter(|e| !e.kind.is_blob())
        .collect();
    let spawn_count = entries.len() as u64;
    let current_count = Arc::new(AtomicU64::new(0));
    let mut futures: Vec<LocalBoxFuture<Result<()>>> = Vec::new();
    for entry in entries {
        let current_count = current_count.clone();
        let src_address = src.address();
        let dest_address = dest.address();
        futures.push(
            async move {
                let res = sync_entry(entry, src_address, dest_address).await;
                current_count.fetch_add(1, Ordering::Relaxed);
                res
            }
            .boxed_local(),
        )
    }

    futures.push(
        async move {
            let mut last_report = Instant::now();
            while current_count.load(Ordering::Relaxed) < spawn_count {
                tokio::time::sleep(Duration::from_millis(200)).await;
                let current_count = current_count.load(Ordering::Relaxed);
                let now = Instant::now();

                if now - last_report > SYNC_LOG_UPDATE_INTERVAL_SECONDS {
                    let percent_done = (current_count as f64 / spawn_count as f64) * 100.0;
                    let progress_message =
                        format!("{:.02}% ({}/{})", percent_done, current_count, spawn_count);
                    tracing::info!(progress = ?progress_message, "syncing layer data...");
                    last_report = now;
                }
            }
            Ok(())
        }
        .boxed_local(),
    );

    let results = futures::future::join_all(futures).await;
    let errors: Vec<_> = results
        .into_iter()
        .filter_map(|res| if let Err(err) = res { Some(err) } else { None })
        .collect();

    if errors.len() > 0 {
        return Err(format!(
            "{:?}, and {} more errors during clean",
            errors[0],
            errors.len() - 1
        )
        .into());
    }

    dest.write_object(&graph::Object::Manifest(manifest))?;
    dest.write_object(&graph::Object::Layer(layer.clone()))?;
    Ok(())
}

async fn sync_entry<S: AsRef<str>>(
    entry: &graph::Entry,
    src_address: S,
    dest_address: S,
) -> Result<()> {
    let src = storage::open_repository(src_address)?;
    let dest = storage::open_repository(dest_address)?;
    sync_entry_local(entry, src.to_repo(), dest.to_repo()).await
}

async fn sync_entry_local(
    entry: &graph::Entry,
    src: Box<dyn storage::Repository>,
    mut dest: Box<dyn storage::Repository>,
) -> Result<()> {
    if entry.kind.is_blob() {
        return Ok(());
    }

    if !dest.has_object(&entry.object) {
        let object = src.read_object(&entry.object)?;
        dest.write_object(&object)?;
    }

    if dest.has_payload(&entry.object) {
        tracing::trace!(digest = ?entry.object, "blob payload already synced");
    } else {
        let mut payload = src.open_payload(&entry.object)?;
        tracing::debug!(digest = ?entry.object, "syncing payload");
        dest.write_data(Box::new(&mut *payload))?;
    }
    Ok(())
}
