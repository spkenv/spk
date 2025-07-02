// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::fmt::Write;
use std::str::FromStr;

use relative_path::RelativePathBuf;
use spk_schema_foundation::ident_build::{Build, EmbeddedSourcePackage};
use spk_schema_foundation::ident_ops::parsing::IdentPartsBuf;
use spk_schema_foundation::ident_ops::{MetadataPath, TagPath};
use spk_schema_foundation::name::{PkgName, PkgNameBuf, RepositoryNameBuf};
use spk_schema_foundation::spec_ops::prelude::*;
use spk_schema_foundation::version::Version;

use crate::ident_version::VersionIdent;
use crate::{
    AnyIdent,
    Error,
    Ident,
    LocatedBuildIdent,
    RangeIdent,
    Result,
    ToAnyIdentWithoutBuild,
    parsing,
};

/// Identifies a specific package name, version and build
pub type BuildIdent = Ident<VersionIdent, Build>;

crate::ident_version::version_ident_methods!(BuildIdent, .base);

impl TryFrom<EmbeddedSourcePackage> for BuildIdent {
    type Error = Error;

    fn try_from(value: EmbeddedSourcePackage) -> std::result::Result<Self, Self::Error> {
        let IdentPartsBuf {
            repository_name: _,
            pkg_name,
            version_str: Some(version),
            build_str: Some(build),
        } = value.ident
        else {
            return if value.ident.build_str.is_some() {
                Err(Error::String(
                    "EmbeddedSourcePackage missing version".to_string(),
                ))
            } else {
                Err(Error::String(
                    "EmbeddedSourcePackage missing build".to_string(),
                ))
            };
        };
        Ok(Self::new(
            VersionIdent::new(pkg_name.try_into()?, version.try_into()?),
            build.try_into()?,
        ))
    }
}

macro_rules! build_ident_methods {
    ($Ident:ty $(, .$($access:ident).+)?) => {
        impl $Ident {
            /// The build id of the identified package
            pub fn build(&self) -> &Build {
                self$(.$($access).+)?.target()
            }

            /// Set the build component of this package identifier
            pub fn set_build(&mut self, build: Build) {
                self$(.$($access).+)?.target = build;
            }

            /// Return a copy of this identifier with the given build instead
            pub fn with_build(&self, build: Build) -> Self {
                let mut new = self.clone();
                new$(.$($access).+)?.set_build(build);
                new
            }

            /// Convert a copy of this identifier into a [`crate::VersionIdent`]
            pub fn to_version_ident(self) -> VersionIdent {
                self$(.$($access).+)?.base().clone()
            }

            /// Convert this identifier into a [`crate::VersionIdent`]
            pub fn into_version_ident(self) -> VersionIdent {
                self$(.$($access).+)?.into_base()
            }

            /// Turn this identifier into an [`AnyIdent`]
            pub fn into_any_ident(self) -> AnyIdent {
                AnyIdent::new(self$(.$($access).+)?.base, Some(self$(.$($access).+)?.target))
            }

            /// Turn a copy of this identifier into an [`AnyIdent`]
            pub fn to_any_ident(&self) -> AnyIdent {
                self$(.$($access).+)?.clone().into_any_ident()
            }

            /// Return if this identifier can possibly have embedded packages
            pub fn can_embed(&self) -> bool {
                // Only true builds can have embeds.
                matches!(self.build(), Build::BuildId(_))
            }

            /// Return true if this identifier is for an embedded package
            pub fn is_embedded(&self) -> bool {
                matches!(self.build(), Build::Embedded(_))
            }

            /// Return true if this identifier is for a source package
            pub fn is_source(&self) -> bool {
                self.build().is_source()
            }
        }

        impl spk_schema_foundation::spec_ops::HasBuild for $Ident {
            fn build(&self) -> &Build {
                self.build()
            }
        }
    };
}

pub(crate) use build_ident_methods;

build_ident_methods!(BuildIdent);

impl BuildIdent {
    /// Convert into a [`LocatedBuildIdent`] with the given [`RepositoryNameBuf`]
    pub fn into_located(self, repository_name: RepositoryNameBuf) -> LocatedBuildIdent {
        LocatedBuildIdent {
            base: repository_name,
            target: self,
        }
    }

