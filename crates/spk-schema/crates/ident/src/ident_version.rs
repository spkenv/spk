// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::fmt::Write;

use spk_schema_foundation::ident_build::Build;
use spk_schema_foundation::name::{PkgName, PkgNameBuf};
use spk_schema_foundation::version::Version;

use crate::{AnyIdent, BuildIdent, Ident};

/// Identifies a package version and build.
pub type VersionIdent = Ident<PkgNameBuf, Version>;

impl VersionIdent {
    /// Create a new identifier for the named package and version 0.0.0
    pub fn new_zero<N: Into<PkgNameBuf>>(name: N) -> Self {
        Self {
            base: name.into(),
            target: Default::default(),
        }
    }

    /// The name of the identified package.
    pub fn name(&self) -> &PkgName {
        self.base().as_ref()
    }

    /// The version number identified for this package
    pub fn version(&self) -> &Version {
        self.target()
    }

    /// Turn this identifier into one with an optional build.
    pub fn into_any(self, build: Option<Build>) -> AnyIdent {
        AnyIdent {
            base: self,
            target: build,
        }
    }

    /// Turn this identifier into one for the given build.
    pub fn into_build(self, build: Build) -> BuildIdent {
        BuildIdent {
            base: self,
            target: build,
        }
    }

    /// Return a copy of this identifier with the given version number instead
    pub fn with_version(&self, version: Version) -> Self {
        Self {
            base: self.base.clone(),
            target: version,
        }
    }
}

impl std::fmt::Display for VersionIdent {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.base.fmt(f)?;
        f.write_char('/')?;
        self.target.fmt(f)
    }
}
