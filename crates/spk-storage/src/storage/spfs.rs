// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::collections::{hash_map, HashMap, HashSet};
use std::convert::{TryFrom, TryInto};
use std::str::FromStr;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::Arc;

use dashmap::DashMap;
use futures::StreamExt;
use itertools::Itertools;
use once_cell::sync::Lazy;
use relative_path::RelativePathBuf;
use serde_derive::{Deserialize, Serialize};
use spfs::storage::EntryType;
use spfs::tracking;
use spk_schema::foundation::ident_build::{parse_build, Build, InvalidBuildError};
use spk_schema::foundation::ident_component::Component;
use spk_schema::foundation::name::{PkgName, PkgNameBuf, RepositoryName, RepositoryNameBuf};
use spk_schema::foundation::version::{parse_version, Version};
use spk_schema::ident_build::parsing::embedded_source_package;
use spk_schema::ident_build::EmbeddedSource;
use spk_schema::ident_ops::TagPath;
use spk_schema::{FromYaml, Ident, Package, Recipe, Spec, SpecRecipe};
use tokio::io::AsyncReadExt;

use super::repository::{PublishPolicy, Storage};
use super::{CachePolicy, Repository};
use crate::storage::repository::internal::RepositoryExt;
use crate::{with_cache_policy, Error, Result};

#[cfg(test)]
#[path = "./spfs_test.rs"]
mod spfs_test;

const REPO_METADATA_TAG: &str = "spk/repo";
const REPO_VERSION: &str = "1.0.0";

#[derive(Debug)]
pub struct SPFSRepository {
    address: url::Url,
    name: RepositoryNameBuf,
    inner: spfs::storage::RepositoryHandle,
    cache_policy: AtomicPtr<CachePolicy>,
    caches: CachesForAddress,
}

impl std::hash::Hash for SPFSRepository {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.address.hash(state);
    }
}

impl Ord for SPFSRepository {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.address.cmp(&other.address)
    }
}

