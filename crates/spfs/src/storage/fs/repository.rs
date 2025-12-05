// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::borrow::Cow;
#[cfg(unix)]
use std::fs::Permissions;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::prelude::PermissionsExt;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use arc_swap::ArcSwap;
use async_stream::try_stream;
use chrono::{DateTime, Utc};
use futures::Stream;

use super::FsHashStore;
use super::hash_store::PROXY_DIRNAME;
use super::migrations::{MigrationError, MigrationResult};
use crate::config::{ToAddress, pathbuf_deserialize_with_tilde_expansion};
use crate::runtime::makedirs_with_perms;
use crate::storage::prelude::*;
use crate::storage::{
    LocalRepository,
    OpenRepositoryError,
    OpenRepositoryResult,
    TagNamespace,
    TagNamespaceBuf,
};
use crate::{Error, Result};

/// The directory name within the repo where durable runtimes keep
/// their upper path roots and upper/work directories.
pub const DURABLE_EDITS_DIR: &str = "durable_edits";

/// Configuration for an fs repository
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct Config {
    #[serde(deserialize_with = "pathbuf_deserialize_with_tilde_expansion::deserialize")]
    pub path: std::path::PathBuf,
    #[serde(flatten)]
    pub params: Params,
}

impl ToAddress for Config {
    fn to_address(&self) -> Result<url::Url> {
        let mut addr = url::Url::from_directory_path(&self.path).map_err(|err| {
            crate::Error::String(format!("Repository path is not a valid address: {err:?}"))
        })?;
        let query = serde_qs::to_string(&self.params).map_err(|err| {
            crate::Error::String(format!(
                "FS repo parameters do not create a valid url: {err:?}"
            ))
        })?;
        addr.set_query(Some(&query));
        Ok(addr)
    }
}

#[derive(Clone, Default, Debug, serde::Deserialize, serde::Serialize)]
pub struct Params {
    #[serde(default)]
    pub create: bool,
    #[serde(default)]
    pub lazy: bool,
    pub tag_namespace: Option<TagNamespaceBuf>,
}

#[async_trait::async_trait]
impl FromUrl for Config {
    async fn from_url(url: &url::Url) -> crate::storage::OpenRepositoryResult<Self> {
        let params = if let Some(qs) = url.query() {
            serde_qs::from_str(qs)
                .map_err(|source| crate::storage::OpenRepositoryError::invalid_query(url, source))?
        } else {
            Params::default()
        };
        // Use to_file_path() to properly decode percent-encoded characters
        // (e.g., %20 -> space) in the URL path. This also handles Windows
        // drive letters correctly (e.g., file:///C:/path -> C:\path).
        let path = url.to_file_path().map_err(|_| {
            crate::storage::OpenRepositoryError::UnsupportedRepositoryType(format!(
                "Invalid file URL: {}",
                url
            ))
        })?;
        Ok(Self { path, params })
    }
}

/// Renders need a place for proxy files and the rendered hard links.
pub struct RenderStore {
    pub proxy: FsHashStore,
    pub renders: FsHashStore,
}

impl RenderStore {
    pub fn for_user<P: AsRef<Path>>(root: &Path, username: P) -> Result<Self> {
        let username = username.as_ref();
        let renders_dir = root.join("renders").join(username);
        FsHashStore::open(renders_dir.join(PROXY_DIRNAME))
            .and_then(|proxy| {
                FsHashStore::open(&renders_dir).map(|renders| RenderStore { proxy, renders })
            })
            .map_err(|source| Error::FailedToOpenRepository {
                repository: format!("<Render Storage for {}>", username.display()),
                source,
            })
    }
}

impl Clone for RenderStore {
    fn clone(&self) -> Self {
        Self {
            proxy: FsHashStore::open_unchecked(self.proxy.root()),
            renders: FsHashStore::open_unchecked(self.renders.root()),
        }
    }
}

/// Operations on a FsRepository.
#[async_trait::async_trait]
pub trait FsRepositoryOps: Send + Sync {
    /// True if this repo is setup to generate local manifest renders.
    fn has_renders(&self) -> bool;

    fn iter_rendered_manifests(
        &self,
    ) -> Pin<Box<dyn Stream<Item = Result<crate::encoding::Digest>> + Send + Sync + '_>>;

    fn proxy_path(&self) -> Option<&std::path::Path>;

