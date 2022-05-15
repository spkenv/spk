// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use tokio_stream::StreamExt;

use super::config::get_config;
use crate::prelude::*;
use crate::{graph, storage, tracking, Error, Result};

#[cfg(test)]
#[path = "./sync_test.rs"]
mod sync_test;

/// Limits the concurrency in sync operations to avoid
/// connection and open file descriptor limits
// TODO: load this from the config
static MAX_CONCURRENT: usize = 256;

pub async fn push_ref<R: AsRef<str>>(
    reference: R,
    remote: Option<storage::RepositoryHandle>,
) -> Result<graph::Object> {
    let config = get_config()?;
    let local = config.get_repository().await?.into();
    let remote = match remote {
        Some(remote) => remote,
        None => config.get_remote("origin").await?,
    };
    sync_ref(reference, &local, &remote).await
}

/// Pull a reference to the local repository, searching all configured remotes.
///
/// On linux the pull process can require special process privileges, this function
/// spawns a new process with those privileges and should be used instead of the
/// sync_reference under most circumstances.
///
/// Args:
/// - reference: The reference to localize
///
/// Errors:
/// - If the remote reference could not be found
pub async fn pull_ref<R: AsRef<str>>(reference: R) -> Result<()> {
    let pull_cmd = match super::which_spfs("pull") {
        Some(cmd) => cmd,
        None => return Err(Error::MissingBinary("spfs-pull")),
    };
    let mut cmd = tokio::process::Command::new(pull_cmd);
    cmd.arg(reference.as_ref());
    tracing::debug!("{:?}", cmd);
    let status = cmd.status().await?;
    if let Some(0) = status.code() {
        Ok(())
    } else {
        Err("pull failed".into())
    }
}

pub async fn sync_ref<R: AsRef<str>>(
    reference: R,
    src: &storage::RepositoryHandle,
    dest: &storage::RepositoryHandle,
) -> Result<graph::Object> {
    let tag = if let Ok(tag) = tracking::TagSpec::parse(reference.as_ref()) {
        match src.resolve_tag(&tag).await {
            Ok(tag) => Some(tag),
            Err(Error::UnknownReference(_)) => None,
            Err(err) => return Err(err),
        }
    } else {
        None
    };

    let obj = src.read_ref(reference.as_ref()).await?;
    sync_object(&obj, src, dest).await?;
    if let Some(tag) = tag {
        tracing::debug!(tag = ?tag.path(), "syncing tag");
        dest.push_raw_tag(&tag).await?;
    }
    tracing::debug!(target = ?reference.as_ref(), "sync complete");
    Ok(obj)
}

#[async_recursion::async_recursion]
pub async fn sync_object<'a>(
    obj: &'a graph::Object,
    src: &'a storage::RepositoryHandle,
    dest: &'a storage::RepositoryHandle,
) -> Result<()> {
    use graph::Object;
    match obj {
        Object::Layer(obj) => sync_layer(obj, src, dest).await,
        Object::Platform(obj) => sync_platform(obj, src, dest).await,
        Object::Blob(obj) => sync_blob(obj, src, dest).await,
        Object::Manifest(obj) => sync_manifest(obj, src, dest).await,
        Object::Mask | Object::Tree(_) => Ok(()),
    }
}

pub async fn sync_platform(
    platform: &graph::Platform,
    src: &storage::RepositoryHandle,
    dest: &storage::RepositoryHandle,
) -> Result<()> {
    let digest = platform.digest()?;
    if dest.has_platform(digest).await {
        tracing::debug!(?digest, "platform already synced");
        return Ok(());
    }
    tracing::info!(?digest, "syncing platform");
    for digest in &platform.stack {
        let obj = src.read_object(*digest).await?;
        sync_object(&obj, src, dest).await?;
    }

    dest.write_object(&graph::Object::Platform(platform.clone()))
        .await
}

