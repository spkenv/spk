// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::{convert::TryFrom, fmt::Write, str::FromStr};

use pyo3::prelude::*;
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

use super::{parse_build, parse_version, validate_name, Build, InvalidNameError, Version};

#[cfg(test)]
#[path = "./ident_test.rs"]
mod ident_test;

/// Ident represents a package identifier.
///
/// The identifier is either a specific package or
/// range of package versions/releases depending on the
/// syntax and context
#[pyclass]
#[derive(Debug, Clone, Hash, PartialEq, Eq, Default)]
pub struct Ident {
    #[pyo3(get)]
    name: String,
    #[pyo3(get, set)]
    pub version: Version,
    #[pyo3(get, set)]
    pub build: Option<Build>,
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

#[pymethods]
impl Ident {
    #[new]
    pub fn newpy(
        name: &str,
        version: Option<Version>,
        build: Option<Build>,
    ) -> crate::Result<Self> {
        let mut ident = Self::new(name)?;
        if let Some(version) = version {
            ident.version = version;
        }
        ident.build = build;
        Ok(ident)
    }

    /// Return true if this identifier is for a source package.
    pub fn is_source(&self) -> bool {
        match &self.build {
            Some(build) => build.is_source(),
            None => false,
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
    pub fn new<S: AsRef<str>>(name: S) -> crate::Result<Self> {
        validate_name(name.as_ref())?;
        Ok(Self {
            name: name.as_ref().to_string(),
            ..Default::default()
        })
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

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

impl TryFrom<&str> for Ident {
    type Error = crate::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::from_str(value)
    }
}

impl FromStr for Ident {
    type Err = crate::Error;

    /// Parse the given identifier string into this instance.
    fn from_str(source: &str) -> crate::Result<Self> {
        let mut parts = source.split('/');
        let name = parts.next().unwrap_or_default();
        let version = parts.next();
        let build = parts.next();

        if parts.next().is_some() {
            return Err(InvalidNameError::new_error(format!(
                "Too many tokens in package identifier, expected at most 2 slashes ('/'): {}",
                source
            )));
        }

        let mut ident = Self::new(name)?;
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