    /// Remove the identified render from this storage.
    async fn remove_rendered_manifest(&self, digest: crate::encoding::Digest) -> Result<()>;

    /// Returns true if the render was actually removed
    async fn remove_rendered_manifest_if_older_than(
        &self,
        older_than: DateTime<Utc>,
        digest: crate::encoding::Digest,
    ) -> Result<bool>;

    /// Returns a list of the render storage for all the users
    /// with renders found in the repository, if any.
    ///
    /// Returns tuples of (username, `FsRepositoryOps`).
    fn renders_for_all_users(&self) -> Result<Vec<(String, impl FsRepositoryOps)>>;
}

#[async_trait::async_trait]
impl<T> FsRepositoryOps for &T
where
    T: FsRepositoryOps,
{
    fn has_renders(&self) -> bool {
        T::has_renders(*self)
    }

    fn iter_rendered_manifests(
        &self,
    ) -> Pin<Box<dyn Stream<Item = Result<crate::encoding::Digest>> + Send + Sync + '_>> {
        T::iter_rendered_manifests(*self)
    }

    fn proxy_path(&self) -> Option<&std::path::Path> {
        T::proxy_path(*self)
    }

    async fn remove_rendered_manifest(&self, digest: crate::encoding::Digest) -> Result<()> {
        T::remove_rendered_manifest(*self, digest).await
    }

    async fn remove_rendered_manifest_if_older_than(
        &self,
        older_than: DateTime<Utc>,
        digest: crate::encoding::Digest,
    ) -> Result<bool> {
        T::remove_rendered_manifest_if_older_than(*self, older_than, digest).await
    }

    fn renders_for_all_users(&self) -> Result<Vec<(String, impl FsRepositoryOps)>> {
        T::renders_for_all_users(*self)
    }
}

/// A pure filesystem-based repository of spfs data.
#[derive(Clone, Debug)]
pub struct FsRepository<FS> {
    pub(crate) fs_impl: Arc<FS>,
}

impl<FS> std::ops::Deref for FsRepository<FS> {
    type Target = FS;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.fs_impl
    }
}

#[async_trait::async_trait]
impl<FS> FsRepositoryOps for FsRepository<FS>
where
    FS: FsRepositoryOps,
{
    fn has_renders(&self) -> bool {
        self.fs_impl.has_renders()
    }

    fn iter_rendered_manifests(
        &self,
    ) -> Pin<Box<dyn Stream<Item = Result<crate::encoding::Digest>> + Send + Sync + '_>> {
        self.fs_impl.iter_rendered_manifests()
    }

    fn proxy_path(&self) -> Option<&std::path::Path> {
        self.fs_impl.proxy_path()
    }

    async fn remove_rendered_manifest(&self, digest: crate::encoding::Digest) -> Result<()> {
        self.fs_impl.remove_rendered_manifest(digest).await
    }

    async fn remove_rendered_manifest_if_older_than(
        &self,
        older_than: DateTime<Utc>,
        digest: crate::encoding::Digest,
    ) -> Result<bool> {
        self.fs_impl
            .remove_rendered_manifest_if_older_than(older_than, digest)
            .await
    }

    fn renders_for_all_users(&self) -> Result<Vec<(String, impl FsRepositoryOps)>> {
        self.fs_impl.renders_for_all_users()
    }
}

impl<FS> Address for FsRepository<FS>
where
    FS: Address,
{
    fn address(&self) -> Cow<'_, url::Url> {
        self.fs_impl.address()
    }
}

impl<FS> LocalRepository for FsRepository<FS>
where
    FS: LocalRepository,
{
    fn payloads(&self) -> &FsHashStore {
        self.fs_impl.payloads()
    }

    fn render_store(&self) -> Result<&RenderStore> {
        self.fs_impl.render_store()
    }
}

pub type MaybeOpenFsRepository = FsRepository<MaybeOpenFsRepositoryImpl>;
pub type OpenFsRepository = FsRepository<OpenFsRepositoryImpl>;