impl PartialOrd for SPFSRepository {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for SPFSRepository {
    fn eq(&self, other: &Self) -> bool {
        self.address == other.address
    }
}

impl Eq for SPFSRepository {}

impl std::ops::Deref for SPFSRepository {
    type Target = spfs::storage::RepositoryHandle;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl std::ops::DerefMut for SPFSRepository {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<S: AsRef<str>, T: Into<spfs::storage::RepositoryHandle>> TryFrom<(S, T)> for SPFSRepository {
    type Error = crate::Error;

    fn try_from(name_and_repo: (S, T)) -> Result<Self> {
        let inner = name_and_repo.1.into();
        let address = inner.address();
        Ok(Self {
            caches: CachesForAddress::new(&address),
            address,
            name: name_and_repo.0.as_ref().try_into()?,
            inner,
            cache_policy: AtomicPtr::new(Box::leak(Box::new(CachePolicy::CacheOk))),
        })
    }
}

impl SPFSRepository {
    pub async fn new(name: &str, address: &str) -> Result<Self> {
        let inner = spfs::open_repository(address).await?;
        let address = inner.address();
        Ok(Self {
            caches: CachesForAddress::new(&address),
            address,
            name: name.try_into()?,
            inner,
            cache_policy: AtomicPtr::new(Box::leak(Box::new(CachePolicy::CacheOk))),
        })
    }
}

impl std::ops::Drop for SPFSRepository {
    fn drop(&mut self) {
        // Safety: We only put valid `Box` pointers into `self.cache_policy`.
        unsafe {
            let _ = Box::from_raw(self.cache_policy.load(Ordering::Relaxed));
        }
    }
}

#[derive(Clone)]
enum CacheValue<T> {
    InvalidPackageSpec(Ident, String),
    PackageNotFoundError(Ident),
    StringError(String),
    StringifiedError(String),
    Success(T),
}

impl<T> From<CacheValue<T>> for Result<T> {
    fn from(cv: CacheValue<T>) -> Self {
        match cv {
            CacheValue::InvalidPackageSpec(i, err) => Err(crate::Error::InvalidPackageSpec(i, err)),
            CacheValue::PackageNotFoundError(i) => Err(crate::Error::SpkValidatorsError(
                spk_schema::validators::Error::PackageNotFoundError(i),
            )),
            CacheValue::StringError(s) => Err(s.into()),
            CacheValue::StringifiedError(s) => Err(s.into()),
            CacheValue::Success(v) => Ok(v),
        }
    }
}

impl<T> From<std::result::Result<T, &crate::Error>> for CacheValue<T> {
    fn from(r: std::result::Result<T, &crate::Error>) -> Self {
        match r {
            Ok(v) => CacheValue::Success(v),
            Err(crate::Error::InvalidPackageSpec(i, err)) => {
                CacheValue::InvalidPackageSpec(i.clone(), err.to_string())
            }
            Err(crate::Error::SpkValidatorsError(
                spk_schema::validators::Error::PackageNotFoundError(i),
            )) => CacheValue::PackageNotFoundError(i.clone()),
            Err(crate::Error::String(s)) => CacheValue::StringError(s.clone()),
            // Decorate the error message so we can tell it was a custom error
            // downgraded to a String.
            Err(err) => CacheValue::StringifiedError(format!("Cached error: {}", err)),
        }
    }
}

// To keep clippy happy
type ArcVecArcVersion = Arc<Vec<Arc<Version>>>;
/// The set of caches for a specific repository.
#[derive(Clone)]
struct CachesForAddress {
    /// Components list cache for list_build_components()
    list_build_components: Arc<DashMap<Ident, CacheValue<Vec<Component>>>>,
    /// EntryTypes list cache for ls_tags() caches
    ls_tags: Arc<DashMap<relative_path::RelativePathBuf, Vec<EntryType>>>,
    /// Package specs cache for read_component_from_storage() and read_embed_stub()
    package: Arc<DashMap<Ident, CacheValue<Arc<Spec>>>>,
    /// Versions list cache for list_packages_versions()
    package_versions: Arc<DashMap<PkgNameBuf, CacheValue<ArcVecArcVersion>>>,
    /// Recipe specs cache for read_recipe()
    recipe: Arc<DashMap<Ident, CacheValue<Arc<spk_schema::SpecRecipe>>>>,
    /// Recipe specs cache for read_recipe()
    tag_spec: Arc<DashMap<tracking::TagSpec, CacheValue<tracking::Tag>>>,
}

static CACHES_FOR_ADDRESS: Lazy<std::sync::Mutex<HashMap<String, CachesForAddress>>> =
    Lazy::new(|| std::sync::Mutex::new(HashMap::new()));

impl CachesForAddress {
    fn new(address: &url::Url) -> Self {
        let mut caches = CACHES_FOR_ADDRESS.lock().unwrap();
        match caches.entry(address.as_str().to_owned()) {
            hash_map::Entry::Occupied(entry) => entry.get().clone(),
            hash_map::Entry::Vacant(entry) => entry
                .insert(Self {
                    list_build_components: Arc::new(DashMap::new()),
                    ls_tags: Arc::new(DashMap::new()),
                    package: Arc::new(DashMap::new()),
                    package_versions: Arc::new(DashMap::new()),
                    recipe: Arc::new(DashMap::new()),
                    tag_spec: Arc::new(DashMap::new()),
                })
                .clone(),
        }
    }
}

impl std::fmt::Debug for CachesForAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CachesForAddress").finish()
    }
}

#[async_trait::async_trait]
impl Storage for SPFSRepository {
    type Recipe = SpecRecipe;
    type Package = Spec;

    async fn get_concrete_package_builds(&self, pkg: &Ident) -> Result<HashSet<Ident>> {
        let pkg = pkg.with_build(Some(Build::Source));
        let mut base = self.build_package_tag(&pkg)?;
        // the package tag contains the name and build, but we need to
        // remove the trailing build in order to list the containing 'folder'
        // eg: pkg/1.0.0/src => pkg/1.0.0
        base.pop();

        let builds: HashSet<_> = self
            .ls_tags(&base)
            .await
            .into_iter()
            .filter_map(|entry| match entry {
                Ok(EntryType::Tag(name)) => Some(name),
                Ok(EntryType::Folder(name)) => Some(name),
                Err(_) => None,
            })
            .filter_map(|b| match parse_build(&b) {
                Ok(v) => Some(v),
                Err(_) => {
                    tracing::warn!("Invalid build found in spfs tags: {}", b);
                    None
                }
            })
            .map(|b| pkg.with_build(Some(b)))
            .collect();

        Ok(builds)
    }

