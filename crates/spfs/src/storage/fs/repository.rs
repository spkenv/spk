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
use std::sync::Arc;

use arc_swap::ArcSwap;

use super::hash_store::PROXY_DIRNAME;
use super::migrations::{MigrationError, MigrationResult};
use super::FsHashStore;
use crate::config::{pathbuf_deserialize_with_tilde_expansion, ToAddress};
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
        #[cfg(windows)]
        // on windows, a path with a drive letter may get prefixed with another
        // root forward slash, which is not appropriate for the platform
        let path = std::path::PathBuf::from(url.path().trim_start_matches('/'));
        #[cfg(unix)]
        let path = std::path::PathBuf::from(url.path());
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

/// A pure filesystem-based repository of spfs data.
///
/// This instance can be already validated and open or
/// lazily evaluated on the each request until successful.
///
/// An [`OpenFsRepository`] is more useful than this one, but
/// can also be easily retrieved via the [`Self::opened`].
#[derive(Clone)]
pub struct FsRepository(Arc<ArcSwap<InnerFsRepository>>);

enum InnerFsRepository {
    Closed(Config),
    Open(Arc<OpenFsRepository>),
}

impl From<OpenFsRepository> for FsRepository {
    fn from(value: OpenFsRepository) -> Self {
        Arc::new(value).into()
    }
}

impl From<Arc<OpenFsRepository>> for FsRepository {
    fn from(value: Arc<OpenFsRepository>) -> Self {
        Self(Arc::new(ArcSwap::new(Arc::new(InnerFsRepository::Open(
            value,
        )))))
    }
}

#[async_trait::async_trait]
impl FromConfig for FsRepository {
    type Config = Config;

    async fn from_config(config: Self::Config) -> crate::storage::OpenRepositoryResult<Self> {
        if config.params.lazy {
            Ok(Self(Arc::new(ArcSwap::new(Arc::new(
                InnerFsRepository::Closed(config),
            )))))
        } else {
            Ok(OpenFsRepository::from_config(config).await?.into())
        }
    }
}

impl FsRepository {
    /// Open a filesystem repository, creating it if necessary
    pub async fn create<P: AsRef<Path>>(root: P) -> OpenRepositoryResult<Self> {
        Ok(Self(Arc::new(ArcSwap::new(Arc::new(
            InnerFsRepository::Open(Arc::new(OpenFsRepository::create(root).await?)),
        )))))
    }

    // Open a repository over the given directory, which must already
    // exist and be properly setup as a repository
    pub async fn open<P: AsRef<Path>>(root: P) -> OpenRepositoryResult<Self> {
        let root = root.as_ref();
        Ok(Self(Arc::new(ArcSwap::new(Arc::new(
            InnerFsRepository::Open(Arc::new(OpenFsRepository::open(&root).await?)),
        )))))
    }

    /// Get the opened version of this repository, performing
    /// any required opening and validation as needed
    pub fn opened(&self) -> impl futures::Future<Output = Result<Arc<OpenFsRepository>>> + 'static {
        self.opened_and_map_err(Error::failed_to_open_repository)
    }

    /// Get the opened version of this repository, performing
    /// any required opening and validation as needed
    pub fn try_open(
        &self,
    ) -> impl futures::Future<Output = OpenRepositoryResult<Arc<OpenFsRepository>>> + 'static {
        self.opened_and_map_err(|_, e| e)
    }

    fn opened_and_map_err<F, E>(
        &self,
        map: F,
    ) -> impl futures::Future<Output = std::result::Result<Arc<OpenFsRepository>, E>> + 'static
    where
        F: FnOnce(&Self, OpenRepositoryError) -> E + 'static,
    {
        let inner = Arc::clone(&self.0);
        async move {
            match &**inner.load() {
                InnerFsRepository::Closed(config) => {
                    let config = config.clone();
                    let opened = match OpenFsRepository::from_config(config).await {
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

impl Address for FsRepository {
    fn address(&self) -> Cow<'_, url::Url> {
        Cow::Owned(url::Url::from_directory_path(self.root()).unwrap())
    }
}

impl std::fmt::Debug for FsRepository {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("FsRepository @ {:?}", self.root()))
    }
}

/// A validated and opened fs repository.
pub struct OpenFsRepository {
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
impl FromConfig for OpenFsRepository {
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

impl Clone for OpenFsRepository {
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

impl LocalRepository for OpenFsRepository {
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

impl OpenFsRepository {
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

    /// The filesystem root path of this repository
    pub fn root(&self) -> PathBuf {
        self.root.clone()
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

    /// Return the configured tag namespace, if any.
    #[inline]
    pub fn get_tag_namespace(&self) -> Option<Cow<'_, TagNamespace>> {
        self.tag_namespace.as_deref().map(Cow::Borrowed)
    }

    /// Set the configured tag namespace, returning the old tag namespace,
    /// if there was one.
    pub fn set_tag_namespace(
        &mut self,
        tag_namespace: Option<TagNamespaceBuf>,
    ) -> Option<TagNamespaceBuf> {
        std::mem::replace(&mut self.tag_namespace, tag_namespace)
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

    /// The latest repository version that this was migrated to.
    pub async fn last_migration(&self) -> MigrationResult<semver::Version> {
        Ok(read_last_migration_version(self.root())
            .await?
            .unwrap_or_else(|| {
                semver::Version::parse(crate::VERSION)
                    .expect("crate::VERSION is a valid semver value")
            }))
    }

    /// Sets the latest version of this repository.
    ///
    /// Should only be modified once a migration has completed successfully.
    pub async fn set_last_migration(&self, version: semver::Version) -> MigrationResult<()> {
        set_last_migration(self.root(), Some(version)).await
    }

    /// True if this repo is setup to generate local manifest renders.
    pub fn has_renders(&self) -> bool {
        self.renders.is_some()
    }

    /// Returns a list of the render storage for all the users
    /// with renders found in the repository, if any.
    ///
    /// Returns tuples of (username, `ManifestViewer`).
    pub fn renders_for_all_users(&self) -> Result<Vec<(String, Self)>> {
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

impl Address for OpenFsRepository {
    fn address(&self) -> Cow<'_, url::Url> {
        Cow::Owned(url::Url::from_directory_path(self.root()).unwrap())
    }
}

impl std::fmt::Debug for OpenFsRepository {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("OpenFsRepository @ {:?}", self.root()))
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
                ))
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
