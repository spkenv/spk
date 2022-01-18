// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::{collections::HashSet, path::Path};

use itertools::Itertools;
use tokio_stream::StreamExt;

use super::config::load_config;
use crate::{encoding, graph, runtime, storage, tracking, Error, Result};
use encoding::Encodable;
use storage::{ManifestStorage, Repository};

#[cfg(test)]
#[path = "./resolve_test.rs"]
mod resolve_test;

/// Render the given environment in the local repository
///
/// All items in the spec will be merged and rendered, so
/// it's usually best to only include one thing in the spec if
/// building up layers for use in an spfs runtime
pub async fn render(spec: tracking::EnvSpec) -> Result<std::path::PathBuf> {
    use std::os::unix::ffi::OsStrExt;
    let render_cmd = match super::which_spfs("render") {
        Some(cmd) => cmd,
        None => return Err(Error::MissingBinary("spfs-render")),
    };
    let mut cmd = tokio::process::Command::new(render_cmd);
    cmd.arg(spec.to_string());
    tracing::debug!("{:?}", cmd);
    let output = cmd.output().await?;
    let mut bytes = output.stdout.as_slice();
    while let Some(b) = bytes.strip_suffix(&[b'\n']) {
        bytes = b
    }
    match output.status.code() {
        Some(0) => Ok(std::path::PathBuf::from(std::ffi::OsStr::from_bytes(bytes))),
        _ => {
            let stderr = std::ffi::OsStr::from_bytes(output.stderr.as_slice());
            Err(format!("render failed:\n{}", stderr.to_string_lossy()).into())
        }
    }
}

/// Render a set of layers into an arbitrary target directory.
///
/// This method runs in the current thread and creates a copy
/// of the desired data in the target directory
pub async fn render_into_directory(
    env_spec: &tracking::EnvSpec,
    target: impl AsRef<std::path::Path>,
) -> Result<()> {
    let repo = load_config()?.get_repository().await?;
    let mut stack = Vec::new();
    for target in &env_spec.items {
        let target = target.to_string();
        let obj = repo.read_ref(target.as_str()).await?;
        stack.push(obj.digest()?);
    }
    let layers = resolve_stack_to_layers(stack.iter(), None).await?;
    let mut manifests = Vec::with_capacity(layers.len());
    for layer in layers {
        manifests.push(repo.read_manifest(layer.manifest).await?);
    }
    let mut manifest = tracking::Manifest::default();
    for next in manifests.into_iter() {
        manifest.update(&next.unlock());
    }
    let manifest = graph::Manifest::from(&manifest);
    repo.render_manifest_into_dir(&manifest, &target, storage::fs::RenderType::Copy)
        .await
}

/// Compute or load the spfs manifest representation for a saved reference.
pub async fn compute_manifest<R: AsRef<str>>(reference: R) -> Result<tracking::Manifest> {
    let config = load_config()?;
    let mut repos: Vec<storage::RepositoryHandle> = vec![config.get_repository().await?.into()];
    for name in config.list_remote_names() {
        match config.get_remote(&name).await {
            Ok(repo) => repos.push(repo),
            Err(err) => {
                tracing::warn!(remote = ?name, "failed to load remote repository");
                tracing::debug!(" > {:?}", err);
            }
        }
    }

    let env = tracking::EnvSpec::new(reference.as_ref())?;
    let mut full_manifest = tracking::Manifest::default();
    for tag_spec in env.items {
        let mut item_manifest = None;
        for repo in repos.iter() {
            match repo.read_ref(&tag_spec.to_string()).await {
                Ok(obj) => {
                    item_manifest = Some(compute_object_manifest(obj, repo).await?);
                    break;
                }
                Err(Error::UnknownObject(_)) => {
                    tracing::trace!("{:?} UnknownObject {}", repo, tag_spec);
                    continue;
                }
                Err(Error::UnknownReference(_)) => {
                    tracing::trace!("{:?} UnknownReference {}", repo, tag_spec);
                    continue;
                }
                Err(err) => return Err(err),
            }
        }
        if let Some(item_manifest) = item_manifest {
            full_manifest.update(&item_manifest);
        } else {
            return Err(Error::UnknownReference(tag_spec.to_string()));
        }
    }
    Ok(full_manifest)
}

pub async fn compute_object_manifest(
    obj: graph::Object,
    repo: &storage::RepositoryHandle,
) -> Result<tracking::Manifest> {
    match obj {
        graph::Object::Layer(obj) => Ok(repo.read_manifest(obj.manifest).await?.unlock()),
        graph::Object::Platform(obj) => {
            let layers = resolve_stack_to_layers(obj.stack.iter(), Some(repo)).await?;
            let mut manifest = tracking::Manifest::default();
            for layer in layers.iter().rev() {
                let layer_manifest = repo.read_manifest(layer.manifest).await?;
                manifest.update(&layer_manifest.unlock());
            }
            Ok(manifest)
        }
        graph::Object::Manifest(obj) => Ok(obj.unlock()),
        obj => Err(format!("Resolve: Unhandled object of type {:?}", obj.kind()).into()),
    }
}

