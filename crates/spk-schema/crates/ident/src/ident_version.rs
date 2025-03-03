// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::fmt::Write;
use std::str::FromStr;

use relative_path::RelativePathBuf;
use spk_schema_foundation::ident_build::Build;
use spk_schema_foundation::ident_ops::{TagPath, TagPathStrategy};
use spk_schema_foundation::name::{PkgName, PkgNameBuf};
use spk_schema_foundation::version::Version;

use crate::{AnyIdent, AsVersionIdent, BuildIdent, Ident, Result, ToAnyIdentWithoutBuild, parsing};

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
    pub fn to_any_ident(&self, build: Option<Build>) -> AnyIdent {
        AnyIdent {
            base: self.clone(),
            target: build,
        }
    }

    /// Turn this identifier into one with an optional build.
    pub fn into_any_ident(self, build: Option<Build>) -> AnyIdent {
        AnyIdent {
            base: self,
            target: build,
        }
    }

    /// Copy this identifier and add the given build.
    pub fn to_build_ident(&self, build: Build) -> BuildIdent {
        BuildIdent {
            base: self.clone(),
            target: build,
        }
    }

    /// Turn this identifier into one for the given build.
    pub fn into_build_ident(self, build: Build) -> BuildIdent {
        BuildIdent {
            base: self,
            target: build,
        }
    }
}

impl AsVersionIdent for VersionIdent {
    fn as_version_ident(&self) -> &VersionIdent {
        self
    }
}

macro_rules! version_ident_methods {
    ($Ident:ty $(, .$($access:ident).+)?) => {
        $crate::ident_optversion::opt_version_ident_methods!($Ident $(, .$($access).+)?);

        impl $Ident {
            /// The version number identified for this package
            pub fn version(&self) -> &Version {
                self$(.$($access).+)?.target()
            }

            /// Set the version number of this package identifier
            pub fn set_version(&mut self, version: Version) {
                self$(.$($access).+)?.target = version;
            }
        }

        impl spk_schema_foundation::spec_ops::HasVersion for $Ident {
            fn version(&self) -> &Version {
                self.version()
            }
        }

        impl spk_schema_foundation::spec_ops::WithVersion for $Ident {
            type Output = Self;

            /// Return a copy of this identifier with the given version number instead
            fn with_version(&self, version: Version) -> Self {
                let mut new = self.clone();
                new$(.$($access).+)?.set_version(version);
                new
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
    fn tag_path<S: TagPathStrategy>(&self) -> RelativePathBuf {
        RelativePathBuf::from(self.name().as_str()).join(self.version().tag_path::<S>())
    }
}

impl ToAnyIdentWithoutBuild for VersionIdent {
    #[inline]
    fn to_any_ident_without_build(&self) -> AnyIdent {
        self.to_any_ident(None)
    }
}

/// Parse a package identifier string.
pub fn parse_version_ident<S: AsRef<str>>(source: S) -> Result<VersionIdent> {
    Ident::from_str(source.as_ref())
}