impl MaybeOpenFsRepository {
    /// Get the opened version of this repository, performing
    /// any required opening and validation as needed
    pub fn opened(&self) -> impl futures::Future<Output = Result<OpenFsRepository>> + 'static {
        let fs_impl = Arc::clone(&self.fs_impl);
        async move {
            let fs_impl = fs_impl
                .opened_and_map_err(Error::failed_to_open_repository)
                .await?;
            Ok(OpenFsRepository { fs_impl })
        }
    }

    /// Open a filesystem repository, creating it if necessary
    pub async fn create<P: AsRef<Path>>(root: P) -> OpenRepositoryResult<Self> {
        MaybeOpenFsRepositoryImpl::create(root)
            .await
            .map(Into::into)
            .map(|fs_impl| FsRepository { fs_impl })
    }
}

#[async_trait::async_trait]
impl FromConfig for MaybeOpenFsRepository {
    type Config = Config;

    async fn from_config(config: Self::Config) -> crate::storage::OpenRepositoryResult<Self> {
        MaybeOpenFsRepositoryImpl::from_config(config)
            .await
            .map(Into::into)
            .map(|fs_impl| FsRepository { fs_impl })
    }
}

impl OpenFsRepository {
    /// Establish a new filesystem repository
    pub async fn create<P: AsRef<Path>>(root: P) -> OpenRepositoryResult<Self> {
        OpenFsRepositoryImpl::create(root)
            .await
            .map(Into::into)
            .map(|fs_impl| FsRepository { fs_impl })
    }
}

impl From<OpenFsRepository> for MaybeOpenFsRepository {
    fn from(value: OpenFsRepository) -> Self {
        MaybeOpenFsRepository {
            fs_impl: Arc::new(MaybeOpenFsRepositoryImpl(Arc::new(ArcSwap::new(Arc::new(
                InnerFsRepository::Open(value.fs_impl),
            ))))),
        }
    }
}

impl From<Arc<OpenFsRepository>> for MaybeOpenFsRepository {
    fn from(value: Arc<OpenFsRepository>) -> Self {
        MaybeOpenFsRepository {
            fs_impl: Arc::new(MaybeOpenFsRepositoryImpl(Arc::new(ArcSwap::new(Arc::new(
                InnerFsRepository::Open(Arc::clone(&value.fs_impl)),
            ))))),
        }
    }
}

/// A pure filesystem-based repository of spfs data.
///
/// This instance can be already validated and open or
/// lazily evaluated on the each request until successful.
///
/// An [`OpenFsRepository`] is more useful than this one, but
/// can also be easily retrieved via the [`Self::opened`].
#[derive(Clone)]
pub struct MaybeOpenFsRepositoryImpl(Arc<ArcSwap<InnerFsRepository>>);

enum InnerFsRepository {
    Closed(Config),
    Open(Arc<OpenFsRepositoryImpl>),
}

impl From<OpenFsRepositoryImpl> for MaybeOpenFsRepositoryImpl {
    fn from(value: OpenFsRepositoryImpl) -> Self {
        Arc::new(value).into()
    }
}

impl From<Arc<OpenFsRepositoryImpl>> for MaybeOpenFsRepositoryImpl {
    fn from(value: Arc<OpenFsRepositoryImpl>) -> Self {
        Self(Arc::new(ArcSwap::new(Arc::new(InnerFsRepository::Open(
            value,
        )))))
    }
}

#[async_trait::async_trait]
impl FromConfig for MaybeOpenFsRepositoryImpl {
    type Config = Config;

    async fn from_config(config: Self::Config) -> crate::storage::OpenRepositoryResult<Self> {
        if config.params.lazy {
            Ok(Self(Arc::new(ArcSwap::new(Arc::new(
                InnerFsRepository::Closed(config),
            )))))
        } else {
            Ok(OpenFsRepositoryImpl::from_config(config).await?.into())
        }
    }
}

impl MaybeOpenFsRepositoryImpl {
    /// Open a filesystem repository, creating it if necessary
    pub async fn create<P: AsRef<Path>>(root: P) -> OpenRepositoryResult<Self> {
        Ok(Self(Arc::new(ArcSwap::new(Arc::new(
            InnerFsRepository::Open(Arc::new(OpenFsRepositoryImpl::create(root).await?)),
        )))))
    }

    // Open a repository over the given directory, which must already
    // exist and be properly setup as a repository
    pub async fn open<P: AsRef<Path>>(root: P) -> OpenRepositoryResult<Self> {
        let root = root.as_ref();
        Ok(Self(Arc::new(ArcSwap::new(Arc::new(
            InnerFsRepository::Open(Arc::new(OpenFsRepositoryImpl::open(&root).await?)),
        )))))
    }