    /// Turn a copy of this identifier into a [`LocatedBuildIdent`]
    pub fn to_located(&self, repository_name: RepositoryNameBuf) -> LocatedBuildIdent {
        LocatedBuildIdent {
            base: repository_name,
            target: self.clone(),
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

impl FromStr for BuildIdent {
    type Err = crate::Error;

    /// Parse the given identifier string into this instance.
    fn from_str(source: &str) -> Result<Self> {
        use nom::combinator::all_consuming;

        all_consuming(parsing::build_ident::<nom_supreme::error::ErrorTree<_>>)(source)
            .map(|(_, ident)| ident)
            .map_err(|err| match err {
                nom::Err::Error(e) | nom::Err::Failure(e) => crate::Error::String(e.to_string()),
                nom::Err::Incomplete(_) => unreachable!(),
            })
    }
}

impl MetadataPath for BuildIdent {
    fn metadata_path(&self) -> RelativePathBuf {
        RelativePathBuf::from(self.name().as_str())
            .join(self.version().metadata_path())
            .join(self.build().metadata_path())
    }
}

impl TryFrom<&IdentPartsBuf> for BuildIdent {
    type Error = crate::Error;

    fn try_from(parts: &IdentPartsBuf) -> Result<Self> {
        if parts.repository_name.is_some() {
            return Err("Ident may not have a repository name".into());
        }

        let name = parts.pkg_name.parse::<PkgNameBuf>()?;
        let version = parts
            .version_str
            .as_ref()
            .map(|v| v.parse::<Version>())
            .transpose()?
            .unwrap_or_default();
        let build = parts
            .build_str
            .as_ref()
            .map(|v| v.parse::<Build>())
            .transpose()?
            .ok_or_else(|| Error::String("Ident must have an associated build id".into()))?;

        Ok(VersionIdent::new(name, version).into_build_ident(build))
    }
}

impl TagPath for BuildIdent {
    fn tag_path(&self) -> RelativePathBuf {
        RelativePathBuf::from(self.name().as_str())
            .join(self.version().tag_path())
            .join(self.build().tag_path())
    }

    fn verbatim_tag_path(&self) -> RelativePathBuf {
        RelativePathBuf::from(self.name().as_str())
            .join(self.version().verbatim_tag_path())
            .join(self.build().verbatim_tag_path())
    }
}

impl ToAnyIdentWithoutBuild for BuildIdent {
    #[inline]
    fn to_any_ident_without_build(&self) -> AnyIdent {
        self.to_any_ident()
    }
}

impl From<&BuildIdent> for IdentPartsBuf {
    fn from(ident: &BuildIdent) -> Self {
        IdentPartsBuf {
            repository_name: None,
            pkg_name: ident.name().to_string(),
            version_str: Some(ident.version().to_string()),
            build_str: Some(ident.build().to_string()),
        }
    }
}

impl PartialEq<&BuildIdent> for IdentPartsBuf {
    fn eq(&self, other: &&BuildIdent) -> bool {
        self.repository_name.is_none()
            && self.pkg_name == other.name().as_str()
            && self.version_str == Some(other.version().to_string())
            && self.build_str == Some(other.build().to_string())
    }
}

impl TryFrom<RangeIdent> for BuildIdent {
    type Error = crate::Error;

    fn try_from(ri: RangeIdent) -> Result<Self> {
        let name = ri.name;
        let build = ri
            .build
            .ok_or_else(|| Error::String("Build was required from range ident".into()))?;
        Ok(ri
            .version
            .try_into_version()
            .map(|version| VersionIdent::new(name, version).into_build_ident(build))?)
    }
}

impl TryFrom<&RangeIdent> for BuildIdent {
    type Error = crate::Error;

    fn try_from(ri: &RangeIdent) -> Result<Self> {
        ri.clone().try_into()
    }
}

/// Parse a package identifier string with associated build.
pub fn parse_build_ident<S: AsRef<str>>(source: S) -> Result<BuildIdent> {
    Ident::from_str(source.as_ref())
}
