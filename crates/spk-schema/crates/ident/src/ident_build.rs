// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::fmt::Write;

use spk_schema_foundation::ident_build::Build;
use spk_schema_foundation::name::{PkgName, RepositoryNameBuf};
use spk_schema_foundation::version::Version;

use crate::ident_version::VersionIdent;
use crate::{Ident, LocatedBuildIdent};

/// Identifies a specific package version and build.
pub type BuildIdent = Ident<VersionIdent, Build>;

impl BuildIdent {
    /// The name of the identified package.
    pub fn name(&self) -> &PkgName {
        self.base().name()
    }

    /// The version number identified for this package
    pub fn version(&self) -> &Version {
        self.base().version()
    }

    // The build id identified for this package
    pub fn build(&self) -> &Build {
        self.target()
    }

    /// Return if this identifier can possibly have embedded packages.
    pub fn can_embed(&self) -> bool {
        // Only builds can have embeds.
        matches!(self.build(), Build::Digest(_))
    }

    /// Return true if this identifier is for an embedded package.
    pub fn is_embedded(&self) -> bool {
        matches!(self.build(), Build::Embedded(_))
    }

    /// Return true if this identifier is for a source package.
    pub fn is_source(&self) -> bool {
        self.build().is_source()
    }

    /// Return a copy of this identifier with the given version number instead
    pub fn with_version(&self, version: Version) -> Self {
        Self::new(self.base().with_version(version), self.target().clone())
    }

    /// Set the build component of this package identifier.
    pub fn set_build(&mut self, build: Build) {
        self.target = build;
    }

    /// Return a copy of this identifier with the given build replaced.
    pub fn with_build(&self, build: Build) -> Self {
        let mut new = self.clone();
        new.set_build(build);
        new
    }

    /// Convert into a [`LocatedBuildIdent`] with the given [`RepositoryNameBuf`].
    pub fn into_located(self, repository_name: RepositoryNameBuf) -> LocatedBuildIdent {
        LocatedBuildIdent {
            base: repository_name,
            target: self,
        }
    }
}

impl std::fmt::Display for BuildIdent {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.base.fmt(f)?;
        f.write_char('/')?;
        self.target.fmt(f)
    }
}