    /// Get the opened version of this repository, performing
    /// any required opening and validation as needed
    pub fn opened(
        &self,
    ) -> impl futures::Future<Output = Result<Arc<OpenFsRepositoryImpl>>> + 'static {
        self.opened_and_map_err(Error::failed_to_open_repository)
    }

    /// Get the opened version of this repository, performing
    /// any required opening and validation as needed
    pub fn try_open(
        &self,
    ) -> impl futures::Future<Output = OpenRepositoryResult<Arc<OpenFsRepositoryImpl>>> + 'static
    {
        self.opened_and_map_err(|_, e| e)
    }

    fn opened_and_map_err<F, E>(
        &self,
        map: F,
    ) -> impl futures::Future<Output = std::result::Result<Arc<OpenFsRepositoryImpl>, E>> + 'static
    where
        F: FnOnce(&Self, OpenRepositoryError) -> E + 'static,
    {
        let inner = Arc::clone(&self.0);
        async move {
            match &**inner.load() {
                InnerFsRepository::Closed(config) => {
                    let config = config.clone();
                    let opened = match OpenFsRepositoryImpl::from_config(config).await {
                        Ok(o) => Arc::new(o),
                        Err(err) => return Err(map(&Self(inner), err)),
                    };
                    inner.rcu(|_| InnerFsRepository::Open(Arc::clone(&opened)));
                    Ok(opened)
                }
                InnerFsRepository::Open(o) => Ok(Arc::clone(o)),
            }
        }
    }

    /// The filesystem root path of this repository
    pub fn root(&self) -> PathBuf {
        match &**self.0.load() {
            InnerFsRepository::Closed(config) => config.path.clone(),
            InnerFsRepository::Open(o) => o.root(),
        }
    }

    pub fn get_tag_namespace(&self) -> Option<Cow<'_, TagNamespace>> {
        match &**self.0.load() {
            InnerFsRepository::Open(repo) => repo
                .get_tag_namespace()
                .as_deref()
                .map(ToOwned::to_owned)
                .map(Cow::Owned),
            InnerFsRepository::Closed(config) => config
                .params
                .tag_namespace
                .as_ref()
                .map(|ns| Cow::Owned(ns.clone())),
        }
    }

    pub fn set_tag_namespace(
        &mut self,
        tag_namespace: Option<TagNamespaceBuf>,
    ) -> Option<TagNamespaceBuf> {
        let mut old_namespace = None;
        self.0.rcu(|inner| match &**inner {
            InnerFsRepository::Open(repo) => {
                let mut new_repo = (**repo).clone();
                old_namespace = new_repo.set_tag_namespace(tag_namespace.clone());
                InnerFsRepository::Open(Arc::new(new_repo))
            }
            InnerFsRepository::Closed(config) => {
                let mut new_config = config.clone();
                old_namespace.clone_from(&new_config.params.tag_namespace);
                new_config.params.tag_namespace.clone_from(&tag_namespace);
                InnerFsRepository::Closed(new_config)
            }
        });
        old_namespace
    }
}

impl Address for MaybeOpenFsRepositoryImpl {
    fn address(&self) -> Cow<'_, url::Url> {
        Cow::Owned(url::Url::from_directory_path(self.root()).unwrap())
    }
}

impl std::fmt::Debug for MaybeOpenFsRepositoryImpl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("FsRepository @ {:?}", self.root()))
    }
}

/// A validated and opened fs repository.
pub struct OpenFsRepositoryImpl {
    root: PathBuf,
    /// the namespace to use for tag resolution. If set, then this is treated
    /// as "chroot" of the real tag root.
    tag_namespace: Option<TagNamespaceBuf>,
    /// stores the actual file data/payloads of this repo
    pub payloads: FsHashStore,
    /// stores all digraph object data for this repo
    pub objects: FsHashStore,
    /// stores rendered file system layers for use in overlayfs
    pub renders: Option<RenderStore>,
}

#[async_trait::async_trait]
impl FromConfig for OpenFsRepositoryImpl {
    type Config = Config;

    async fn from_config(config: Self::Config) -> crate::storage::OpenRepositoryResult<Self> {
        let repo = if config.params.create {
            Self::create(&config.path).await
        } else {
            Self::open(&config.path).await
        };
        repo.map(|mut repo| {
            repo.set_tag_namespace(config.params.tag_namespace);
            repo
        })
    }
}

