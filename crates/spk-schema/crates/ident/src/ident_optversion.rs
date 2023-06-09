// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::fmt::Write;
use std::str::FromStr;

use spk_schema_foundation::name::{PkgName, PkgNameBuf};
use spk_schema_foundation::version::Version;

use crate::{parsing, Ident, Result, VersionIdent};

/// Identifies a package name and number version.
pub type OptVersionIdent = Ident<PkgNameBuf, Option<Version>>;

impl OptVersionIdent {
    /// Create a new identifier for the named package and no version.
    pub fn new_none<N: Into<PkgNameBuf>>(name: N) -> Self {
        Self {
            base: name.into(),
            target: Default::default(),
        }
    }

    /// Copy this identifier and add the given version.
    pub fn to_version(&self, version: Version) -> VersionIdent {
        VersionIdent {
            base: self.base.clone(),
            target: version,
        }
    }

    /// Turn this identifier into one for the given version.
    pub fn into_version(self, version: Version) -> VersionIdent {
        VersionIdent {
            base: self.base,
            target: version,
        }
    }
}

macro_rules! opt_version_ident_methods {
    ($Ident:ty $(, .$($access:ident).+)?) => {
        impl $Ident {
            /// The name of the identified package
            pub fn name(&self) -> &PkgName {
                self$(.$($access).+)?.base().as_ref()
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
        }

        impl spk_schema_foundation::spec_ops::Named for $Ident {
            fn name(&self) -> &PkgName {
                self.name()
            }
        }
    };
}

pub(crate) use opt_version_ident_methods;

opt_version_ident_methods!(OptVersionIdent);

impl std::fmt::Display for OptVersionIdent {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.base.fmt(f)?;
        if let Some(version) = &self.target {
            f.write_char('/')?;
            version.fmt(f)
        } else {
            Ok(())
        }
    }
}

impl FromStr for OptVersionIdent {
    type Err = crate::Error;

    /// Parse the given identifier string into this instance.
    fn from_str(source: &str) -> Result<Self> {
        use nom::combinator::all_consuming;

        all_consuming(parsing::opt_version_ident::<nom_supreme::error::ErrorTree<_>>)(source)
            .map(|(_, ident)| ident)
            .map_err(|err| match err {
                nom::Err::Error(e) | nom::Err::Failure(e) => crate::Error::String(e.to_string()),
                nom::Err::Incomplete(_) => unreachable!(),
            })
    }
}

/// Parse a package identifier string.
pub fn parse_optversion_ident<S: AsRef<str>>(source: S) -> Result<OptVersionIdent> {
    Ident::from_str(source.as_ref())
}
