// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use encoding::Encodable;
use futures::{FutureExt, TryFutureExt, TryStreamExt};
use itertools::Itertools;
use nonempty::NonEmpty;
use serde::{Deserialize, Serialize};

use super::config::get_config;
use crate::storage::fallback::FallbackProxy;
use crate::storage::fs::{ManifestRenderPath, RenderSummary};
use crate::storage::prelude::*;
use crate::{encoding, graph, runtime, storage, tracking, Error, Result};

#[cfg(test)]
#[path = "./resolve_test.rs"]
mod resolve_test;

/// Information returned from spfs-render.
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct RenderResult {
    pub paths_rendered: Vec<PathBuf>,
    pub render_summary: storage::fs::RenderSummary,
}

/// Render the given environment in the local repository by
/// calling the `spfs-render` binary (ensuring the necessary
/// privileges are available)
///
/// The return value is defined only if the spfs-render output could be parsed
/// successfully into a [`RenderResult`].
async fn render_via_subcommand(
    spec: tracking::EnvSpec,
    kept_runtime: bool,
) -> Result<Option<RenderResult>> {
    if spec.is_empty() {
        return Ok(Some(RenderResult::default()));
    }

    let render_cmd = match super::which_spfs("render") {
        Some(cmd) => cmd,
        None => return Err(Error::MissingBinary("spfs-render")),
    };
    let mut cmd = tokio::process::Command::new(render_cmd);
    if kept_runtime {
        // Durable runtimes are mounted without the index=on feature
        // of overlayfs. To avoid any issues editing files and
        // hardlinks the rendering for them switches to Copy.
        cmd.arg("--strategy");
        cmd.arg::<&str>(crate::storage::fs::RenderType::Copy.into());
    }
    cmd.arg(spec.to_string());
    tracing::debug!("{:?}", cmd);
    let output = cmd
        .output()
        .await
        .map_err(|err| Error::process_spawn_error("spfs-render", err, None))?;
    let res = match output.status.code() {
        Some(0) => {
            if let Ok(render_result) =
                serde_json::from_slice::<RenderResult>(output.stdout.as_slice())
            {
                Ok(Some(render_result))
            } else {
                // Don't hard error if the output of spfs-render can't be
                // parsed.
                tracing::warn!("Failed to parse output from spfs-render");
                Ok(None)
            }
        }
        _ => Err(Error::process_spawn_error(
            "spfs-render",
            std::io::Error::new(
                std::io::ErrorKind::Other,
                "process exited with non-zero status",
            ),
            None,
        )),
    };
    if res.is_err() {
        // Let's show the user any stderr output from spfs-render.
        let _ = std::io::Write::write_all(&mut std::io::stderr(), &output.stderr);
    }
    res
}

