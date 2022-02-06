// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::{collections::HashSet, convert::TryFrom, fmt::Write, str::FromStr};

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

use super::{parse_build, parse_version, Build, InvalidNameError, PkgNameBuf, Version};

#[cfg(test)]
#[path = "./ident_test.rs"]
mod ident_test;

/// Parse an identifier from a string.
///
/// This will panic if the identifier is wrong,
/// and should only be used for testing.
///
/// ```
/// # #[macro_use] extern crate spk;
/// # fn main() {
/// ident!("my-pkg/1.0.0");
/// # }
/// ```
#[macro_export]
macro_rules! ident {
    ($ident:literal) => {
        $crate::api::parse_ident($ident).unwrap()
    };
}

#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RepositoryName(String);

/// Ident represents a package identifier.
///
/// The identifier is either a specific package or
/// range of package versions/releases depending on the
/// syntax and context
#[derive(Clone, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub struct Ident {
    repository_name: Option<RepositoryName>,
    pub name: PkgNameBuf,
    pub version: Version,
    pub build: Option<Build>,
}

impl std::fmt::Debug for Ident {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Ident").field(&self.to_string()).finish()
    }
}

impl std::fmt::Display for Ident {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(self.name.as_str())?;
        if let Some(vb) = self.version_and_build() {
            f.write_char('/')?;
            f.write_str(vb.as_str())?;
        }
        Ok(())
    }
}

impl Ident {
    /// Return true if this identifier is for a source package.
    pub fn is_source(&self) -> bool {
        match &self.build {
            Some(build) => build.is_source(),
            None => false,
        }
    }

    /// Return a copy of this identifier with the given version number instead
    pub fn with_version(&self, version: Version) -> Ident {
        Self {
            repository_name: self.repository_name.clone(),
            name: self.name.clone(),
            version,
            build: self.build.clone(),
        }
    }

    /// Set the build component of this package identifier.
    pub fn set_build(&mut self, build: Option<Build>) {
        self.build = build;
    }

    /// Return a copy of this identifier with the given build replaced.
    pub fn with_build(&self, build: Option<Build>) -> Self {
        let mut new = self.clone();
        new.build = build;
        new
    }
}

impl Ident {
    pub fn new(name: PkgNameBuf) -> Self {
        Self {
            repository_name: Default::default(),
            name,
            version: Default::default(),
            build: Default::default(),
        }
    }

    /// A string containing the properly formatted name and version number
    ///
    /// This is the same as [`ToString::to_string`] when the build is None.
    pub fn version_and_build(&self) -> Option<String> {
        match &self.build {
            Some(build) => Some(format!("{}/{}", self.version, build.digest())),
            None => {
                if self.version.is_zero() {
                    None
                } else {
                    Some(self.version.to_string())
                }
            }
        }
    }
}

impl From<PkgNameBuf> for Ident {
    fn from(n: PkgNameBuf) -> Self {
        Self::new(n)
    }
}

impl TryFrom<&str> for Ident {
    type Error = crate::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::from_str(value)
    }
}

impl TryFrom<&String> for Ident {
    type Error = crate::Error;

    fn try_from(value: &String) -> Result<Self, Self::Error> {
        Self::from_str(value.as_str())
    }
}

impl TryFrom<String> for Ident {
    type Error = crate::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::from_str(value.as_str())
    }
}

impl FromStr for Ident {
    type Err = crate::Error;

    /// Parse the given identifier string into this instance.
    fn from_str(source: &str) -> crate::Result<Self> {
        // TODO: this list of possible names should come from reading
        // the config file
        let known_repositories: HashSet<&'static str> =
            ["local", "origin"].iter().cloned().collect();

        let parts = source.split('/').collect::<Vec<_>>();

        let (repository_name, name, version, build) = match parts[..] {
            [] => unreachable!(),
            [name] => (None, name, None, None),
            [repository_name, name] if known_repositories.contains(repository_name) => {
                (Some(repository_name), name, None, None)
            }
            [name, version] => (None, name, Some(version), None),
            [repository_name, name, version] if known_repositories.contains(repository_name) => {
                (Some(repository_name), name, Some(version), None)
            }
            [name, version, build] => (None, name, Some(version), Some(build)),
            [repository_name, name, version, build] => {
                (Some(repository_name), name, Some(version), Some(build))
            }
            [_, _, _, _, ..] => {
                return Err(InvalidNameError::new_error(format!(
                    "Too many tokens in package identifier, expected at most 3 slashes ('/'): {}",
                    source
                )))
            }
        };

        let mut ident = Self::new(name.parse()?);
        if let Some(repository_name) = repository_name {
            ident.repository_name = Some(RepositoryName(repository_name.to_owned()));
        }
        if let Some(version) = version {
            ident.version = parse_version(version)?;
        }
        if let Some(build) = build {
            ident.build = Some(parse_build(build)?);
        }
        Ok(ident)
    }
}

/// Parse a package identifier string.
pub fn parse_ident<S: AsRef<str>>(source: S) -> crate::Result<Ident> {
    Ident::from_str(source.as_ref())
}

impl Serialize for Ident {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
impl<'de> Deserialize<'de> for Ident {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s).map_err(de::Error::custom)
    }
}