impl Clone for OpenFsRepositoryImpl {
    fn clone(&self) -> Self {
        let root = self.root.clone();
        Self {
            objects: FsHashStore::open_unchecked(root.join("objects")),
            payloads: FsHashStore::open_unchecked(root.join("payloads")),
            renders: self.renders.clone(),
            root,
            tag_namespace: self.tag_namespace.clone(),
        }
    }
}

impl LocalRepository for OpenFsRepositoryImpl {
    #[inline]
    fn payloads(&self) -> &FsHashStore {
        &self.payloads
    }

    #[inline]
    fn render_store(&self) -> Result<&RenderStore> {
        self.renders
            .as_ref()
            .ok_or_else(|| Error::NoRenderStorage(self.address()))
    }
}

impl OpenFsRepositoryImpl {
    /// The address of this repository that can be used to re-open it
    pub fn address(&self) -> url::Url {
        Config {
            path: self.root(),
            params: Params {
                create: false,
                lazy: false,
                tag_namespace: self.tag_namespace.clone(),
            },
        }
        .to_address()
        .expect("repository address is valid")
    }

    /// Establish a new filesystem repository
    pub async fn create<P: AsRef<Path>>(root: P) -> OpenRepositoryResult<Self> {
        let root = root.as_ref();
        // avoid creating any blocking tasks so as to not spawn
        // threads for the case where this repo is being opened as
        // part of the runtime setup process on linux
        makedirs_with_perms(root, 0o777).map_err(|source| {
            OpenRepositoryError::PathNotInitialized {
                path: root.to_owned(),
                source,
            }
        })?;
        let root = dunce::canonicalize(root).map_err(|source| {
            OpenRepositoryError::PathNotInitialized {
                path: root.to_owned(),
                source,
            }
        })?;
        let username = whoami::username();
        for path in [
            root.join("tags"),
            root.join("objects"),
            root.join("payloads"),
            root.join("renders").join(username).join(PROXY_DIRNAME),
            root.join(DURABLE_EDITS_DIR),
        ] {
            makedirs_with_perms(&path, 0o777)
                .map_err(|source| OpenRepositoryError::PathNotInitialized { path, source })?;
        }

        set_last_migration(&root, None).await?;
        // Safety: we canonicalized `root` and we just changed the repo
        // `VERSION` to our version, so it is compatible.
        // FIXME: No attempt to check if the repo already existed and is
        // actually incompatible.
        unsafe { Self::open_unchecked(root) }
    }

    pub(crate) fn get_render_storage(&self) -> Result<&crate::storage::fs::FsHashStore> {
        match &self.renders {
            Some(render_store) => Ok(&render_store.renders),
            None => Err(Error::NoRenderStorage(self.address())),
        }
    }

    /// Return the configured tag namespace, if any.
    #[inline]
    pub fn get_tag_namespace(&self) -> Option<Cow<'_, TagNamespace>> {
        self.tag_namespace.as_deref().map(Cow::Borrowed)
    }

    /// The latest repository version that this was migrated to.
    pub async fn last_migration(&self) -> MigrationResult<semver::Version> {
        Ok(read_last_migration_version(self.root())
            .await?
            .unwrap_or_else(|| {
                semver::Version::parse(crate::VERSION)
                    .expect("crate::VERSION is a valid semver value")
            }))
    }

    // Open a repository over the given directory, which must already
    // exist and be a repository
    pub async fn open<P: AsRef<Path>>(root: P) -> OpenRepositoryResult<Self> {
        // although this is an async function, we avoid spawning a blocking task
        // here for the cases where a local fs repo is opened to spawn a runtime
        // and the program cannot spawn another thread without angering the kernel
        let root = match dunce::canonicalize(&root) {
            Ok(r) => r,
            Err(source) => {
                return Err(OpenRepositoryError::PathNotInitialized {
                    path: root.as_ref().into(),
                    source,
                });
            }
        };

        // Safety: we canonicalized `root` and check the version compatibility
        // in the next step.
        let repo = unsafe { Self::open_unchecked(&root)? };

        let current_version = semver::Version::parse(crate::VERSION).unwrap();
        let repo_version = repo.last_migration().await?;
        if repo_version.major > current_version.major {
            return Err(OpenRepositoryError::VersionIsTooNew { repo_version });
        }
        if repo_version.major < current_version.major {
            return Err(OpenRepositoryError::VersionIsTooOld { repo_version });
        }

        Ok(repo)
    }

    /// Open a repository at the given directory, without reading or verifying
    /// the migration version of the repository.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `root` is canonicalized.
    ///
    /// The caller must ensure that the repository version is compatible with
    /// this version of spfs before using the repository.
    unsafe fn open_unchecked<P: AsRef<Path>>(root: P) -> OpenRepositoryResult<Self> {
        let root = root.as_ref();
        let username = whoami::username();
        Ok(Self {
            objects: FsHashStore::open(root.join("objects"))?,
            payloads: FsHashStore::open(root.join("payloads"))?,
            renders: RenderStore::for_user(root, username).ok(),
            root: root.to_owned(),
            tag_namespace: None,
        })
    }

    /// The filesystem root path of this repository
    pub fn root(&self) -> PathBuf {
        self.root.clone()
    }

    /// Sets the latest version of this repository.
    ///
    /// Should only be modified once a migration has completed successfully.
    pub async fn set_last_migration(&self, version: semver::Version) -> MigrationResult<()> {
        set_last_migration(self.root(), Some(version)).await
    }

    /// Set the configured tag namespace, returning the old tag namespace,
    /// if there was one.
    pub fn set_tag_namespace(
        &mut self,
        tag_namespace: Option<TagNamespaceBuf>,
    ) -> Option<TagNamespaceBuf> {
        std::mem::replace(&mut self.tag_namespace, tag_namespace)
    }
}

