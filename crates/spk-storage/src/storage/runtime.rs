// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::collections::{HashMap, HashSet};
use std::convert::{TryFrom, TryInto};
use std::io::Read;
use std::sync::Arc;

use itertools::Itertools;
use relative_path::RelativePathBuf;
use spfs::find_path::ObjectPathEntry;
use spfs::io::DigestFormat;
use spfs::prelude::*;
use spk_schema::foundation::ident_build::parse_build;
use spk_schema::foundation::ident_component::Component;
use spk_schema::foundation::name::{PkgName, PkgNameBuf, RepositoryName, RepositoryNameBuf};
use spk_schema::foundation::version::{parse_version, Version};
use spk_schema::ident_build::{Build, EmbeddedSource};
use spk_schema::{BuildIdent, FromYaml, Package, Spec, SpecRecipe, VersionIdent};

use super::repository::{PublishPolicy, Storage};
use super::Repository;
use crate::{Error, Result};

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct RuntimeRepository {
    address: url::Url,
    name: RepositoryNameBuf,
    root: std::path::PathBuf,
}

impl Default for RuntimeRepository {
    fn default() -> Self {
        let root = std::path::PathBuf::from("/spfs/spk/pkg");
        let address = Self::address_from_root(&root);
        Self {
            address,
            name: "runtime".try_into().expect("valid repository name"),
            root,
        }
    }
}

impl Ord for RuntimeRepository {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.address.cmp(&other.address)
    }
}
impl PartialOrd for RuntimeRepository {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl RuntimeRepository {
    fn address_from_root(root: &std::path::PathBuf) -> url::Url {
        let address = format!("runtime://{}", root.display());
        match url::Url::parse(&address) {
            Ok(a) => a,
            Err(err) => {
                tracing::error!(
                    "failed to create valid address for path {:?}: {:?}",
                    root,
                    err
                );
                url::Url::parse(&format!("runtime://{}", root.to_string_lossy()))
                    .expect("Failed to create url from path (fallback)")
            }
        }
    }

    #[cfg(test)]
    pub fn new(root: std::path::PathBuf) -> Self {
        // this function is not allowed outside of testing because read_components
        // makes assumptions about the runtime directory which cannot be
        // reasonably altered
        let address = Self::address_from_root(&root);
        Self {
            address,
            name: "runtime".try_into().expect("valid repository name"),
            root,
        }
    }

