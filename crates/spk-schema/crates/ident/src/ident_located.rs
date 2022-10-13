// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use relative_path::RelativePathBuf;
use spk_schema_foundation::ident_build::Build;
use spk_schema_foundation::ident_ops::{MetadataPath, TagPath};
use spk_schema_foundation::name::{PkgName, PkgNameBuf, RepositoryName, RepositoryNameBuf};
use spk_schema_foundation::spec_ops::HasLocation;
use spk_schema_foundation::version::Version;

use crate::ident_build::BuildIdent;
use crate::{AnyIdent, Ident, VersionIdent};

/// Identifies a specific package version in a named repository.
pub type LocatedVersionIdent = Ident<RepositoryNameBuf, VersionIdent>;

/// Identifies a specific package build in a named repository.
pub type LocatedBuildIdent = Ident<RepositoryNameBuf, BuildIdent>;

crate::ident_version::version_ident_methods!(LocatedVersionIdent, .target);
crate::ident_version::version_ident_methods!(LocatedBuildIdent, .target.base);
crate::ident_build::build_ident_methods!(LocatedBuildIdent, .target);

impl<T> Ident<RepositoryNameBuf, T> {
    /// The name of the identified repository
    pub fn repository_name(&self) -> &RepositoryName {
        self.base().as_ref()
    }

    /// Set the name of the associated repository
    pub fn set_repository_name(&mut self, repo: RepositoryNameBuf) {
        self.base = repo;
    }
}

impl<T> Ident<RepositoryNameBuf, T>
where
    T: Clone,
{
    /// Return a copy of this identifier with the given repository instead
    pub fn with_repository_name(&self, repo: RepositoryNameBuf) -> Self {
        self.with_base(repo)
    }
}

impl LocatedBuildIdent {
    /// Reinterpret this identifier as a [`BuildIdent`]
    pub fn as_build(&self) -> &BuildIdent {
        self.target()
    }

    /// Reinterpret this identifier as a [`BuildIdent`]
    pub fn into_build(self) -> BuildIdent {
        self.into_target()
    }
}

impl<T> HasLocation for Ident<RepositoryNameBuf, T> {
    fn location(&self) -> &RepositoryName {
        self.repository_name()
    }
}

impl MetadataPath for LocatedBuildIdent {
    fn metadata_path(&self) -> RelativePathBuf {
        // The data path *does not* include the repository name.
        RelativePathBuf::from(self.name().as_str())
            .join(self.version().metadata_path())
            .join(self.build().metadata_path())
    }
}

impl TagPath for LocatedBuildIdent {
    fn tag_path(&self) -> RelativePathBuf {
        // The data path *does not* include the repository name.
        RelativePathBuf::from(self.name().as_str())
            .join(self.version().tag_path())
            .join(self.build().tag_path())
    }
}