#[async_trait::async_trait]
impl FsRepositoryOps for OpenFsRepositoryImpl {
    /// True if this repo is setup to generate local manifest renders.
    fn has_renders(&self) -> bool {
        self.renders.is_some()
    }

    fn iter_rendered_manifests(
        &self,
    ) -> Pin<Box<dyn Stream<Item = Result<crate::encoding::Digest>> + Send + Sync + '_>> {
        Box::pin(try_stream! {
            let renders = self.get_render_storage()?;
            for await digest in renders.iter() {
                yield digest?;
            }
        })
    }

    fn proxy_path(&self) -> Option<&std::path::Path> {
        self.renders
            .as_ref()
            .map(|render_store| render_store.proxy.root())
    }

    async fn remove_rendered_manifest(&self, digest: crate::encoding::Digest) -> Result<()> {
        let renders = match &self.renders {
            Some(render_store) => &render_store.renders,
            None => return Ok(()),
        };
        let rendered_dirpath = renders.build_digest_path(&digest);
        let workdir = renders.workdir();
        makedirs_with_perms(&workdir, renders.directory_permissions).map_err(|source| {
            Error::StorageWriteError("remove render create workdir", workdir.clone(), source)
        })?;
        OpenFsRepository::remove_dir_atomically(&rendered_dirpath, &workdir).await
    }

    async fn remove_rendered_manifest_if_older_than(
        &self,
        older_than: DateTime<Utc>,
        digest: crate::encoding::Digest,
    ) -> Result<bool> {
        let renders = match &self.renders {
            Some(render_store) => &render_store.renders,
            None => return Ok(false),
        };
        let rendered_dirpath = renders.build_digest_path(&digest);

        let metadata = match tokio::fs::symlink_metadata(&rendered_dirpath).await {
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(err) => {
                return Err(Error::StorageReadError(
                    "symlink_metadata on rendered dir path",
                    rendered_dirpath.clone(),
                    err,
                ));
            }
            Ok(metadata) => metadata,
        };

        let mtime = metadata.modified().map_err(|err| {
            Error::StorageReadError(
                "modified on symlink metadata of rendered dir path",
                rendered_dirpath.clone(),
                err,
            )
        })?;

        if DateTime::<Utc>::from(mtime) >= older_than {
            return Ok(false);
        }

        self.remove_rendered_manifest(digest).await?;
        Ok(true)
    }

    /// Returns a list of the render storage for all the users
    /// with renders found in the repository, if any.
    ///
    /// Returns tuples of (username, `ManifestViewer`).
    fn renders_for_all_users(&self) -> Result<Vec<(String, impl FsRepositoryOps)>> {
        if !self.has_renders() {
            return Ok(Vec::new());
        }

        let mut render_dirs = Vec::new();

        let renders_dir = self.root.join("renders");
        for entry in std::fs::read_dir(&renders_dir).map_err(|err| {
            Error::StorageReadError("read_dir on renders dir", renders_dir.clone(), err)
        })? {
            let entry = entry.map_err(|err| {
                Error::StorageReadError("entry in renders dir", renders_dir.clone(), err)
            })?;

            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            render_dirs.push((
                entry
                    .file_name()
                    .to_str()
                    .expect("filename is valid utf8")
                    .to_string(),
                dir,
            ));
        }

        Ok(render_dirs
            .into_iter()
            .map(|(username, dir)| -> (String, Self) {
                (
                    username,
                    Self {
                        objects: FsHashStore::open_unchecked(self.root.join("objects")),
                        payloads: FsHashStore::open_unchecked(self.root.join("payloads")),
                        renders: self
                            .renders
                            .as_ref()
                            .and_then(|_| RenderStore::for_user(self.root.as_ref(), dir).ok()),
                        root: self.root.clone(),
                        tag_namespace: self.tag_namespace.clone(),
                    },
                )
            })
            .collect())
    }
}