    /// Identify the payloads for the identified packages' components.
    ///
    /// Like calling `Repository::read_components` for each package, but more
    /// efficient.
    ///
    /// The components are returned in the same order as the items in `pkgs`.
    pub async fn read_components_bulk(
        &self,
        pkgs: &[&BuildIdent],
    ) -> Result<Vec<HashMap<Component, spfs::encoding::Digest>>> {
        let mut results = Vec::new();
        results.resize_with(pkgs.len(), Default::default);

        enum ComponentMode {
            BackwardsCompatibility,
            RealComponent(spk_schema::ident_component::Component),
        }

        struct ComponentFilenames {
            index: usize,
            filenames: Vec<(ComponentMode, RelativePathBuf)>,
        }

        let mut component_filenames_for_package_f = Vec::new();

        for (index, pkg) in pkgs.iter().enumerate() {
            if let Build::Embedded(EmbeddedSource::Package(_package)) = pkg.build() {
                // An embedded package's components are only accessible
                // via its package spec
                let embedded_spec = self.read_package(pkg).await?;
                let components = embedded_spec
                    .components()
                    .iter()
                    .map(|c| (c.name.clone(), spfs::encoding::EMPTY_DIGEST.into()))
                    .collect::<HashMap<Component, spfs::encoding::Digest>>();

                results[index] = components;
                continue;
            }

            let path = self.root.join(pkg.to_string());
            let pkg_string = pkg.to_string();
            component_filenames_for_package_f.push((
                index,
                tokio::spawn(async move {
                    let entries = get_all_filenames(path).await?;
                    let components: Vec<Component> = entries
                        .into_iter()
                        .filter_map(|n| n.strip_suffix(".cmpt").map(str::to_string))
                        .map(Component::parse)
                        .collect::<spk_schema::foundation::ident_component::Result<_>>()?;

                    let mut path = relative_path::RelativePathBuf::from("/spk/pkg");
                    path.push(pkg_string);

                    let mut filenames_to_resolve_for_index = Vec::new();
                    for name in components.into_iter() {
                        let path = path.join(format!("{}.cmpt", &name));
                        filenames_to_resolve_for_index
                            .push((ComponentMode::RealComponent(name), path));
                    }
                    if filenames_to_resolve_for_index.is_empty() {
                        // This is package was published before component support
                        // was added. It does not have distinct components. So add
                        // default Build and Run components that point at the full
                        // package digest.
                        filenames_to_resolve_for_index
                            .push((ComponentMode::BackwardsCompatibility, path));
                    }
                    Ok::<_, Error>(filenames_to_resolve_for_index)
                }),
            ));
        }

        let mut filenames_to_resolve = Vec::new();

        for (index, component_filenames_f) in component_filenames_for_package_f.into_iter() {
            let filenames = component_filenames_f
                .await
                .map_err(|err| Error::String(format!("Tokio join error: {err}")))??;
            filenames_to_resolve.push(ComponentFilenames { index, filenames });
        }

        // Flatten all the paths that need to be looked up in a deterministic
        // order...

        let filenames_to_resolve_flattened = filenames_to_resolve
            .iter()
            .flat_map(|component_filenames| {
                component_filenames.filenames.iter().map(|(_, path)| path)
            })
            .collect_vec();

        let digests = find_layers_by_filenames(&filenames_to_resolve_flattened)
            .await
            .map_err(|err| {
                if let Error::SPFS(spfs::Error::UnknownReference(path)) = &err {
                    // In order to return a `PackageNotFoundError` we need to look
                    // for what package owned the path in this `UnknownReference`
                    // error.
                    filenames_to_resolve
                        .iter()
                        .find(|component_filenames| {
                            component_filenames
                                .filenames
                                .iter()
                                .any(|(_, p)| *p == *path)
                        })
                        .map(|component_filenames| {
                            Error::SpkValidatorsError(
                                spk_schema::validators::Error::PackageNotFoundError(
                                    (pkgs[component_filenames.index]).to_any(),
                                ),
                            )
                        })
                        .unwrap_or(err)
                } else {
                    err
                }
            })?;

        // Now all the results can be collected via that same order.

        debug_assert_eq!(
            digests.len(),
            filenames_to_resolve_flattened.len(),
            "return value from find_layers_by_filenames expected to match input length"
        );

        for ((component_mode, component_filenames_index), digest) in filenames_to_resolve
            .into_iter()
            .flat_map(|component_filenames| {
                component_filenames
                    .filenames
                    .into_iter()
                    .map(move |(comp_name_opt, _)| (comp_name_opt, component_filenames.index))
            })
            .zip(digests.into_iter())
        {
            match component_mode {
                ComponentMode::RealComponent(comp) => {
                    results[component_filenames_index].insert(comp, digest);
                }
                ComponentMode::BackwardsCompatibility => {
                    // Simulate component support for old packages by
                    // faking a Run and Build component with the same
                    // contents.
                    results[component_filenames_index].insert(Component::Run, digest);
                    results[component_filenames_index].insert(Component::Build, digest);
                }
            }
        }

        Ok(results)
    }
}

#[async_trait::async_trait]
impl Storage for RuntimeRepository {
    type Recipe = SpecRecipe;
    type Package = Spec;

    async fn get_concrete_package_builds(&self, pkg: &VersionIdent) -> Result<HashSet<BuildIdent>> {
        let mut base = self.root.join(pkg.name());
        base.push(pkg.version().to_string());
        Ok(get_all_filenames(&base)
            .await?
            .into_iter()
            .filter_map(|entry| {
                if entry.ends_with('/') {
                    Some(entry[0..entry.len() - 1].to_string())
                } else {
                    None
                }
            })
            .filter(|entry| base.join(entry).join("spec.yaml").exists())
            .filter_map(|candidate| match parse_build(&candidate) {
                Ok(b) => Some(pkg.to_build(b)),
                Err(err) => {
                    tracing::debug!(
                        "Skipping invalid build in {:?}: [{}] {:?}",
                        self.root,
                        candidate,
                        err
                    );
                    None
                }
            })
            .collect())
    }

    async fn get_embedded_package_builds(
        &self,
        _pkg: &VersionIdent,
    ) -> Result<HashSet<BuildIdent>> {
        // Can't publish packages to a runtime so there can't be any stubs
        Ok(HashSet::default())
    }

    async fn publish_embed_stub_to_storage(&self, _spec: &Self::Package) -> Result<()> {
        Err(Error::String(
            "Cannot publish to a runtime repository".into(),
        ))
    }

    async fn publish_package_to_storage(
        &self,
        _package: &<Self::Recipe as spk_schema::Recipe>::Output,
        _components: &HashMap<Component, spfs::encoding::Digest>,
    ) -> Result<()> {
        Err(Error::String(
            "Cannot publish to a runtime repository".into(),
        ))
    }

    async fn publish_recipe_to_storage(
        &self,
        _spec: &Self::Recipe,
        _publish_policy: PublishPolicy,
    ) -> Result<()> {
        Err(Error::String(
            "Cannot publish to a runtime repository".into(),
        ))
    }