    async fn get_embedded_package_builds(&self, pkg: &Ident) -> Result<HashSet<Ident>> {
        let pkg = pkg.with_build(Some(Build::Source));
        let mut base = self.build_spec_tag(&pkg);
        // the package tag contains the name and build, but we need to
        // remove the trailing build in order to list the containing 'folder'
        // eg: pkg/1.0.0/src => pkg/1.0.0
        base.pop();

        let builds: HashSet<_> = self
            .ls_tags(&base)
            .await
            .into_iter()
            .filter_map(|entry| match entry {
                Ok(EntryType::Tag(name)) => Some(name),
                Ok(EntryType::Folder(_)) => None,
                Err(_) => None,
            })
            .filter_map(|b| {
                b.strip_prefix("embedded-by-")
                    .and_then(|encoded_ident| {
                        data_encoding::BASE32_NOPAD
                            .decode(encoded_ident.as_bytes())
                            .ok()
                    })
                    .and_then(|bytes| String::from_utf8(bytes).ok())
                    .and_then(|ident_str| {
                        // The decoded BASE32 value will look something like this:
                        //
                        //     "embedded[embed-projection:run/1.0/3I42H3S6]"
                        //
                        // The `embedded_source_package` parser knows how to
                        // parse the "[...]" part and return the type we want,
                        // but we need to strip the "embedded" prefix.
                        ident_str
                            .strip_prefix("embedded")
                            .and_then(|ident_str| {
                                use nom::combinator::all_consuming;

                                all_consuming(
                                        embedded_source_package::<(_, nom::error::ErrorKind)>,
                                    )(ident_str)
                                    .map(|(_, ident_with_components)| ident_with_components)
                                    .ok()
                            })
                            .map(Build::Embedded)
                    })
            })
            .map(|b| pkg.with_build(Some(b)))
            .collect();

        Ok(builds)
    }

    async fn publish_embed_stub_to_storage(&self, spec: &Self::Package) -> Result<()> {
        let ident = spec.ident();
        let tag_path = self.build_spec_tag(ident);
        let tag_spec = spfs::tracking::TagSpec::parse(tag_path.as_str())?;

        let payload = serde_yaml::to_vec(&spec)
            .map_err(|err| Error::SpkSpecError(spk_schema::Error::SpecEncodingError(err)))?;
        let digest = self
            .inner
            .commit_blob(Box::pin(std::io::Cursor::new(payload)))
            .await?;
        self.inner.push_tag(&tag_spec, &digest).await?;
        self.invalidate_caches();
        Ok(())
    }

    async fn publish_package_to_storage(
        &self,
        package: &<Self::Recipe as spk_schema::Recipe>::Output,
        components: &HashMap<Component, spfs::encoding::Digest>,
    ) -> Result<()> {
        let tag_path = self.build_package_tag(package.ident())?;

        // We will also publish the 'run' component in the old style
        // for compatibility with older versions of the spk command.
        // It's not perfect but at least the package will be visible
        let legacy_tag = spfs::tracking::TagSpec::parse(&tag_path)?;
        let legacy_component = if let Some(Build::Source) = package.ident().build {
            *components.get(&Component::Source).ok_or_else(|| {
                Error::String("Package must have a source component to be published".to_string())
            })?
        } else {
            *components.get(&Component::Run).ok_or_else(|| {
                Error::String("Package must have a run component to be published".to_string())
            })?
        };

        self.inner.push_tag(&legacy_tag, &legacy_component).await?;

        let components: std::result::Result<Vec<_>, _> = components
            .iter()
            .map(|(name, digest)| {
                spfs::tracking::TagSpec::parse(tag_path.join(name.as_str()))
                    .map(|spec| (spec, digest))
            })
            .collect();
        for (tag_spec, digest) in components?.into_iter() {
            self.inner.push_tag(&tag_spec, digest).await?;
        }

        // TODO: dedupe this part with force_publish_recipe
        let tag_path = self.build_spec_tag(package.ident());
        let tag_spec = spfs::tracking::TagSpec::parse(tag_path)?;
        let payload = serde_yaml::to_vec(&package)
            .map_err(|err| Error::SpkSpecError(spk_schema::Error::SpecEncodingError(err)))?;
        let digest = self
            .inner
            .commit_blob(Box::pin(std::io::Cursor::new(payload)))
            .await?;
        self.inner.push_tag(&tag_spec, &digest).await?;
        self.invalidate_caches();
        Ok(())
    }

