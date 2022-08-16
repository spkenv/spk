// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    convert::{TryFrom, TryInto},
    str::FromStr,
    sync::{
        atomic::{AtomicPtr, Ordering},
        Arc,
    },
};

use futures::StreamExt;
use itertools::Itertools;
use relative_path::RelativePathBuf;
use serde_derive::{Deserialize, Serialize};
use spfs::{storage::EntryType, tracking};
use spk_foundation::ident_build::{parse_build, Build, InvalidBuildError};
use spk_foundation::ident_component::Component;
use spk_foundation::spec_ops::{PackageOps, RecipeOps};
use spk_ident::Ident;
use spk_name::{PkgName, PkgNameBuf, RepositoryName, RepositoryNameBuf};
use spk_spec::{Spec, SpecRecipe};
use spk_version::{parse_version, Version};
use tokio::io::AsyncReadExt;

use super::{CachePolicy, Repository};
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
        Ok(Self {
            address: inner.address(),
            name: name_and_repo.0.as_ref().try_into()?,
            inner,
            cache_policy: AtomicPtr::new(Box::leak(Box::new(CachePolicy::CacheOk))),
        })
    }
}

impl SPFSRepository {
    pub async fn new(name: &str, address: &str) -> Result<Self> {
        let inner = spfs::open_repository(address).await?;
        Ok(Self {
            address: inner.address(),
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
            Box::from_raw(self.cache_policy.load(Ordering::Relaxed));
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
            CacheValue::InvalidPackageSpec(i, err) => Err(crate::Error::InvalidPackageSpec(
                i,
                serde::ser::Error::custom(err),
            )),
            CacheValue::PackageNotFoundError(i) => Err(crate::Error::SpkValidatorsError(
                spk_validators::Error::PackageNotFoundError(i),
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
            Err(crate::Error::SpkValidatorsError(spk_validators::Error::PackageNotFoundError(
                i,
            ))) => CacheValue::PackageNotFoundError(i.clone()),
            Err(crate::Error::String(s)) => CacheValue::StringError(s.clone()),
            // Decorate the error message so we can tell it was a custom error
            // downgraded to a String.
            Err(err) => CacheValue::StringifiedError(format!("Cached error: {}", err)),
        }
    }
}

// Cache is KKV with outer key being the repo address.
type CacheByAddress<K, V> = RefCell<HashMap<url::Url, HashMap<K, V>>>;

std::thread_local! {
    static LS_TAGS_CACHE : CacheByAddress<
        relative_path::RelativePathBuf,
        Vec<EntryType>
    > = RefCell::new(HashMap::new());

    static PACKAGE_VERSIONS_CACHE : CacheByAddress<
        PkgNameBuf,
        CacheValue<Arc<Vec<Arc<Version>>>>
    > = RefCell::new(HashMap::new());

    static RECIPE_CACHE : CacheByAddress<
        Ident,
        CacheValue<Arc<spk_spec::SpecRecipe>>
    > = RefCell::new(HashMap::new());

    static PACKAGE_CACHE : CacheByAddress<
        Ident,
        CacheValue<Arc<Spec>>
    > = RefCell::new(HashMap::new());

    static TAG_SPEC_CACHE : CacheByAddress<
        tracking::TagSpec,
        CacheValue<tracking::Tag>
    > = RefCell::new(HashMap::new());
}

#[async_trait::async_trait]
impl Repository for SPFSRepository {
    type Recipe = SpecRecipe;

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
        let address = self.address();
        if self.cached_result_permitted() {
            let r = PACKAGE_VERSIONS_CACHE.with(|hm| {
                hm.borrow()
                    .get(address)
                    .and_then(|hm| hm.get(name).cloned())
            });
            if let Some(r) = r {
                return r.into();
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
        PACKAGE_VERSIONS_CACHE.with(|hm| {
            let mut hm = hm.borrow_mut();
            let hm = hm.entry(address.clone()).or_insert_with(HashMap::new);
            hm.insert(name.to_owned(), r.as_ref().map(|b| b.clone()).into())
        });
        r
    }

    async fn list_package_builds(&self, pkg: &Ident) -> Result<Vec<Ident>> {
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
        // XXX: infallible vs return type
        Ok(builds.into_iter().collect_vec())
    }

    async fn list_build_components(&self, pkg: &Ident) -> Result<Vec<Component>> {
        match self.lookup_package(pkg).await {
            Ok(p) => Ok(p.into_components().into_keys().collect()),
            Err(Error::SpkValidatorsError(spk_validators::Error::PackageNotFoundError(_))) => {
                Ok(Vec::new())
            }
            Err(err) => Err(err),
        }
    }

    fn name(&self) -> &RepositoryName {
        &self.name
    }

    async fn read_recipe(&self, pkg: &Ident) -> Result<Arc<Self::Recipe>> {
        let address = self.address();
        if pkg.build.is_some() {
            return Err(format!("cannot read a recipe for a package build: {pkg}").into());
        }
        if self.cached_result_permitted() {
            let r = RECIPE_CACHE
                .with(|hm| hm.borrow().get(address).and_then(|hm| hm.get(pkg).cloned()));
            if let Some(r) = r {
                return r.into();
            }
        }
        let r: Result<Arc<SpecRecipe>> = async {
            let tag_path = self.build_spec_tag(pkg);
            let tag_spec = spfs::tracking::TagSpec::parse(&tag_path.as_str())?;
            let tag = self.resolve_tag(pkg, &tag_spec).await?;

            let (mut reader, _) = self.inner.open_payload(tag.target).await?;
            let mut yaml = String::new();
            reader.read_to_string(&mut yaml).await?;
            serde_yaml::from_str(&yaml)
                .map_err(|err| Error::InvalidPackageSpec(pkg.clone(), err))
                .map(Arc::new)
        }
        .await;
        RECIPE_CACHE.with(|hm| {
            let mut hm = hm.borrow_mut();
            let hm = hm.entry(address.clone()).or_insert_with(HashMap::new);
            hm.insert(pkg.clone(), r.as_ref().map(Arc::clone).into());
        });
        r
    }

    async fn read_components(
        &self,
        pkg: &Ident,
    ) -> Result<HashMap<Component, spfs::encoding::Digest>> {
        let package = self.lookup_package(pkg).await?;
        let component_tags = package.into_components();
        let mut components = HashMap::with_capacity(component_tags.len());
        for (name, tag_spec) in component_tags.into_iter() {
            let tag = self.resolve_tag(pkg, &tag_spec).await?;
            components.insert(name, tag.target);
        }
        Ok(components)
    }

    async fn read_package(
        &self,
        pkg: &Ident,
    ) -> Result<Arc<<Self::Recipe as spk_spec::Recipe>::Output>> {
        // TODO: reduce duplicate code with read_recipe
        let address = self.address();
        if self.cached_result_permitted() {
            let r = PACKAGE_CACHE
                .with(|hm| hm.borrow().get(address).and_then(|hm| hm.get(pkg).cloned()));
            if let Some(r) = r {
                return r.into();
            }
        }
        let r: Result<Arc<Spec>> = async {
            let tag_path = self.build_spec_tag(pkg);
            let tag_spec = spfs::tracking::TagSpec::parse(&tag_path.as_str())?;
            let tag = self.resolve_tag(pkg, &tag_spec).await?;

            let (mut reader, _) = self.inner.open_payload(tag.target).await?;
            let mut yaml = String::new();
            reader.read_to_string(&mut yaml).await?;
            serde_yaml::from_str(&yaml)
                .map_err(|err| Error::InvalidPackageSpec(pkg.clone(), err))
                .map(Arc::new)
        }
        .await;
        PACKAGE_CACHE.with(|hm| {
            let mut hm = hm.borrow_mut();
            let hm = hm.entry(address.clone()).or_insert_with(HashMap::new);
            hm.insert(pkg.clone(), r.as_ref().map(Arc::clone).into());
        });
        r
    }

    async fn publish_recipe(&self, spec: &Self::Recipe) -> Result<()> {
        let tag_path = self.build_spec_tag(&spec.to_ident());
        let tag_spec = spfs::tracking::TagSpec::parse(&tag_path.as_str())?;
        if self.inner.has_tag(&tag_spec).await {
            // BUG(rbottriell): this creates a race condition but is not super dangerous
            // because of the non-destructive tag history
            Err(Error::SpkValidatorsError(
                spk_validators::Error::VersionExistsError(spec.to_ident()),
            ))
        } else {
            self.force_publish_recipe(spec).await
        }
    }

    async fn remove_recipe(&self, pkg: &Ident) -> Result<()> {
        let tag_path = self.build_spec_tag(pkg);
        let tag_spec = spfs::tracking::TagSpec::parse(&tag_path)?;
        match self.inner.remove_tag_stream(&tag_spec).await {
            Err(spfs::Error::UnknownReference(_)) => Err(Error::SpkValidatorsError(
                spk_validators::Error::PackageNotFoundError(pkg.clone()),
            )),
            Err(err) => Err(err.into()),
            Ok(_) => {
                self.invalidate_caches();
                Ok(())
            }
        }
    }

    async fn force_publish_recipe(&self, spec: &Self::Recipe) -> Result<()> {
        let tag_path = self.build_spec_tag(&spec.to_ident());
        let tag_spec = spfs::tracking::TagSpec::parse(tag_path)?;

        let payload = serde_yaml::to_vec(&spec).map_err(spk_spec::Error::SpecEncodingError)?;
        let digest = self
            .inner
            .commit_blob(Box::pin(std::io::Cursor::new(payload)))
            .await?;
        self.inner.push_tag(&tag_spec, &digest).await?;
        self.invalidate_caches();
        Ok(())
    }

    async fn publish_package(
        &self,
        spec: &Spec,
        components: &HashMap<Component, spfs::encoding::Digest>,
    ) -> Result<()> {
        let tag_path = self.build_package_tag(spec.ident())?;

        // We will also publish the 'run' component in the old style
        // for compatibility with older versions of the spk command.
        // It's not perfect but at least the package will be visible
        let legacy_tag = spfs::tracking::TagSpec::parse(&tag_path)?;
        let legacy_component = if let Some(Build::Source) = spec.ident().build {
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

        if let Some(Build::Embedded) = spec.ident().build {
            return Err(Error::SpkIdentBuildError(InvalidBuildError::new_error(
                "Cannot publish embedded package".to_string(),
            )));
        }

        // TODO: dedupe this part with force_publish_recipe
        let tag_path = self.build_spec_tag(spec.ident());
        let tag_spec = spfs::tracking::TagSpec::parse(tag_path)?;
        let payload = serde_yaml::to_vec(&spec)
            .map_err(|err| Error::SpkSpecError(spk_spec::Error::SpecEncodingError(err)))?;
        let digest = self
            .inner
            .commit_blob(Box::pin(std::io::Cursor::new(payload)))
            .await?;
        self.inner.push_tag(&tag_spec, &digest).await?;
        self.invalidate_caches();
        Ok(())
    }

    async fn remove_package(&self, pkg: &Ident) -> Result<()> {
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
            tracing::info!("replicating old tags for {}...", name);
            let mut pkg = Ident::new(name.to_owned());
            for version in self.list_package_versions(&name).await?.iter() {
                pkg.version = (**version).clone();
                for build in self.list_package_builds(&pkg).await? {
                    let stored = with_cache_policy!(self, CachePolicy::BypassCache, {
                        self.lookup_package(&build)
                    })
                    .await?;
                    if stored.has_components() {
                        continue;
                    }
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
        Ok("All packages were re-tagged for components".to_string())
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
    ///
    /// # Warning
    ///
    /// This only operates on the caches for the current thread.
    fn invalidate_caches(&self) {
        let address = self.address();
        LS_TAGS_CACHE.with(|hm| hm.borrow_mut().get_mut(address).map(|hm| hm.clear()));
        PACKAGE_VERSIONS_CACHE.with(|hm| hm.borrow_mut().get_mut(address).map(|hm| hm.clear()));
        RECIPE_CACHE.with(|hm| hm.borrow_mut().get_mut(address).map(|hm| hm.clear()));
        PACKAGE_CACHE.with(|hm| hm.borrow_mut().get_mut(address).map(|hm| hm.clear()));
        TAG_SPEC_CACHE.with(|hm| hm.borrow_mut().get_mut(address).map(|hm| hm.clear()));
    }

    async fn ls_tags(&self, path: &relative_path::RelativePath) -> Vec<Result<EntryType>> {
        let address = self.address();
        if self.cached_result_permitted() {
            let r = LS_TAGS_CACHE.with(|hm| {
                hm.borrow()
                    .get(address)
                    .and_then(|hm| hm.get(path).cloned())
            });
            if let Some(r) = r {
                return r.into_iter().map(Ok).collect();
            }
        }
        let r: Vec<Result<EntryType>> = self
            .inner
            .ls_tags(path)
            .map(|el| el.map_err(|err| err.into()))
            .collect::<Vec<_>>()
            .await;
        LS_TAGS_CACHE.with(|hm| {
            let mut hm = hm.borrow_mut();
            let hm = hm.entry(address.clone()).or_insert_with(HashMap::new);
            hm.insert(
                path.to_owned(),
                r.iter().filter_map(|r| r.as_ref().ok()).cloned().collect(),
            );
        });
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
        reader.read_to_string(&mut yaml).await?;
        let meta: RepositoryMetadata =
            serde_yaml::from_str(&yaml).map_err(Error::InvalidRepositoryMetadata)?;
        Ok(meta)
    }

    async fn resolve_tag(
        &self,
        for_pkg: &Ident,
        tag_spec: &tracking::TagSpec,
    ) -> Result<tracking::Tag> {
        let address = self.address();
        if self.cached_result_permitted() {
            let r = TAG_SPEC_CACHE.with(|hm| {
                hm.borrow()
                    .get(address)
                    .and_then(|hm| hm.get(tag_spec).cloned())
            });
            if let Some(r) = r {
                return r.into();
            }
        }
        let r = self
            .inner
            .resolve_tag(tag_spec)
            .await
            .map_err(|err| match err {
                spfs::Error::UnknownReference(_) => Error::SpkValidatorsError(
                    spk_validators::Error::PackageNotFoundError(for_pkg.clone()),
                ),
                err => err.into(),
            });
        TAG_SPEC_CACHE.with(|hm| {
            let mut hm = hm.borrow_mut();
            let hm = hm.entry(address.clone()).or_insert_with(HashMap::new);
            hm.insert(tag_spec.clone(), r.as_ref().map(|el| el.clone()).into());
        });
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
            spk_validators::Error::PackageNotFoundError(pkg.clone()),
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
        // the "+" character is not a valid spfs tag character,
        // so we 'encode' it with two dots, which is not a valid sequence
        // for spk package names
        tag.push(pkg.to_string().replace('+', ".."));

        Ok(tag)
    }

    /// Construct an spfs tag string to represent a spec file blob.
    fn build_spec_tag(&self, pkg: &Ident) -> RelativePathBuf {
        let mut tag = RelativePathBuf::from("spk");
        tag.push("spec");
        // the "+" character is not a valid spfs tag character,
        // see above ^
        tag.push(pkg.to_string().replace('+', ".."));
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
    Ok(SPFSRepository {
        address: inner.address(),
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
    let repo = config.get_remote(&name).await?;
    Ok(SPFSRepository {
        address: repo.address(),
        name: name.as_ref().try_into()?,
        inner: repo,
        cache_policy: AtomicPtr::new(Box::leak(Box::new(CachePolicy::CacheOk))),
    })
}