    async fn read_components_from_storage(
        &self,
        pkg: &BuildIdent,
    ) -> Result<HashMap<Component, spfs::encoding::Digest>> {
        let entries = get_all_filenames(self.root.join(pkg.to_string())).await?;
        let components: Vec<Component> = entries
            .into_iter()
            .filter_map(|n| n.strip_suffix(".cmpt").map(str::to_string))
            .map(Component::parse)
            .collect::<spk_schema::foundation::ident_component::Result<_>>()?;

        let mut path = relative_path::RelativePathBuf::from("/spk/pkg");
        path.push(pkg.to_string());

        let mut mapped = HashMap::with_capacity(components.len());
        for name in components.into_iter() {
            let digest = find_layer_by_filename(path.join(format!("{}.cmpt", &name)))
                .await
                .map_err(|err| {
                    if let Error::SPFS(spfs::Error::UnknownReference(_)) = err {
                        Error::SpkValidatorsError(
                            spk_schema::validators::Error::PackageNotFoundError(pkg.to_any()),
                        )
                    } else {
                        err
                    }
                })?;
            mapped.insert(name, digest);
        }

        if mapped.is_empty() {
            // This is package was published before component support
            // was added. It does not have distinct components. So add
            // default Build and Run components that point at the full
            // package digest.
            let digest = find_layer_by_filename(path).await.map_err(|err| {
                if let Error::SPFS(spfs::Error::UnknownReference(_)) = err {
                    Error::SpkValidatorsError(spk_schema::validators::Error::PackageNotFoundError(
                        pkg.to_any(),
                    ))
                } else {
                    err
                }
            })?;
            mapped.insert(Component::Run, digest);
            mapped.insert(Component::Build, digest);
        }
        Ok(mapped)
    }

    async fn read_package_from_storage(
        &self,
        pkg: &BuildIdent,
    ) -> Result<Arc<<Self::Recipe as spk_schema::Recipe>::Output>> {
        let mut path = self.root.join(pkg.to_string());
        path.push("spec.yaml");

        let mut reader = std::fs::File::open(&path).map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                Error::SpkValidatorsError(spk_schema::validators::Error::PackageNotFoundError(
                    pkg.to_any(),
                ))
            } else {
                Error::FileOpenError(path.to_owned(), err)
            }
        })?;
        let mut yaml = String::new();
        reader
            .read_to_string(&mut yaml)
            .map_err(|err| Error::FileReadError(path.to_owned(), err))?;
        <Self::Recipe as spk_schema::Recipe>::Output::from_yaml(yaml)
            .map(Arc::new)
            .map_err(|err| Error::InvalidPackageSpec(pkg.to_any(), err.to_string()))
    }

    async fn remove_embed_stub_from_storage(&self, _pkg: &BuildIdent) -> Result<()> {
        Err(Error::String("Cannot modify a runtime repository".into()))
    }

    async fn remove_package_from_storage(&self, _pkg: &BuildIdent) -> Result<()> {
        Err(Error::String("Cannot modify a runtime repository".into()))
    }
}

#[async_trait::async_trait]
impl Repository for RuntimeRepository {
    fn address(&self) -> &url::Url {
        &self.address
    }

    fn name(&self) -> &RepositoryName {
        &self.name
    }

    async fn list_packages(&self) -> Result<Vec<PkgNameBuf>> {
        Ok(get_all_filenames(&self.root)
            .await?
            .into_iter()
            .filter_map(|entry| {
                if entry.ends_with('/') {
                    Some(entry[0..entry.len() - 1].to_string())
                } else {
                    None
                }
            })
            .filter_map(|e| PkgNameBuf::try_from(e).ok())
            .collect())
    }

    async fn list_package_versions(&self, name: &PkgName) -> Result<Arc<Vec<Arc<Version>>>> {
        Ok(Arc::new(
            get_all_filenames(self.root.join(name))
                .await?
                .into_iter()
                .filter_map(|entry| {
                    if entry.ends_with('/') {
                        Some(entry[0..entry.len() - 1].to_string())
                    } else {
                        None
                    }
                })
                .filter_map(|candidate| match parse_version(&candidate) {
                    Ok(v) => Some(Arc::new(v)),
                    Err(err) => {
                        tracing::debug!(
                            "Skipping invalid version in /spfs/spk: [{}], {:?}",
                            candidate,
                            err
                        );
                        None
                    }
                })
                .collect(),
        ))
    }

    async fn list_build_components(&self, pkg: &BuildIdent) -> Result<Vec<Component>> {
        let entries = get_all_filenames(self.root.join(pkg.to_string())).await?;
        entries
            .into_iter()
            .filter_map(|n| n.strip_suffix(".cmpt").map(str::to_string))
            .map(|c| Component::parse(c).map_err(|err| err.into()))
            .collect()
    }