    async fn publish_recipe_to_storage(
        &self,
        spec: &Self::Recipe,
        publish_policy: PublishPolicy,
    ) -> Result<()> {
        let ident = spec.to_ident();
        let tag_path = self.build_spec_tag(&ident);
        let tag_spec = spfs::tracking::TagSpec::parse(tag_path.as_str())?;
        if matches!(publish_policy, PublishPolicy::DoNotOverwriteVersion)
            && self.inner.has_tag(&tag_spec).await
        {
            // BUG(rbottriell): this creates a race condition but is not super dangerous
            // because of the non-destructive tag history
            return Err(Error::SpkValidatorsError(
                spk_schema::validators::Error::VersionExistsError(ident),
            ));
        }

        let payload = serde_yaml::to_vec(&spec)
            .map_err(|err| Error::SpkSpecError(spk_schema::Error::SpecEncodingError(err)))?;
        let digest = self
            .inner
            .commit_blob(Box::pin(std::io::Cursor::new(payload)))
            .await?;
        self.inner.push_tag(&tag_spec, &digest).await?;
        self.invalidate_caches();
        Ok(())
    }

    async fn read_components_from_storage(
        &self,
        pkg: &Ident,
    ) -> Result<HashMap<Component, spfs::encoding::Digest>> {
        if matches!(pkg.build, Some(Build::Embedded(_))) {
            return Ok(HashMap::new());
        }
        let package = self.lookup_package(pkg).await?;
        let component_tags = package.into_components();
        let mut components = HashMap::with_capacity(component_tags.len());
        for (name, tag_spec) in component_tags.into_iter() {
            let tag = self.resolve_tag(pkg, &tag_spec).await?;
            components.insert(name, tag.target);
        }
        Ok(components)
    }

    async fn read_package_from_storage(
        &self,
        pkg: &Ident,
    ) -> Result<Arc<<Self::Recipe as spk_schema::Recipe>::Output>> {
        // TODO: reduce duplicate code with read_recipe
        if self.cached_result_permitted() {
            if let Some(v) = self.caches.package.get(pkg) {
                return v.value().clone().into();
            }
        }

        let r: Result<Arc<Spec>> = async {
            let tag_path = self.build_spec_tag(pkg);
            let tag_spec = spfs::tracking::TagSpec::parse(tag_path.as_str())?;
            let tag = self.resolve_tag(pkg, &tag_spec).await?;

            let (mut reader, filename) = self.inner.open_payload(tag.target).await?;
            let mut yaml = String::new();
            reader
                .read_to_string(&mut yaml)
                .await
                .map_err(|err| Error::FileReadError(filename, err))?;
            Spec::from_yaml(&yaml)
                .map_err(|err| Error::InvalidPackageSpec(pkg.clone(), err.to_string()))
                .map(Arc::new)
        }
        .await;

        self.caches
            .package
            .insert(pkg.clone(), r.as_ref().map(Arc::clone).into());
        r
    }

    async fn remove_embed_stub_from_storage(&self, pkg: &Ident) -> Result<()> {
        // Same as removing a recipe for now...
        self.remove_recipe(pkg).await
    }

    async fn remove_package_from_storage(&self, pkg: &Ident) -> Result<()> {
        for tag_spec in
            with_cache_policy!(self, CachePolicy::BypassCache, { self.lookup_package(pkg) })
                .await?
                .tags()
        {
            match self.inner.remove_tag_stream(tag_spec).await {
                Err(spfs::Error::UnknownReference(_)) => (),
                res => res?,
            }
        }
        // because we double-publish packages to be visible/compatible
        // with the old repo tag structure, we must also try to remove
        // the legacy version of the tag after removing the discovered
        // as it may still be there and cause the removal to be ineffective
        if let Ok(legacy_tag) = spfs::tracking::TagSpec::parse(self.build_package_tag(pkg)?) {
            match self.inner.remove_tag_stream(&legacy_tag).await {
                Err(spfs::Error::UnknownReference(_)) => (),
                res => res?,
            }
        }
        self.invalidate_caches();
        Ok(())
    }
}

#[async_trait::async_trait]
impl Repository for SPFSRepository {
    fn address(&self) -> &url::Url {
        &self.address
    }

    async fn list_packages(&self) -> Result<Vec<PkgNameBuf>> {
        let path = relative_path::RelativePath::new("spk/spec");
        // XXX: infallible vs return type
        Ok(self
            .ls_tags(path)
            .await
            .into_iter()
            .filter_map(|entry| match entry {
                Ok(EntryType::Folder(name)) => name.parse().ok(),
                Ok(EntryType::Tag(_)) => None,
                Err(_) => None,
            })
            .collect::<Vec<_>>())
    }