/// Compute or load the spfs manifest representation for a saved reference.
pub async fn compute_manifest<R: AsRef<str>>(reference: R) -> Result<tracking::Manifest> {
    let config = get_config()?;
    let mut repos: Vec<storage::RepositoryHandle> =
        vec![config.get_local_repository().await?.into()];
    for name in config.list_remote_names() {
        match config.get_remote(&name).await {
            Ok(repo) => repos.push(repo),
            Err(err) => {
                tracing::warn!(remote = ?name, "failed to load remote repository");
                tracing::debug!(" > {:?}", err);
            }
        }
    }

    let env = tracking::EnvSpec::parse(reference)?;
    let mut full_manifest = tracking::Manifest::default();
    for tag_spec in env.iter() {
        let mut item_manifest = None;
        for repo in repos.iter() {
            match repo.read_ref(&tag_spec.to_string()).await {
                Ok(obj) => {
                    item_manifest = Some(compute_object_manifest(obj, repo).await?);
                    break;
                }
                Err(Error::UnknownObject(_)) => {
                    tracing::trace!("{repo:?} UnknownObject {tag_spec}");
                    continue;
                }
                Err(Error::UnknownReference(_)) => {
                    tracing::trace!("{repo:?} UnknownReference {tag_spec}");
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

/// Calculate the file manifest for the layers in the given environment spec.
pub async fn compute_environment_manifest(
    env: &tracking::EnvSpec,
    repo: &storage::RepositoryHandle,
) -> Result<tracking::Manifest> {
    let stack_futures: futures::stream::FuturesOrdered<_> = env
        .iter()
        .map(|i| match i {
            tracking::EnvSpecItem::Digest(d) => std::future::ready(Ok(*d)).boxed(),
            tracking::EnvSpecItem::PartialDigest(p) => repo.resolve_full_digest(p).boxed(),
            tracking::EnvSpecItem::TagSpec(t) => repo.resolve_tag(t).map_ok(|t| t.target).boxed(),
        })
        .collect();
    let stack: Vec<_> = stack_futures.try_collect().await?;
    let layers = resolve_stack_to_layers(stack.iter(), Some(repo)).await?;
    let mut manifest = tracking::Manifest::default();
    for layer in layers.iter().rev() {
        manifest.update(
            &repo
                .read_manifest(layer.manifest)
                .await?
                .to_tracking_manifest(),
        )
    }
    Ok(manifest)
}

pub async fn compute_object_manifest(
    obj: graph::Object,
    repo: &storage::RepositoryHandle,
) -> Result<tracking::Manifest> {
    match obj {
        graph::Object::Layer(obj) => Ok(repo
            .read_manifest(obj.manifest)
            .await?
            .to_tracking_manifest()),
        graph::Object::Platform(obj) => {
            let layers = resolve_stack_to_layers(obj.stack.iter(), Some(repo)).await?;
            let mut manifest = tracking::Manifest::default();
            for layer in layers.iter().rev() {
                let layer_manifest = repo.read_manifest(layer.manifest).await?;
                manifest.update(&layer_manifest.to_tracking_manifest());
            }
            Ok(manifest)
        }
        graph::Object::Manifest(obj) => Ok(obj.to_tracking_manifest()),
        obj => Err(format!("Resolve: Unhandled object of type {:?}", obj.kind()).into()),
    }
}

/// Compile the set of directories to be overlaid for a runtime.
///
/// These are returned as a list, from bottom to top.
///
/// If `skip_runtime_save` is true, the runtime will not be saved, even if
/// the `flattened_layers` property is modified. Only pass true here if the
/// runtime is unconditionally saved shortly after calling this function.
pub(crate) async fn resolve_overlay_dirs<R>(
    runtime: &mut runtime::Runtime,
    repo: R,
    skip_runtime_save: bool,
) -> Result<Vec<graph::Manifest>>
where
    R: Repository + ManifestRenderPath,
{
    enum ResolvedManifest {
        Existing {
            order: usize,
            manifest: graph::Manifest,
        },
        Proposed(Box<NonEmpty<ResolvedManifest>>),
    }

    impl ResolvedManifest {
        /// Iterate over all the "existing" manifests contained within this
        /// manifest.
        fn existing(self) -> impl Iterator<Item = graph::Manifest> {
            // Find all the `Existing` manifests in this recursive structure,
            // returning them in an order based on their original order, to
            // preserve the ordering of how manifests will be merged.
            let mut result = Vec::new();
            let mut stack = vec![self];
            while !stack.is_empty() {
                match stack.pop() {
                    Some(ResolvedManifest::Existing { order, manifest }) => {
                        result.push((order, manifest));
                    }
                    Some(ResolvedManifest::Proposed(m)) => stack.extend(m.into_iter()),
                    None => {}
                }
            }
            result.sort_by_key(|(order, _)| *order);
            result.into_iter().map(|(_, m)| m)
        }

        fn manifest(&self) -> &graph::Manifest {
            match self {
                ResolvedManifest::Existing { manifest, .. } => manifest,
                ResolvedManifest::Proposed(m) => {
                    // return the first element as a placeholder
                    m.head.manifest()
                }
            }
        }
    }

    let layers = resolve_stack_to_layers_with_repo(runtime.status.stack.iter(), &repo).await?;
    let mut manifests = Vec::with_capacity(layers.len());
    for (index, layer) in layers.iter().enumerate() {
        manifests.push(ResolvedManifest::Existing {
            order: index,
            manifest: repo.read_manifest(layer.manifest).await?,
        });
    }

    // Determine if layers need to be combined to stay within the length limits
    // of mount args.
    #[cfg(unix)]
    loop {
        let mut overlay_dirs = Vec::with_capacity(manifests.len());
        for manifest in &manifests {
            let rendered_dir = repo.manifest_render_path(manifest.manifest())?;
            overlay_dirs.push(rendered_dir);
        }
        if crate::env::get_overlay_args(runtime, overlay_dirs).is_ok() {
            break;
        }
        // Cut the number of layers in half...
        let mut to_flatten = manifests.len() / 2;
        // Infinite loop protection.
        while to_flatten > 1 {
            // How many layers to flatten together as a group. By grouping,
            // the cost of flattening can be amortized compared to doing one
            // layer at a time, while still allowing for a chance of reusing
            // previously flattened layers.
            //
            //     A B C D E F G H I J K L M N O P ...
            //     |---- A' -----|---- H' -----|
            //
            //     A B C D E F G H I J K L Q R S T ...
            //     |---- A' -----|---- H'' ----|
            //
            // In the above example, both would produce the same `A'` flattened
            // layer and its render could be reused.
            const FLATTEN_GROUP_SIZE: usize = 7;

            let flatten_group_length = to_flatten.min(FLATTEN_GROUP_SIZE);

            tracing::debug!("flattening {flatten_group_length} layers into one...");
            let manifest = ResolvedManifest::Proposed(Box::new(
                NonEmpty::from_vec(manifests.drain(0..flatten_group_length).collect()).unwrap(),
            ));

            // Don't store the "proposed" manifest yet. Note that a proposed
            // manifest can get merged into another manifest in a further
            // iteration, so this manifest may never need to be written.
            manifests.insert(0, manifest);

            to_flatten -= flatten_group_length;
        }
    }

    let mut resolved_manifests = Vec::with_capacity(manifests.len());
    let mut flattened_layers = HashSet::new();
    for manifest in manifests.into_iter() {
        match manifest {
            ResolvedManifest::Existing { manifest, .. } => resolved_manifests.push(manifest),
            ResolvedManifest::Proposed(m) => {
                let mut manifest = tracking::Manifest::default();
                for next in m.into_iter() {
                    for next in next.existing() {
                        manifest.update(&next.to_tracking_manifest());
                    }
                }
                let manifest = graph::Manifest::from(&manifest);
                // Store the newly created manifest so that the render process
                // can read it back. This little dance avoid an expensive
                // (300 ms) clone.
                let object = manifest.into();
                repo.write_object(&object).await?;
                flattened_layers.insert(object.digest().expect("Object has valid digest"));
                match object {
                    graph::Object::Manifest(m) => resolved_manifests.push(m),
                    _ => unreachable!(),
                }
            }
        }
    }

    // Note the layers we manufactured here via flattening so they will have a
    // strong reference in the runtime.
    if !skip_runtime_save && runtime.status.flattened_layers != flattened_layers {
        // If the additional layers has changed, then the runtime needs to be
        // re-saved.
        runtime.status.flattened_layers = flattened_layers;
        runtime.save_state_to_storage().await?;
    }

    Ok(resolved_manifests)
}

/// Compile the set of directories to be overlayed for a runtime, and
/// render them.
///
/// These are returned as a list, from bottom to top.
///
/// If `skip_runtime_save` is true, the runtime will not be saved, even if
/// the `flattened_layers` property is modified. Only pass true here if the
/// runtime is unconditionally saved shortly after calling this function.
pub(crate) async fn resolve_and_render_overlay_dirs(
    runtime: &mut runtime::Runtime,
    skip_runtime_save: bool,
) -> Result<RenderResult> {
    let config = get_config()?;
    let (repo, remotes) =
        tokio::try_join!(config.get_opened_local_repository(), config.list_remotes())?;
    let fallback_repo = FallbackProxy::new(repo, remotes);

    let manifests = resolve_overlay_dirs(runtime, &fallback_repo, skip_runtime_save).await?;
    let to_render = manifests.iter().map(|m| m.digest()).try_collect()?;
    match render_via_subcommand(to_render, runtime.config.durable).await? {
        Some(render_result) => Ok(render_result),
        None => {
            // If we couldn't parse the value printed by spfs-render, calculate
            // the paths rendered here.
            let paths_rendered = manifests
                .iter()
                .map(|m| fallback_repo.manifest_render_path(m))
                .try_collect()?;
            Ok(RenderResult {
                paths_rendered,
                render_summary: RenderSummary::default(),
            })
        }
    }
}

/// Given a sequence of tags and digests, resolve to the set of underlying layers.
pub async fn resolve_stack_to_layers<'iter, 'repo, D, I>(
    stack: I,
    mut repo: Option<&'repo storage::RepositoryHandle>,
) -> Result<Vec<graph::Layer>>
where
    I: Iterator<Item = D> + Send + 'iter,
    D: AsRef<encoding::Digest> + Send,
{
    let owned_handle;
    let repo = match repo.take() {
        Some(repo) => repo,
        None => {
            let config = get_config()?;
            owned_handle = storage::RepositoryHandle::from(config.get_local_repository().await?);
            &owned_handle
        }
    };
    match repo {
        storage::RepositoryHandle::FS(r) => resolve_stack_to_layers_with_repo(stack, r).await,
        storage::RepositoryHandle::Tar(r) => resolve_stack_to_layers_with_repo(stack, r).await,
        storage::RepositoryHandle::Rpc(r) => resolve_stack_to_layers_with_repo(stack, r).await,
        storage::RepositoryHandle::FallbackProxy(r) => {
            resolve_stack_to_layers_with_repo(stack, &**r).await
        }
        storage::RepositoryHandle::Proxy(r) => resolve_stack_to_layers_with_repo(stack, &**r).await,
    }
}

/// See [`resolve_stack_to_layers`].
#[async_recursion::async_recursion]
pub async fn resolve_stack_to_layers_with_repo<I, D, R>(
    stack: I,
    repo: R,
) -> Result<Vec<graph::Layer>>
where
    I: Iterator<Item = D> + Send + 'async_recursion,
    D: AsRef<encoding::Digest> + Send,
    R: storage::Repository + Send + Sync + Copy + 'async_recursion,
{
    let mut layers = Vec::new();
    for reference in stack {
        let reference = reference.as_ref();
        let entry = repo.read_ref(reference.to_string().as_str()).await?;
        match entry {
            graph::Object::Layer(layer) => layers.push(layer),
            graph::Object::Platform(platform) => {
                let mut expanded =
                    resolve_stack_to_layers_with_repo(platform.stack.clone().into_iter(), repo)
                        .await?;
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
    #[cfg(unix)]
    let command = format!("spfs-{}", subcommand.as_ref());
    #[cfg(windows)]
    let command = format!("spfs-{}.exe", subcommand.as_ref());
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

/// Find a command by name
///
/// On windows, the .exe extension must be present or
/// the executable will never be found
pub fn which<S: AsRef<str>>(name: S) -> Option<std::path::PathBuf> {
    let path = std::env::var("PATH").unwrap_or_else(|_| "".to_string());
    let search_paths = std::env::split_paths(&path);
    let name = name.as_ref();
    #[cfg(windows)]
    if !name.ends_with(".exe") {
        return None;
    };
    for path in search_paths {
        let filepath = path.join(name);
        if is_exe(&filepath) {
            return Some(filepath);
        }
    }
    None
}

#[cfg(windows)]
#[inline]
fn is_exe<P: AsRef<Path>>(filepath: P) -> bool {
    use std::ffi::OsStr;

    let filepath = filepath.as_ref();
    filepath.extension() == Some(OsStr::new("exe")) && filepath.is_file()
}

#[cfg(unix)]
fn is_exe<P: AsRef<Path>>(filepath: P) -> bool {
    use faccess::PathExt;

    if !filepath.as_ref().is_file() {
        false
    } else {
        filepath.as_ref().executable()
    }
}