    async fn read_embed_stub(&self, pkg: &BuildIdent) -> Result<Arc<Self::Package>> {
        Err(Error::SpkValidatorsError(
            spk_schema::validators::Error::PackageNotFoundError(pkg.to_any()),
        ))
    }

    async fn read_recipe(&self, pkg: &VersionIdent) -> Result<Arc<Self::Recipe>> {
        Err(Error::SpkValidatorsError(
            spk_schema::validators::Error::PackageNotFoundError(pkg.to_any(None)),
        ))
    }

    async fn remove_recipe(&self, _pkg: &VersionIdent) -> Result<()> {
        Err(Error::String("Cannot modify a runtime repository".into()))
    }
}

/// Works like ls_tags, returning strings that end with '/' for directories
/// and not for regular files
async fn get_all_filenames<P: AsRef<std::path::Path>>(path: P) -> Result<Vec<String>> {
    let mut entries = match tokio::fs::read_dir(&path).await {
        Err(err) => {
            return match err.kind() {
                std::io::ErrorKind::NotFound => Ok(Default::default()),
                _ => Err(Error::FileOpenError(path.as_ref().to_owned(), err)),
            }
        }
        Ok(e) => e,
    };
    let mut results = Vec::new();
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|err| Error::FileOpenError(path.as_ref().to_owned(), err))?
    {
        let mut name = entry.file_name().to_string_lossy().to_string();
        match entry.file_type().await {
            Ok(t) if t.is_dir() => name.push('/'),
            _ => (),
        }
        results.push(name)
    }
    Ok(results)
}

async fn find_layer_by_filename<S: AsRef<str>>(path: S) -> Result<spfs::encoding::Digest> {
    let runtime = spfs::active_runtime().await?;
    let repo = spfs::get_runtime_backing_repo(&runtime).await?;

    let layers = spfs::resolve_stack_to_layers(runtime.status.stack.iter(), Some(&repo)).await?;
    for layer in layers.iter().rev() {
        let manifest = repo
            .read_manifest(layer.manifest)
            .await?
            .to_tracking_manifest();
        if manifest.get_path(&path).is_some() {
            return Ok(layer.digest()?);
        }
    }
    Err(spfs::Error::UnknownReference(path.as_ref().into()).into())
}

/// Perform a reverse lookup in the current runtime to find which layer, if
/// any, a path belongs to.
///
/// It is more efficient to perform this lookup on multiple paths
/// simultaneously than to do one path at a time.
///
/// The digests are returned in the same order as the elements in `paths`.
async fn find_layers_by_filenames<S: AsRef<str>>(
    paths: &[S],
) -> Result<Vec<spfs::encoding::Digest>> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }

    let runtime = spfs::active_runtime().await?;
    let repo = spfs::get_runtime_backing_repo(&runtime).await?;

    let mut paths = paths.iter().map(Some).enumerate().collect::<Vec<_>>();

    let mut results = Vec::new();
    results.resize_with(paths.len(), Default::default);

    let layers = spfs::resolve_stack_to_layers(runtime.status.stack.iter(), Some(&repo)).await?;
    for layer in layers.iter().rev() {
        let manifest = repo
            .read_manifest(layer.manifest)
            .await?
            .to_tracking_manifest();

        let mut paths_remaining = false;
        for (index, path_opt) in paths.iter_mut() {
            match path_opt {
                Some(path) => {
                    if manifest.get_path(path).is_some() {
                        results[*index] = layer.digest()?;
                        *path_opt = None;
                    } else {
                        paths_remaining = true;
                    }
                }
                None => {}
            }
        }

        if !paths_remaining {
            return Ok(results);
        }
    }

    // Some path(s) were not resolved.
    for (_, path_opt) in paths {
        if let Some(path) = path_opt {
            return Err(spfs::Error::UnknownReference(path.as_ref().into()).into());
        }
    }

    Err(Error::String(
        "Internal bug; not all paths resolved but unknown which".to_owned(),
    ))
}

/// Return a list of spfs object lists that lead to the given
/// filepath in the runtime repo.
pub async fn find_path_providers(filepath: &str) -> Result<(bool, Vec<Vec<ObjectPathEntry>>)> {
    let config = spfs::get_config()?;
    let repo = config.get_local_repository_handle().await?;

    spfs::find_path::find_path_providers_in_spfs_runtime(filepath, &repo)
        .await
        .map_err(|err| err.into())
}

/// Print out a spfs object list down to the given filepath
pub async fn pretty_print_filepath(
    filepath: &str,
    objectpath: &Vec<ObjectPathEntry>,
) -> Result<()> {
    let config = spfs::get_config()?;
    let repo = config.get_local_repository_handle().await?;

    let digest_format = DigestFormat::Shortened(&repo);
    match spfs::io::pretty_print_filepath(filepath, objectpath, digest_format).await {
        Ok(r) => Ok(r),
        Err(err) => Err(err.into()),
    }
}