    async fn list_package_versions(&self, name: &PkgName) -> Result<Arc<Vec<Arc<Version>>>> {
        if self.cached_result_permitted() {
            if let Some(v) = self.caches.package_versions.get(name) {
                return v.value().clone().into();
            }
        }
        let r: Result<Arc<_>> = async {
            let path = self.build_spec_tag(&name.to_owned().into());
            let versions: HashSet<_> = self
                .ls_tags(&path)
                .await
                .into_iter()
                .filter_map(|entry| match entry {
                    // undo our encoding of the invalid '+' character in spfs tags
                    Ok(EntryType::Folder(name)) => Some(name.replace("..", "+")),
                    Ok(EntryType::Tag(name)) => Some(name.replace("..", "+")),
                    Err(_) => None,
                })
                .filter_map(|v| match parse_version(&v) {
                    Ok(v) => Some(v),
                    Err(_) => {
                        tracing::warn!("Invalid version found in spfs tags: {}", v);
                        None
                    }
                })
                .collect();
            let mut versions = versions.into_iter().map(Arc::new).collect_vec();
            versions.sort();
            // XXX: infallible vs return type
            Ok(Arc::new(versions))
        }
        .await;

        self.caches
            .package_versions
            .insert(name.to_owned(), r.as_ref().map(|b| b.clone()).into());
        r
    }

    async fn list_build_components(&self, pkg: &Ident) -> Result<Vec<Component>> {
        if self.cached_result_permitted() {
            if let Some(v) = self.caches.list_build_components.get(pkg) {
                return v.value().clone().into();
            }
        }

        let r = if matches!(pkg.build, Some(Build::Embedded(_))) {
            Ok(Vec::new())
        } else {
            match self.lookup_package(pkg).await {
                Ok(p) => Ok(p.into_components().into_keys().collect()),
                Err(Error::SpkValidatorsError(
                    spk_schema::validators::Error::PackageNotFoundError(_),
                )) => Ok(Vec::new()),
                Err(err) => Err(err),
            }
        };

        self.caches
            .list_build_components
            .insert(pkg.to_owned(), r.as_ref().map(|v| v.clone()).into());
        r
    }

    fn name(&self) -> &RepositoryName {
        &self.name
    }

    async fn read_embed_stub(&self, pkg: &Ident) -> Result<Arc<Self::Package>> {
        // This is similar to read_recipe but it returns a package and
        // uses the package cache.
        match pkg.build {
            Some(Build::Embedded(EmbeddedSource::Package { .. })) => {
                // Allow embedded stubs to be read as a "package"
            }
            _ => {
                return Err(format!("Cannot read this ident as an embed stub: {pkg}").into());
            }
        };
        if self.cached_result_permitted() {
            if let Some(v) = self.caches.package.get(pkg) {
                return v.value().clone().into();
            }
        }
        let r: Result<Arc<Spec>> = async {
            let tag_path = self.build_spec_tag(pkg);
            let tag_spec = spfs::tracking::TagSpec::parse(tag_path.as_str())?;
            let tag = self.resolve_tag(pkg, &tag_spec).await?;

            let (mut reader, _) = self.inner.open_payload(tag.target).await?;
            let mut yaml = String::new();
            reader
                .read_to_string(&mut yaml)
                .await
                .map_err(|err| Error::FileReadError(tag.target.to_string().into(), err))?;
            Spec::from_yaml(yaml)
                .map_err(|err| Error::InvalidPackageSpec(pkg.clone(), err.to_string()))
                .map(Arc::new)
        }
        .await;

        self.caches
            .package
            .insert(pkg.clone(), r.as_ref().map(Arc::clone).into());
        r
    }

