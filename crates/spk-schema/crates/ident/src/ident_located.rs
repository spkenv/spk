// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use relative_path::RelativePathBuf;
use spk_schema_foundation::ident_build::Build;
use spk_schema_foundation::ident_ops::{MetadataPath, TagPath};
use spk_schema_foundation::name::{PkgName, RepositoryName, RepositoryNameBuf};
use spk_schema_foundation::version::Version;

use crate::ident_build::BuildIdent;
use crate::{Ident, VersionIdent};

/// Identifies a specific package version in a named repository.
pub type LocatedVersionIdent = Ident<RepositoryNameBuf, VersionIdent>;
/// Identifies a specific package build in a named repository.
pub type LocatedBuildIdent = Ident<RepositoryNameBuf, BuildIdent>;

impl LocatedBuildIdent {
    /// The name of the repository identified for this package
    pub fn repository(&self) -> &RepositoryName {
        self.base().as_ref()
    }

    /// The name of the identified package.
    pub fn name(&self) -> &PkgName {
        self.target().name()
    }

    /// The version number identified for this package
    pub fn version(&self) -> &Version {
        self.target().version()
    }

    // The build id identified for this package
    pub fn build(&self) -> &Build {
        self.target().build()
    }

    /// Return true if this identifier is for a source package.
    pub fn is_source(&self) -> bool {
        self.build().is_source()
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
