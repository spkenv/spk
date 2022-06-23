// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::fs::Permissions;
use std::io::Write;
use std::os::unix::prelude::PermissionsExt;
use std::path::{Path, PathBuf};

use super::FSHashStore;
use crate::runtime::makedirs_with_perms;
use crate::storage::prelude::*;
use crate::Result;

/// Configuration for an fs repository
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct Config {
    pub path: std::path::PathBuf,
    #[serde(flatten)]
    pub params: Params,
}

#[derive(Clone, Default, Debug, serde::Deserialize, serde::Serialize)]
pub struct Params {
    #[serde(default)]
    pub create: bool,
}

#[async_trait::async_trait]
impl FromUrl for Config {
    async fn from_url(url: &url::Url) -> Result<Self> {
        let params = if let Some(qs) = url.query() {
            serde_qs::from_str(qs).map_err(|err| {
                crate::Error::String(format!("Invalid grpc repo parameters: {:?}", err))
            })?
        } else {
            Params::default()
        };
        Ok(Self {
            path: std::path::PathBuf::from(url.path()),
            params,
        })
    }
}

/// A pure filesystem-based repository of spfs data.
pub struct FSRepository {
    root: PathBuf,
    /// stores the actual file data/payloads of this repo
    pub payloads: FSHashStore,
    /// stores all digraph object data for this repo
    pub objects: FSHashStore,
    /// stores rendered file system layers for use in overlayfs
    pub renders: Option<FSHashStore>,
}

#[async_trait::async_trait]
impl FromConfig for FSRepository {
    type Config = Config;

    async fn from_config(config: Self::Config) -> Result<Self> {
        if config.params.create {
            Self::create(&config.path).await
        } else {
            Self::open(&config.path).await
        }
    }
}

impl FSRepository {
    /// Establish a new filesystem repository
    pub async fn create<P: AsRef<Path>>(root: P) -> Result<Self> {
        makedirs_with_perms(&root, 0o777)?;
        let root = tokio::fs::canonicalize(root.as_ref()).await?;
        makedirs_with_perms(root.join("tags"), 0o777)?;
        makedirs_with_perms(root.join("objects"), 0o777)?;
        makedirs_with_perms(root.join("payloads"), 0o777)?;
        let username = whoami::username();
        makedirs_with_perms(root.join("renders").join(username), 0o777)?;
        set_last_migration(&root, None).await?;
        Self::open(root).await
    }

    // Open a repository over the given directory, which must already
    // exist and be a repository
    pub async fn open<P: AsRef<Path>>(root: P) -> Result<Self> {
        let root = match tokio::fs::canonicalize(&root).await {
            Ok(r) => r,
            Err(err) => {
                return Err(crate::Error::FailedToOpenRepository {
                    reason: root.as_ref().to_string_lossy().to_string(),
                    source: Box::new(err.into()),
                })
            }
        };
        let username = whoami::username();
        let repo = Self {
            objects: FSHashStore::open(root.join("objects"))?,
            payloads: FSHashStore::open(root.join("payloads"))?,
            renders: FSHashStore::open(root.join("renders").join(username)).ok(),
            root: root.clone(),
        };

        let current_version = semver::Version::parse(crate::VERSION).unwrap();
        let repo_version = repo.last_migration().await?;
        if repo_version.major > current_version.major {
            return Err(format!(
                "Repository requires a newer version of spfs [{:?}]: {:?}",
                repo_version, root
            )
            .into());
        }
        if repo_version.major < current_version.major {
            return Err(format!(
                "Repository requires a migration, run `spfs migrate {:?}`",
                repo.address()
            )
            .into());
        }

        Ok(repo)
    }

    pub fn root(&self) -> PathBuf {
        self.root.clone()
    }

    pub async fn last_migration(&self) -> Result<semver::Version> {
        Ok(read_last_migration_version(self.root())
            .await?
            .unwrap_or_else(|| {
                semver::Version::parse(crate::VERSION)
                    .expect("crate::VERSION is a valid semver value")
            }))
    }

    pub async fn set_last_migration(&self, version: semver::Version) -> Result<()> {
        set_last_migration(self.root(), Some(version)).await
    }
}

impl Clone for FSRepository {
    fn clone(&self) -> Self {
        let root = self.root.clone();
        Self {
            objects: FSHashStore::open_unchecked(root.join("objects")),
            payloads: FSHashStore::open_unchecked(root.join("payloads")),
            renders: self
                .renders
                .as_ref()
                .map(|r| FSHashStore::open_unchecked(r.root())),
            root,
        }
    }
}

impl BlobStorage for FSRepository {}
impl ManifestStorage for FSRepository {}
impl LayerStorage for FSRepository {}
impl PlatformStorage for FSRepository {}
impl Repository for FSRepository {
    fn address(&self) -> url::Url {
        url::Url::from_directory_path(self.root()).unwrap()
    }
    fn renders(&self) -> Result<Box<dyn ManifestViewer>> {
        match &self.renders {
            Some(_) => Ok(Box::new(self.clone())),
            None => Err("repository has not been setup for rendering manifests".into()),
        }
    }
}

impl std::fmt::Debug for FSRepository {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("FSRepository @ {:?}", self.root()))
    }
}

/// Read the last marked migration version for a repository root path.
///
/// Return None if no `VERSION` file was found, or was empty.
pub async fn read_last_migration_version<P: AsRef<Path>>(
    root: P,
) -> Result<Option<semver::Version>> {
    let version_file = root.as_ref().join("VERSION");
    let version = match tokio::fs::read_to_string(version_file).await {
        Ok(version) => version,
        Err(err) => match err.kind() {
            std::io::ErrorKind::NotFound => return Ok(None),
            _ => return Err(err.into()),
        },
    };

    let version = version.trim();
    if version.is_empty() {
        return Ok(None);
    }
    match semver::Version::parse(version) {
        Ok(v) => Ok(Some(v)),
        Err(err) => match err {
            semver::SemVerError::ParseError(err) => Err(crate::Error::String(format!(
                "Failed to parse repository version '{version}': {err}",
            ))),
        },
    }
}

/// Set the last migration version of the repo with the given root directory.
pub async fn set_last_migration<P: AsRef<Path>>(
    root: P,
    version: Option<semver::Version>,
) -> Result<()> {
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

fn write_version_file<P: AsRef<Path>>(root: P, version: &semver::Version) -> Result<()> {
    let mut temp_version_file = tempfile::NamedTempFile::new_in(root.as_ref())?;
    // This file can be read only. It will be replaced by a new file
    // if the contents need to be changed. But for interop with older
    // versions of spfs that need to write to it, enable write.
    temp_version_file
        .as_file()
        .set_permissions(Permissions::from_mode(0o666))?;
    temp_version_file.write_all(version.to_string().as_bytes())?;
    temp_version_file.flush()?;
    temp_version_file
        .persist(root.as_ref().join("VERSION"))
        .map_err(|err| crate::Error::String(err.to_string()))?;
    Ok(())
}