    async fn read_recipe(&self, pkg: &Ident) -> Result<Arc<Self::Recipe>> {
        if pkg.build.is_some() {
            return Err(format!("cannot read a recipe for a package build: {pkg}").into());
        };
        if self.cached_result_permitted() {
            if let Some(v) = self.caches.recipe.get(pkg) {
                return v.value().clone().into();
            }
        }
        let r: Result<Arc<SpecRecipe>> = async {
            let tag_path = self.build_spec_tag(pkg);
            let tag_spec = spfs::tracking::TagSpec::parse(tag_path.as_str())?;
            let tag = self.resolve_tag(pkg, &tag_spec).await?;

            let (mut reader, _) = self.inner.open_payload(tag.target).await?;
            let mut yaml = String::new();
            reader
                .read_to_string(&mut yaml)
                .await
                .map_err(|err| Error::FileReadError(tag.target.to_string().into(), err))?;
            SpecRecipe::from_yaml(yaml)
                .map_err(|err| Error::InvalidPackageSpec(pkg.clone(), err.to_string()))
                .map(Arc::new)
        }
        .await;

        self.caches
            .recipe
            .insert(pkg.clone(), r.as_ref().map(Arc::clone).into());
        r
    }

    async fn remove_recipe(&self, pkg: &Ident) -> Result<()> {
        let tag_path = self.build_spec_tag(pkg);
        let tag_spec = spfs::tracking::TagSpec::parse(&tag_path)?;
        match self.inner.remove_tag_stream(&tag_spec).await {
            Err(spfs::Error::UnknownReference(_)) => Err(Error::SpkValidatorsError(
                spk_schema::validators::Error::PackageNotFoundError(pkg.clone()),
            )),
            Err(err) => Err(err.into()),
            Ok(_) => {
                self.invalidate_caches();
                Ok(())
            }
        }
    }

    async fn upgrade(&self) -> Result<String> {
        let target_version = Version::from_str(REPO_VERSION).unwrap();
        let mut meta = self.read_metadata().await?;
        if meta.version > target_version {
            // for this particular upgrade (moving old-style tags to new)
            // we allow it to be run again over the same repo since it's
            // possible that some clients are still publishing the old way
            // during the transition period
            return Ok("Nothing to do.".to_string());
        }
        for name in self.list_packages().await? {
            tracing::info!("Processing {name}...");
            let mut pkg = Ident::new(name.to_owned());
            for version in self.list_package_versions(&name).await?.iter() {
                pkg.version = (**version).clone();
                for build in self.list_package_builds(&pkg).await? {
                    if build.is_embedded() {
                        // XXX `lookup_package` isn't able to read embed stubs.
                        // Should it be able to?
                        continue;
                    }
                    let stored = with_cache_policy!(self, CachePolicy::BypassCache, {
                        self.lookup_package(&build)
                    })
                    .await?;

                    // [Re-]create embedded stubs.
                    if build.can_embed() {
                        let spec = self.read_package(&build).await?;
                        let providers = self.get_embedded_providers(&spec)?;
                        if !providers.is_empty() {
                            tracing::info!("Creating embedded stubs for {name}...");
                            for (embedded, components) in providers.into_iter() {
                                self.create_embedded_stub_for_spec(&spec, &embedded, components)
                                    .await?
                            }
                        }
                    }

                    if stored.has_components() {
                        continue;
                    }
                    tracing::info!("Replicating old tags for {name}...");
                    let components = stored.into_components();
                    for (name, tag_spec) in components.into_iter() {
                        let tag = self.inner.resolve_tag(&tag_spec).await?;
                        let new_tag_path = self.build_package_tag(&build)?.join(name.to_string());
                        let new_tag_spec = spfs::tracking::TagSpec::parse(&new_tag_path)?;

                        // NOTE(rbottriell): this copying process feels annoying
                        // and error prone. Ideally, there would be some set methods
                        // on the tag for changing the org/name on an existing one
                        let mut new_tag = spfs::tracking::Tag::new(
                            new_tag_spec.org(),
                            new_tag_spec.name(),
                            tag.target,
                        )?;
                        new_tag.parent = tag.parent;
                        new_tag.time = tag.time;
                        new_tag.user = tag.user;

                        self.insert_tag(&new_tag).await?;
                    }
                }
            }
        }
        meta.version = target_version;
        self.write_metadata(&meta).await?;
        // Note caches are already invalidated in `write_metadata`
        Ok("Repo up to date".to_string())
    }

    fn set_cache_policy(&self, cache_policy: CachePolicy) -> CachePolicy {
        let orig = self
            .cache_policy
            .swap(Box::leak(Box::new(cache_policy)), Ordering::Relaxed);

        // Safety: We only put valid `Box` pointers into `self.cache_policy`.
        *unsafe { Box::from_raw(orig) }
    }
}

impl SPFSRepository {
    fn cached_result_permitted(&self) -> bool {
        // Safety: We only put valid `Box` pointers into `self.cache_policy`.
        unsafe { *self.cache_policy.load(Ordering::Relaxed) }.cached_result_permitted()
    }