/// Compile the set of directories to be overlayed for a runtime.
///
/// These are returned as a list, from bottom to top.
pub async fn resolve_overlay_dirs(runtime: &runtime::Runtime) -> Result<Vec<std::path::PathBuf>> {
    let config = load_config()?;
    let mut repo = config.get_repository().await?.into();
    let mut overlay_dirs = Vec::new();
    let layers = resolve_stack_to_layers(runtime.get_stack().iter(), Some(&repo)).await?;
    let mut manifests = Vec::with_capacity(layers.len());
    for layer in layers {
        manifests.push(repo.read_manifest(layer.manifest).await?);
    }
    if manifests.len() > config.filesystem.max_layers {
        let to_flatten = manifests.len() - config.filesystem.max_layers as usize;
        tracing::debug!("flattening {} layers into one...", to_flatten);
        let mut manifest = tracking::Manifest::default();
        for next in manifests.drain(0..to_flatten) {
            manifest.update(&next.unlock());
        }
        let manifest = graph::Manifest::from(&manifest);
        // store the newly created manifest so that the render process can read it back
        repo.write_object(&manifest.clone().into()).await?;
        manifests.insert(0, manifest);
    }

    let renders = repo.renders()?;
    let mut to_render = HashSet::new();
    for digest in manifests.iter().map(|m| m.digest().unwrap()) {
        if !renders.has_rendered_manifest(digest).await {
            to_render.insert(digest);
        }
    }
    if !to_render.is_empty() {
        tracing::info!("{} layers require rendering", to_render.len());

        let style = indicatif::ProgressStyle::default_bar()
            .template("       {msg} [{bar:40}] {pos:>7}/{len:7}")
            .progress_chars("=>-");
        let bar = indicatif::ProgressBar::new(to_render.len() as u64).with_style(style);
        bar.set_message("rendering layers");
        let mut futures: futures::stream::FuturesUnordered<_> = to_render
            .into_iter()
            .map(move |digest| tokio::spawn(render(digest.into())))
            .collect();
        while let Some(result) = futures.next().await {
            bar.inc(1);
            result
                .map_err(|e| Error::String(format!("Unexpected error in render process: {}", e)))
                .and_then(|r| r)?;
        }
    }
    for manifest in manifests {
        let rendered_dir = renders.render_manifest(&manifest).await?;
        overlay_dirs.push(rendered_dir);
    }

    Ok(overlay_dirs)
}

/// Given a sequence of tags and digests, resolve to the set of underlying layers.
#[async_recursion::async_recursion(?Send)]
pub async fn resolve_stack_to_layers<D: AsRef<encoding::Digest>>(
    stack: impl Iterator<Item = D> + 'async_recursion,
    mut repo: Option<&'async_recursion storage::RepositoryHandle>,
) -> Result<Vec<graph::Layer>> {
    let owned_handle;
    let repo = match repo.take() {
        Some(repo) => repo,
        None => {
            let config = load_config()?;
            owned_handle = storage::RepositoryHandle::from(config.get_repository().await?);
            &owned_handle
        }
    };

    let mut layers = Vec::new();
    for reference in stack {
        let reference = reference.as_ref();
        let entry = repo.read_ref(reference.to_string().as_str()).await?;
        match entry {
            graph::Object::Layer(layer) => layers.push(layer),
            graph::Object::Platform(platform) => {
                let mut expanded =
                    resolve_stack_to_layers(platform.stack.clone().into_iter(), Some(repo)).await?;
                layers.append(&mut expanded);
            }
            graph::Object::Manifest(manifest) => {
                layers.push(graph::Layer::new(manifest.digest().unwrap()))
            }
            obj => {
                return Err(format!(
                    "Cannot resolve object into a mountable filesystem layer: {:?}",
                    obj.kind()
                )
                .into())
            }
        }
    }

    // ensure that there are not duplicated layers in the final set
    // because overlayfs will die if the same directory is included
    // more than once
    Ok(layers.into_iter().unique().collect_vec())
}

/// Find an spfs-* subcommand in the current environment
pub fn which_spfs<S: AsRef<str>>(subcommand: S) -> Option<std::path::PathBuf> {
    let command = format!("spfs-{}", subcommand.as_ref());
    if let Some(path) = which(&command) {
        return Some(path);
    }
    if let Ok(mut path) = std::env::current_exe() {
        path.set_file_name(&command);
        if is_exe(&path) {
            return Some(path);
        }
    }
    None
}

/// Find a command
pub fn which<S: AsRef<str>>(name: S) -> Option<std::path::PathBuf> {
    let path = std::env::var("PATH").unwrap_or_else(|_| "".to_string());
    let search_paths = path.split(':');
    for path in search_paths {
        let filepath = Path::new(path).join(name.as_ref());
        if is_exe(&filepath) {
            return Some(filepath);
        }
    }
    None
}

fn is_exe<P: AsRef<Path>>(filepath: P) -> bool {
    use faccess::PathExt;

    if !filepath.as_ref().is_file() {
        false
    } else {
        filepath.as_ref().executable()
    }
}