impl Address for OpenFsRepositoryImpl {
    fn address(&self) -> Cow<'_, url::Url> {
        Cow::Owned(url::Url::from_directory_path(self.root()).unwrap())
    }
}

impl std::fmt::Debug for OpenFsRepositoryImpl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("OpenFsRepositoryImpl @ {:?}", self.root()))
    }
}

/// Read the last marked migration version for a repository root path.
///
/// Return None if no `VERSION` file was found, or was empty.
pub async fn read_last_migration_version<P: AsRef<Path>>(
    root: P,
) -> MigrationResult<Option<semver::Version>> {
    let version_file = root.as_ref().join("VERSION");
    let version = match tokio::fs::read_to_string(&version_file).await {
        Ok(version) => version,
        Err(err) => match err.kind() {
            std::io::ErrorKind::NotFound => return Ok(None),
            _ => {
                return Err(MigrationError::ReadError(
                    "read_to_string on last migration version",
                    version_file,
                    err,
                ));
            }
        },
    };

    let version = version.trim();
    if version.is_empty() {
        return Ok(None);
    }
    semver::Version::parse(version)
        .map(Some)
        .map_err(|source| MigrationError::InvalidVersion {
            version: version.to_owned(),
            source,
        })
}

/// Set the last migration version of the repo with the given root directory.
pub async fn set_last_migration<P: AsRef<Path>>(
    root: P,
    version: Option<semver::Version>,
) -> MigrationResult<()> {
    let version = match version {
        Some(v) => v,
        None => semver::Version::parse(crate::VERSION).unwrap(),
    };
    match write_version_file(&root, &version) {
        Ok(r) => Ok(r),
        Err(write_err) => {
            // If the write fails, before giving up, see if by chance the file
            // already exists with the desired contents.
            match read_last_migration_version(&root).await {
                Ok(Some(existing)) if existing == version => Ok(()),
                _ => Err(write_err),
            }
        }
    }
}

fn write_version_file<P: AsRef<Path>>(root: P, version: &semver::Version) -> MigrationResult<()> {
    let mut temp_version_file = tempfile::NamedTempFile::new_in(root.as_ref()).map_err(|err| {
        MigrationError::WriteError(
            "create version file temp file",
            root.as_ref().to_owned(),
            err,
        )
    })?;
    #[cfg(unix)]
    {
        // This file can be read only. It will be replaced by a new file
        // if the contents need to be changed. But for interop with older
        // versions of spfs that need to write to it, enable write.
        temp_version_file
            .as_file()
            .set_permissions(Permissions::from_mode(0o666))
            .map_err(|err| {
                MigrationError::WriteError(
                    "set_permissions on version file temp file",
                    temp_version_file.path().to_owned(),
                    err,
                )
            })?;
    }
    temp_version_file
        .write_all(version.to_string().as_bytes())
        .map_err(|err| {
            MigrationError::WriteError(
                "write_all on version file temp file",
                temp_version_file.path().to_owned(),
                err,
            )
        })?;
    temp_version_file.flush().map_err(|err| {
        MigrationError::WriteError(
            "flush on version file temp file",
            temp_version_file.path().to_owned(),
            err,
        )
    })?;
    let version_file = root.as_ref().join("VERSION");
    temp_version_file.persist(&version_file).map_err(|err| {
        MigrationError::WriteError("persist VERSION file", version_file, err.error)
    })?;
    Ok(())
}
