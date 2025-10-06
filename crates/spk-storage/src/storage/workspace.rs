// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use itertools::Itertools;
use spk_schema::foundation::ident_component::Component;
use spk_schema::foundation::name::{PkgName, PkgNameBuf, RepositoryName, RepositoryNameBuf};
use spk_schema::foundation::version::Version;
use spk_schema::ident::ToAnyIdentWithoutBuild;
use spk_schema::ident_build::Build;
use spk_schema::template::DiscoverVersions;
use spk_schema::{BuildIdent, Recipe, Spec, SpecRecipe, Template, VersionIdent};

use super::Repository;
use super::repository::{PublishPolicy, Storage};
use crate::{Error, Result};

#[cfg(test)]
#[path = "./workspace_test.rs"]
mod workspace_test;

/// A repository that represents package build for
/// and from an [`spk_workspace::Workspace`].
#[derive(Clone, Debug)]
pub struct WorkspaceRepository {
    address: url::Url,
    name: RepositoryNameBuf,
    inner: spk_workspace::Workspace,
}

impl std::hash::Hash for WorkspaceRepository {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.address.hash(state);
    }
}

impl Eq for WorkspaceRepository {}
impl PartialEq for WorkspaceRepository {
    fn eq(&self, other: &Self) -> bool {
        self.address == other.address
    }
}

impl Ord for WorkspaceRepository {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.address.cmp(&other.address)
    }
}

impl PartialOrd for WorkspaceRepository {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl WorkspaceRepository {
    /// Build a workspace repository from its parts.
    pub fn new(
        root: &std::path::Path,
        name: RepositoryNameBuf,
        workspace: spk_workspace::Workspace,
    ) -> Self {
        let address = Self::address_from_root(root);
        Self {
            address,
            name,
            inner: workspace,
        }
    }

    /// Open a workspace repository from its root directory, using the default name.
    ///
    /// Panics if the workspace cannot be loaded at the given path.
    #[cfg(test)]
    pub fn open(root: &std::path::Path) -> Result<Self> {
        // this function is not allowed outside of testing because it will
        // panic if the workspace is invalid
        let address = Self::address_from_root(root);
        Ok(Self {
            address,
            name: "workspace".try_into()?,
            inner: spk_workspace::Workspace::builder()
                .load_from_dir(root)?
                .build()?,
        })
    }

    fn address_from_root(root: &std::path::Path) -> url::Url {
        let address = format!("workspace://{}", root.display());
        match url::Url::parse(&address) {
            Ok(a) => a,
            Err(err) => {
                tracing::error!(
                    "failed to create valid address for path {:?}: {:?}",
                    root,
                    err
                );
                url::Url::parse(&format!("workspace://{}", root.to_string_lossy()))
                    .expect("Failed to create url from path (fallback)")
            }
        }
    }
}

impl std::ops::Deref for WorkspaceRepository {
    type Target = spk_workspace::Workspace;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

#[async_trait::async_trait]
impl Storage for WorkspaceRepository {
    type Recipe = SpecRecipe;
    type Package = Spec;

    async fn get_concrete_package_builds(&self, pkg: &VersionIdent) -> Result<HashSet<BuildIdent>> {
        // assuming that this version was previously loaded via list_versions,
        // we can present that a source build is available.
        // TODO: it's not clear if this assumption will turn out dangerous,
        // but we generally assume that the solver won't try to look for build
        // of a package version that it doesn't know exists...
        let mut builds = HashSet::new();
        builds.insert(pkg.to_build_ident(Build::Source));
        Ok(builds)
    }

    async fn get_embedded_package_builds(
        &self,
        _pkg: &VersionIdent,
    ) -> Result<HashSet<BuildIdent>> {
        // Can't publish packages to a workspace so there can't be any stubs
        Ok(HashSet::default())
    }

    async fn publish_embed_stub_to_storage(&self, _spec: &Self::Package) -> Result<()> {
        Err(Error::String(
            "Cannot publish to a workspace repository".into(),
        ))
    }

    async fn publish_package_to_storage(
        &self,
        _package: &<Self::Recipe as spk_schema::Recipe>::Output,
        _components: &HashMap<Component, spfs::encoding::Digest>,
    ) -> Result<()> {
        Err(Error::String(
            "Cannot publish to a workspace repository".into(),
        ))
    }

    async fn publish_recipe_to_storage(
        &self,
        _spec: &Self::Recipe,
        _publish_policy: PublishPolicy,
    ) -> Result<()> {
        Err(Error::String(
            "Cannot publish to a workspace repository".into(),
        ))
    }

    async fn read_components_from_storage(
        &self,
        _pkg: &BuildIdent,
    ) -> Result<HashMap<Component, spfs::encoding::Digest>> {
        Ok(HashMap::new())
    }

