// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashSet;
use std::path::Path;

use encoding::Encodable;
use itertools::Itertools;
use nonempty::NonEmpty;
use storage::{ManifestStorage, Repository};
use tokio_stream::StreamExt;

use super::config::get_config;
use crate::{encoding, graph, runtime, storage, tracking, Error, Result};

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
    let output = cmd
        .output()
        .await
        .map_err(|err| Error::process_spawn_error("spfs-render".to_owned(), err, None))?;
    eprint!("{}", String::from_utf8_lossy(output.stderr.as_slice()));
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
    pull_from: Option<&storage::RepositoryHandle>,
) -> Result<()> {
    let repo = get_config()?.get_local_repository().await?;
    let mut stack = Vec::new();
    for target in env_spec.iter() {
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
    repo.render_manifest_into_dir(&manifest, &target, storage::fs::RenderType::Copy, pull_from)
        .await
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
///
/// If `skip_runtime_save` is true, the runtime will not be saved, even if
/// the `flattened_layers` property is modified. Only pass true here if the
/// runtime is unconditionally saved shortly after calling this function.
pub(crate) async fn resolve_overlay_dirs(
    runtime: &mut runtime::Runtime,
    repo: &storage::RepositoryHandle,
    skip_runtime_save: bool,
) -> Result<Vec<graph::Manifest>> {
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

    let layers = resolve_stack_to_layers(runtime.status.stack.iter(), Some(repo)).await?;
    let mut manifests = Vec::with_capacity(layers.len());
    for (index, layer) in layers.iter().enumerate() {
        manifests.push(ResolvedManifest::Existing {
            order: index,
            manifest: repo.read_manifest(layer.manifest).await?,
        });
    }

    let renders = repo.renders()?;

    // Determine if layers need to be combined to stay within the length limits
    // of mount args.
    loop {
        let mut overlay_dirs = Vec::with_capacity(manifests.len());
        for manifest in &manifests {
            let rendered_dir = renders.manifest_render_path(manifest.manifest())?;
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
                        manifest.update(&next.unlock());
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
) -> Result<Vec<std::path::PathBuf>> {
    let config = get_config()?;
    let (repo, remote) = tokio::join!(
        config.get_local_repository_handle(),
        crate::config::open_repository_from_string(&config, Some("origin")),
    );
    let repo = repo?;
    let remote = remote.ok();
    let renders = repo.renders()?;

    let manifests = resolve_overlay_dirs(runtime, &repo, skip_runtime_save).await?;

    let mut to_render = HashSet::new();
    for digest in manifests.iter().map(|m| m.digest()) {
        let digest = digest?;
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
                .map_err(|e| Error::String(format!("Unexpected error in render process: {e}")))
                .and_then(|r| r)?;
        }
    }
    let mut overlay_dirs = Vec::with_capacity(manifests.len());
    for manifest in manifests {
        let rendered_dir = renders.render_manifest(&manifest, remote.as_ref()).await?;
        overlay_dirs.push(rendered_dir);
    }

    Ok(overlay_dirs)
}

/// Given a sequence of tags and digests, resolve to the set of underlying layers.
#[async_recursion::async_recursion]
pub async fn resolve_stack_to_layers<D: AsRef<encoding::Digest> + Send>(
    stack: impl Iterator<Item = D> + Send + 'async_recursion,
    mut repo: Option<&'async_recursion storage::RepositoryHandle>,
) -> Result<Vec<graph::Layer>> {
    let owned_handle;
    let repo = match repo.take() {
        Some(repo) => repo,
        None => {
            let config = get_config()?;
            owned_handle = storage::RepositoryHandle::from(config.get_local_repository().await?);
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
