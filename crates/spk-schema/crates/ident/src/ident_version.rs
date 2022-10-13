// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::fmt::Write;
use std::str::FromStr;

use relative_path::RelativePathBuf;
use spk_schema_foundation::ident_build::Build;
use spk_schema_foundation::ident_ops::TagPath;
use spk_schema_foundation::name::{PkgName, PkgNameBuf};
use spk_schema_foundation::version::Version;

use crate::{parsing, AnyIdent, BuildIdent, Ident, Result};

/// Identifies a package name and number version.
pub type VersionIdent = Ident<PkgNameBuf, Version>;

impl VersionIdent {
    /// Create a new identifier for the named package and version 0.0.0
    pub fn new_zero<N: Into<PkgNameBuf>>(name: N) -> Self {
        Self {
            base: name.into(),
            target: Default::default(),
        }
    }

    /// Copy this identifier and add the given build.
    pub fn to_any(&self, build: Option<Build>) -> AnyIdent {
        AnyIdent {
            base: self.clone(),
            target: build,
        }
    }

    /// Turn this identifier into one with an optional build.
    pub fn into_any(self, build: Option<Build>) -> AnyIdent {
        AnyIdent {
            base: self,
            target: build,
        }
    }

    /// Copy this identifier and add the given build.
    pub fn to_build(&self, build: Build) -> BuildIdent {
        BuildIdent {
            base: self.clone(),
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
}

macro_rules! version_ident_methods {
    ($Ident:ty $(, .$($access:ident).+)?) => {
        impl $Ident {
            /// The name of the identified package
            pub fn name(&self) -> &PkgName {
                self$(.$($access).+)?.base().as_ref()
            }

            /// The version number identified for this package
            pub fn version(&self) -> &Version {
                self$(.$($access).+)?.target()
            }

            /// Set the package name of this package identifier
            pub fn set_name<T: Into<PkgNameBuf>>(&mut self, name: T) {
                self$(.$($access).+)?.base = name.into();
            }

            /// Return a copy of this identifier with the given name instead
            pub fn with_name<T: Into<PkgNameBuf>>(&self, name: T) -> Self {
                let mut new = self.clone();
                new$(.$($access).+)?.set_name(name);
                new
            }

            /// Set the version number of this package identifier
            pub fn set_version(&mut self, version: Version) {
                self$(.$($access).+)?.target = version;
            }

            /// Return a copy of this identifier with the given version number instead
            pub fn with_version(&self, version: Version) -> Self {
                let mut new = self.clone();
                new$(.$($access).+)?.set_version(version);
                new
            }
        }

        impl spk_schema_foundation::spec_ops::Named for $Ident {
            fn name(&self) -> &PkgName {
                self.name()
            }
        }

        impl spk_schema_foundation::spec_ops::HasVersion for $Ident {
            fn version(&self) -> &Version {
                self.version()
            }
        }
    };
}

pub(crate) use version_ident_methods;

version_ident_methods!(VersionIdent);

impl std::fmt::Display for VersionIdent {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.base.fmt(f)?;
        f.write_char('/')?;
        self.target.fmt(f)
    }
}

impl FromStr for VersionIdent {
    type Err = crate::Error;

    /// Parse the given identifier string into this instance.
    fn from_str(source: &str) -> Result<Self> {
        use nom::combinator::all_consuming;

        all_consuming(parsing::version_ident::<nom_supreme::error::ErrorTree<_>>)(source)
            .map(|(_, ident)| ident)
            .map_err(|err| match err {
                nom::Err::Error(e) | nom::Err::Failure(e) => crate::Error::String(e.to_string()),
                nom::Err::Incomplete(_) => unreachable!(),
            })
    }
}

impl TagPath for VersionIdent {
    fn tag_path(&self) -> RelativePathBuf {
        RelativePathBuf::from(self.name().as_str()).join(self.version().tag_path())
    }
}

/// Parse a package identifier string.
pub fn parse_version_ident<S: AsRef<str>>(source: S) -> Result<VersionIdent> {
    Ident::from_str(source.as_ref())
}