    async fn read_package_from_storage(
        &self,
        pkg: &BuildIdent,
    ) -> Result<Arc<<Self::Recipe as spk_schema::Recipe>::Output>> {
        if !pkg.is_source() {
            return Err(Error::PackageNotFound(Box::new(
                pkg.clone().into_any_ident(),
            )));
        }

        let mut candidates = Vec::new();
        for (name, tpl) in self.inner.iter() {
            if name != pkg.name() {
                continue;
            }
            let versions = tpl.discover_versions()?;
            if versions.contains(pkg.version()) {
                candidates.push(tpl);
            }
        }
        if candidates.is_empty() {
            return Err(Error::PackageNotFound(Box::new(pkg.to_any_ident())));
        }
        if candidates.len() > 1 {
            tracing::warn!(
                "multiple viable recipes found in workspace for {pkg} [{}]",
                candidates
                    .iter()
                    .map(|r| r.file_path().to_string_lossy())
                    .join(", ")
            );
        }
        let rendered = candidates[0].render(spk_schema::template::TemplateRenderConfig {
            version: Some(pkg.version().to_owned()),
            ..Default::default()
        })?;
        let recipe = rendered.into_recipe().map_err(|err| {
            Error::String(format!(
                "Failed to convert rendered template into recipe: {err}"
            ))
        })?;
        let build = recipe.generate_source_build(
            candidates[0]
                .file_path()
                .parent()
                .or(self.inner.root())
                .unwrap_or_else(|| std::path::Path::new(".")),
        )?;
        Ok(Arc::new(build))
    }

    async fn remove_embed_stub_from_storage(&self, _pkg: &BuildIdent) -> Result<()> {
        Err(Error::String("Cannot modify a workspace repository".into()))
    }

    async fn remove_package_from_storage(&self, _pkg: &BuildIdent) -> Result<()> {
        Err(Error::String("Cannot modify a workspace repository".into()))
    }
}

#[async_trait::async_trait]
impl Repository for WorkspaceRepository {
    fn address(&self) -> &url::Url {
        &self.address
    }

    fn name(&self) -> &RepositoryName {
        &self.name
    }

    async fn list_packages(&self) -> Result<Vec<PkgNameBuf>> {
        let unique = self
            .inner
            .iter()
            .map(|(name, _)| name.to_owned())
            .collect::<BTreeSet<_>>();
        Ok(unique.into_iter().collect())
    }

    async fn list_package_versions(&self, name: &PkgName) -> Result<Arc<Vec<Arc<Version>>>> {
        let mut versions = HashSet::new();
        for (tpl_name, tpl) in self.inner.iter() {
            if tpl_name != name {
                continue;
            }
            let discovered = tpl.discover_versions()?;
            versions.extend(discovered.into_iter().map(Arc::new));
        }
        let mut sorted = versions.into_iter().collect::<Vec<_>>();
        sorted.sort();
        Ok(Arc::new(sorted))
    }

    async fn list_build_components(&self, pkg: &BuildIdent) -> Result<Vec<Component>> {
        if pkg.is_source() {
            return Ok(vec![Component::Source]);
        }
        Err(Error::PackageNotFound(Box::new(pkg.to_any_ident())))
    }

    async fn read_embed_stub(&self, pkg: &BuildIdent) -> Result<Arc<Self::Package>> {
        Err(Error::PackageNotFound(Box::new(pkg.to_any_ident())))
    }

    async fn read_recipe(&self, pkg: &VersionIdent) -> Result<Arc<Self::Recipe>> {
        let mut candidates = Vec::new();

        for (name, tpl) in self.inner.iter() {
            if name != pkg.name() {
                continue;
            }
            let versions = tpl.discover_versions()?;
            if versions.contains(pkg.version()) {
                candidates.push(tpl);
            }
        }
        if candidates.is_empty() {
            return Err(Error::PackageNotFound(Box::new(
                pkg.to_any_ident_without_build(),
            )));
        }
        if candidates.len() > 1 {
            tracing::warn!(
                "multiple viable recipes found in workspace for {pkg} [{}]",
                candidates
                    .iter()
                    .map(|r| r.file_path().to_string_lossy())
                    .join(", ")
            );
        }
        let rendered = candidates[0].render(spk_schema::template::TemplateRenderConfig {
            version: Some(pkg.version().to_owned()),
            ..Default::default()
        })?;
        rendered.into_recipe().map_err(|err| {
            Error::String(format!(
                "Failed to convert rendered template into recipe: {err}"
            ))
        })
    }

    async fn remove_recipe(&self, _pkg: &VersionIdent) -> Result<()> {
        Err(Error::String("Cannot modify a workspace repository".into()))
    }
}