    async fn has_tag(&self, for_pkg: &Ident, tag: &tracking::TagSpec) -> bool {
        // This goes through the cache!
        self.resolve_tag(for_pkg, tag).await.is_ok()
    }

    /// Invalidate (clear) all cached results.
    fn invalidate_caches(&self) {
        self.caches.ls_tags.clear();
        self.caches.package_versions.clear();
        self.caches.recipe.clear();
        self.caches.package.clear();
        self.caches.tag_spec.clear();
        self.caches.list_build_components.clear();
    }

    async fn ls_tags(&self, path: &relative_path::RelativePath) -> Vec<Result<EntryType>> {
        if self.cached_result_permitted() {
            if let Some(v) = self.caches.ls_tags.get(path) {
                return v
                    .value()
                    .clone()
                    .into_iter()
                    .map(Ok)
                    .collect::<Vec<Result<EntryType>>>();
            }
        }
        let r: Vec<Result<EntryType>> = self
            .inner
            .ls_tags(path)
            .map(|el| el.map_err(|err| err.into()))
            .collect::<Vec<_>>()
            .await;

        self.caches.ls_tags.insert(
            path.to_owned(),
            r.iter().filter_map(|r| r.as_ref().ok()).cloned().collect(),
        );
        r
    }

    /// Read the metadata for this spk repository.
    ///
    /// The repo metadata contains information about
    /// how this particular spfs repository has been setup
    /// with spk. Namely, version and compatibility information.
    pub async fn read_metadata(&self) -> Result<RepositoryMetadata> {
        let tag_spec = spfs::tracking::TagSpec::parse(REPO_METADATA_TAG).unwrap();
        let digest = match self.inner.resolve_tag(&tag_spec).await {
            Ok(tag) => tag.target,
            Err(spfs::Error::UnknownReference(_)) => return Ok(Default::default()),
            Err(err) => return Err(err.into()),
        };
        let (mut reader, _) = self.inner.open_payload(digest).await?;
        let mut yaml = String::new();
        reader
            .read_to_string(&mut yaml)
            .await
            .map_err(|err| Error::FileReadError(digest.to_string().into(), err))?;
        let meta: RepositoryMetadata =
            serde_yaml::from_str(&yaml).map_err(Error::InvalidRepositoryMetadata)?;
        Ok(meta)
    }

    async fn resolve_tag(
        &self,
        for_pkg: &Ident,
        tag_spec: &tracking::TagSpec,
    ) -> Result<tracking::Tag> {
        if self.cached_result_permitted() {
            if let Some(v) = self.caches.tag_spec.get(tag_spec) {
                return v.value().clone().into();
            }
        }
        let r = self
            .inner
            .resolve_tag(tag_spec)
            .await
            .map_err(|err| match err {
                spfs::Error::UnknownReference(_) => Error::SpkValidatorsError(
                    spk_schema::validators::Error::PackageNotFoundError(for_pkg.clone()),
                ),
                err => err.into(),
            });

        self.caches
            .tag_spec
            .insert(tag_spec.clone(), r.as_ref().map(|el| el.clone()).into());
        r
    }

    /// Update the metadata for this spk repository.
    async fn write_metadata(&self, meta: &RepositoryMetadata) -> Result<()> {
        let tag_spec = spfs::tracking::TagSpec::parse(REPO_METADATA_TAG).unwrap();
        let yaml = serde_yaml::to_string(meta).map_err(Error::InvalidRepositoryMetadata)?;
        let digest = self
            .inner
            .commit_blob(Box::pin(std::io::Cursor::new(yaml.into_bytes())))
            .await?;
        self.inner.push_tag(&tag_spec, &digest).await?;
        self.invalidate_caches();
        Ok(())
    }