pub async fn sync_layer(
    layer: &graph::Layer,
    src: &storage::RepositoryHandle,
    dest: &storage::RepositoryHandle,
) -> Result<()> {
    let layer_digest = layer.digest()?;
    if dest.has_layer(layer_digest).await {
        tracing::debug!(digest = ?layer_digest, "layer already synced");
        return Ok(());
    }

    tracing::info!(digest = ?layer_digest, "syncing layer");
    let manifest = src.read_manifest(layer.manifest).await?;
    sync_manifest(&manifest, src, dest).await?;
    dest.write_object(&graph::Object::Layer(layer.clone()))
        .await?;
    Ok(())
}

pub async fn sync_manifest(
    manifest: &graph::Manifest,
    src: &storage::RepositoryHandle,
    dest: &storage::RepositoryHandle,
) -> Result<()> {
    let manifest_digest = manifest.digest()?;
    if dest.has_manifest(manifest_digest).await {
        tracing::info!(digest = ?manifest_digest, "manifest already synced");
        return Ok(());
    }

    tracing::debug!(digest = ?manifest_digest, "syncing manifest");
    let entries: Vec<_> = manifest
        .list_entries()
        .into_iter()
        .cloned()
        .filter(|e| e.kind.is_blob())
        .collect();
    let style = indicatif::ProgressStyle::default_bar()
        .template("      {msg} [{bar:40}] {bytes:>7}/{total_bytes:7}")
        .progress_chars("=>-");
    let total_bytes = entries.iter().fold(0, |c, e| c + e.size);
    let bar = indicatif::ProgressBar::new(total_bytes).with_style(style);
    bar.set_message("syncing manifest");
    let mut results = Vec::with_capacity(entries.len());
    let mut futures =
        futures::stream::FuturesUnordered::<tokio::task::JoinHandle<Result<u64>>>::new();
    for entry in entries {
        while futures.len() >= MAX_CONCURRENT {
            if let Some(res) = futures.next().await {
                let res = res
                    .map_err(|err| Error::String(format!("Sync task failed unexpectedly: {}", err)))
                    .and_then(|e| e);
                if let Ok(size) = res {
                    bar.inc(size);
                }
                results.push(res);
            }
        }
        let dest_address = dest.address();
        let src_address = src.address();
        let future = tokio::spawn(async move {
            let src = crate::open_repository(src_address).await?;
            let dest = crate::open_repository(dest_address).await?;
            sync_entry(&entry, &src, &dest).await?;
            Ok(entry.size)
        });
        futures.push(future);
    }
    while let Some(res) = futures.next().await {
        let res = res
            .map_err(|err| Error::String(format!("Sync task failed unexpectedly: {}", err)))
            .and_then(|e| e);
        if let Ok(size) = res {
            bar.inc(size);
        }
        results.push(res);
    }
    bar.finish();

    let errors: Vec<_> = results
        .into_iter()
        .filter_map(|res| if let Err(err) = res { Some(err) } else { None })
        .collect();

    if !errors.is_empty() {
        return Err(format!(
            "{:?}, and {} more errors during sync",
            errors[0],
            errors.len() - 1
        )
        .into());
    }

    dest.write_object(&graph::Object::Manifest(manifest.clone()))
        .await?;
    Ok(())
}

async fn sync_entry(
    entry: &graph::Entry,
    src: &storage::RepositoryHandle,
    dest: &storage::RepositoryHandle,
) -> Result<()> {
    if !entry.kind.is_blob() {
        return Ok(());
    }
    let blob = graph::Blob {
        payload: entry.object,
        size: entry.size,
    };
    sync_blob(&blob, src, dest).await
}

async fn sync_blob(
    blob: &graph::Blob,
    src: &storage::RepositoryHandle,
    dest: &storage::RepositoryHandle,
) -> Result<()> {
    if dest.has_payload(blob.payload).await {
        tracing::trace!(digest = ?blob.payload, "blob payload already synced");
    } else {
        let payload = src.open_payload(blob.payload).await?;
        tracing::debug!(digest = ?blob.payload, "syncing payload");
        let (digest, _) = dest.write_data(payload).await?;
        if digest != blob.payload {
            return Err(Error::String(format!(
                "Source repository provided blob that did not match the requested digest: wanted {}, got {digest}",
                blob.payload,
            )));
        }
    }
    dest.write_blob(blob.clone()).await?;
    Ok(())
}
