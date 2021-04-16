// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use indicatif::ParallelProgressIterator;
use rayon::prelude::*;

use super::config::load_config;
use crate::prelude::*;
use crate::{graph, storage, tracking, Result};

#[cfg(test)]
#[path = "./sync_test.rs"]
mod sync_test;

pub fn push_ref<R: AsRef<str>>(
    reference: R,
    mut remote: Option<storage::RepositoryHandle>,
) -> Result<graph::Object> {
    let config = load_config()?;
    let local = config.get_repository()?.into();
    let mut remote = match remote.take() {
        Some(remote) => remote,
        None => config.get_remote("origin")?.into(),
    };
    sync_ref(reference, &local, &mut remote)
}

/// Pull a reference to the local repository, searching all configured remotes.
///
/// Args:
/// - reference: The reference to localize
///
/// Errors:
/// - If the remote reference could not be found
pub fn pull_ref<R: AsRef<str>>(reference: R) -> Result<()> {
    let pull_cmd = match super::which_spfs("pull") {
        Some(cmd) => cmd,
        None => return Err("'spfs-pull' command not found in environment".into()),
    };
    let mut cmd = std::process::Command::new(pull_cmd);
    cmd.arg(reference.as_ref());
    tracing::debug!("{:?}", cmd);
    let status = cmd.status()?;
    if let Some(0) = status.code() {
        Ok(())
    } else {
        Err("pull failed".into())
    }
}

pub fn sync_ref<R: AsRef<str>>(
    reference: R,
    src: &storage::RepositoryHandle,
    dest: &mut storage::RepositoryHandle,
) -> Result<graph::Object> {
    let tag = if let Ok(tag) = tracking::TagSpec::parse(reference.as_ref()) {
        match src.resolve_tag(&tag) {
            Ok(tag) => Some(tag),
            Err(err) => return Err(err),
        }
    } else {
        None
    };

    let obj = src.read_ref(reference.as_ref())?;
    sync_object(&obj, src, dest)?;
    if let Some(tag) = tag {
        tracing::debug!(tag = ?tag.path(), "syncing tag");
        dest.push_raw_tag(&tag)?;
    }
    tracing::debug!(target = ?reference.as_ref(), "sync complete");
    Ok(obj)
}

pub fn sync_object<'a>(
    obj: &'a graph::Object,
    src: &'a storage::RepositoryHandle,
    dest: &'a mut storage::RepositoryHandle,
) -> Result<()> {
    use graph::Object;
    match obj {
        Object::Layer(obj) => sync_layer(obj, src, dest),
        Object::Platform(obj) => sync_platform(obj, src, dest),
        Object::Blob(obj) => sync_blob(obj, src, dest),
        Object::Manifest(obj) => sync_manifest(obj, src, dest),
        Object::Mask | Object::Tree(_) => Ok(()),
    }
}

pub fn sync_platform(
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
        sync_object(&obj, src, dest)?;
    }

    dest.write_object(&graph::Object::Platform(platform.clone()))
}

pub fn sync_layer(
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
    sync_manifest(&manifest, &src, dest)?;
    dest.write_object(&graph::Object::Layer(layer.clone()))?;
    Ok(())
}

pub fn sync_manifest(
    manifest: &graph::Manifest,
    src: &storage::RepositoryHandle,
    dest: &mut storage::RepositoryHandle,
) -> Result<()> {
    let manifest_digest = manifest.digest()?;
    if dest.has_manifest(&manifest_digest) {
        tracing::info!(digest = ?manifest_digest, "manifest already synced");
        return Ok(());
    }

    tracing::debug!(digest = ?manifest_digest, "syncing manifest");
    let entries: Vec<_> = manifest
        .list_entries()
        .into_iter()
        .filter(|e| e.kind.is_blob())
        .collect();
    let style = indicatif::ProgressStyle::default_bar()
        .template("       {msg} [{bar:40}] {pos:>7}/{len:7}")
        .progress_chars("=>-");
    let bar = indicatif::ProgressBar::new(entries.len() as u64).with_style(style.clone());
    bar.set_message("syncing manifest");
    let src_address = &src.address();
    let dest_address = &dest.address();
    let results: Vec<_> = entries
        .par_iter()
        .progress_with(bar)
        .map(move |entry| {
            let src = storage::open_repository(src_address)?;
            let mut dest = storage::open_repository(dest_address)?;
            sync_entry(entry.clone(), &src, &mut dest)
        })
        .collect();

    let errors: Vec<_> = results
        .into_iter()
        .filter_map(|res| if let Err(err) = res { Some(err) } else { None })
        .collect();

    if errors.len() > 0 {
        return Err(format!(
            "{:?}, and {} more errors during sync",
            errors[0],
            errors.len() - 1
        )
        .into());
    }

    dest.write_object(&graph::Object::Manifest(manifest.clone()))?;
    Ok(())
}

fn sync_entry(
    entry: &graph::Entry,
    src: &storage::RepositoryHandle,
    dest: &mut storage::RepositoryHandle,
) -> Result<()> {
    if !entry.kind.is_blob() {
        return Ok(());
    }
    let blob = graph::Blob {
        payload: entry.object,
        size: entry.size,
    };
    sync_blob(&blob, src, dest)
}

fn sync_blob(
    blob: &graph::Blob,
    src: &storage::RepositoryHandle,
    dest: &mut storage::RepositoryHandle,
) -> Result<()> {
    if dest.has_payload(&blob.payload) {
        tracing::trace!(digest = ?blob.payload, "blob payload already synced");
    } else {
        let mut payload = src.open_payload(&blob.payload)?;
        tracing::debug!(digest = ?blob.payload, "syncing payload");
        dest.write_data(Box::new(&mut *payload))?;
    }
    dest.write_blob(blob.clone())?;
    Ok(())
}