    /// Find a package stored in this repo in either the new or old way of tagging
    ///
    /// (with or without package components)
    async fn lookup_package(&self, pkg: &Ident) -> Result<StoredPackage> {
        use spfs::tracking::TagSpec;
        let tag_path = self.build_package_tag(pkg)?;
        let tag_specs: HashMap<Component, TagSpec> = self
            .ls_tags(&tag_path)
            .await
            .into_iter()
            .filter_map(|entry| match entry {
                Ok(EntryType::Tag(name)) => Some(name),
                Ok(EntryType::Folder(_)) => None,
                Err(_) => None,
            })
            .filter_map(|e| Component::parse(&e).map(|c| (c, e)).ok())
            .filter_map(|(c, e)| TagSpec::parse(&tag_path.join(e)).map(|p| (c, p)).ok())
            .collect();
        if !tag_specs.is_empty() {
            return Ok(StoredPackage::WithComponents(tag_specs));
        }
        let tag_spec = spfs::tracking::TagSpec::parse(&tag_path)?;
        if self.has_tag(pkg, &tag_spec).await {
            return Ok(StoredPackage::WithoutComponents(tag_spec));
        }
        Err(Error::SpkValidatorsError(
            spk_schema::validators::Error::PackageNotFoundError(pkg.clone()),
        ))
    }

    /// Construct an spfs tag string to represent a binary package layer.
    fn build_package_tag(&self, pkg: &Ident) -> Result<RelativePathBuf> {
        if pkg.build.is_none() {
            return Err(InvalidBuildError::new_error(
                "Package must have associated build digest".to_string(),
            )
            .into());
        }

        let mut tag = RelativePathBuf::from("spk");
        tag.push("pkg");
        tag.push(pkg.tag_path());

        Ok(tag)
    }

    /// Construct an spfs tag string to represent a spec file blob.
    fn build_spec_tag(&self, pkg: &Ident) -> RelativePathBuf {
        let mut tag = RelativePathBuf::from("spk");
        tag.push("spec");
        tag.push(pkg.tag_path());

        tag
    }

    pub fn flush(&mut self) -> Result<()> {
        match &mut self.inner {
            spfs::storage::RepositoryHandle::Tar(tar) => Ok(tar.flush()?),
            _ => Ok(()),
        }
    }
}

#[derive(Deserialize, Serialize, Default, Debug, PartialEq, Eq)]
pub struct RepositoryMetadata {
    version: Version,
}

/// A simple enum that allows us to represent both the old and new form
/// of package storage as spfs tags.
enum StoredPackage {
    WithoutComponents(spfs::tracking::TagSpec),
    WithComponents(HashMap<Component, spfs::tracking::TagSpec>),
}

impl StoredPackage {
    /// true if this stored package uses the new format with
    /// tags for each package component
    fn has_components(&self) -> bool {
        matches!(self, Self::WithComponents(_))
    }

    /// Identify all of the tags associated with this package
    fn tags(&self) -> Vec<&spfs::tracking::TagSpec> {
        match &self {
            Self::WithoutComponents(tag) => vec![tag],
            Self::WithComponents(cmpts) => cmpts.values().collect(),
        }
    }

    /// Return the mapped component tags for this package, converting
    /// from the legacy storage format if needed.
    fn into_components(self) -> HashMap<Component, spfs::tracking::TagSpec> {
        match self {
            Self::WithComponents(cmpts) => cmpts,
            Self::WithoutComponents(tag) if tag.name() == "src" => {
                vec![(Component::Source, tag)].into_iter().collect()
            }
            Self::WithoutComponents(tag) => {
                vec![(Component::Build, tag.clone()), (Component::Run, tag)]
                    .into_iter()
                    .collect()
            }
        }
    }
}

/// Return the local packages repository used for development.
pub async fn local_repository() -> Result<SPFSRepository> {
    let config = spfs::get_config()?;
    let repo = config.get_local_repository().await?;
    let inner: spfs::prelude::RepositoryHandle = repo.into();
    let address = inner.address();
    Ok(SPFSRepository {
        caches: CachesForAddress::new(&address),
        address,
        name: "local".try_into()?,
        inner,
        cache_policy: AtomicPtr::new(Box::leak(Box::new(CachePolicy::CacheOk))),
    })
}

/// Return the remote repository of the given name.
///
/// If not name is specified, return the default spfs repository.
pub async fn remote_repository<S: AsRef<str>>(name: S) -> Result<SPFSRepository> {
    let config = spfs::get_config()?;
    let inner = config.get_remote(&name).await?;
    let address = inner.address();
    Ok(SPFSRepository {
        caches: CachesForAddress::new(&address),
        address,
        name: name.as_ref().try_into()?,
        inner,
        cache_policy: AtomicPtr::new(Box::leak(Box::new(CachePolicy::CacheOk))),
    })
}
