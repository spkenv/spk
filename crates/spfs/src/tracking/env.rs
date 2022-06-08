// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::str::FromStr;

use super::tag::TagSpec;
use crate::encoding;
use crate::{Error, Result};

#[cfg(test)]
#[path = "./env_test.rs"]
mod env_test;

/// The pattern used to split components of an env spec string
pub const ENV_SPEC_SEPARATOR: &str = "+";

/// Specifies an spfs item that can appear in a runtime environment.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum EnvSpecItem {
    TagSpec(TagSpec),
    PartialDigest(encoding::PartialDigest),
    Digest(encoding::Digest),
}

impl EnvSpecItem {
    /// Find the object digest for this item.
    ///
    /// Any necessary lookups are done using the provided repository
    pub async fn resolve_digest<R>(&self, repo: &R) -> Result<encoding::Digest>
    where
        R: crate::storage::Repository + ?Sized,
    {
        match self {
            Self::TagSpec(spec) => repo.resolve_tag(spec).await.map(|t| t.target),
            Self::PartialDigest(part) => repo.resolve_full_digest(part).await,
            Self::Digest(digest) => Ok(*digest),
        }
    }
}

impl std::fmt::Display for EnvSpecItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TagSpec(x) => x.fmt(f),
            Self::PartialDigest(x) => x.fmt(f),
            Self::Digest(x) => x.fmt(f),
        }
    }
}

impl FromStr for EnvSpecItem {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        parse_env_spec_item(s)
    }
}

/// Specifies a complete runtime environment that
/// can be made up of multiple layers.
///
/// The env spec contains an non-empty, ordered set of references
/// that make up this environment.
///
/// It can be easily parsed from a string containing
/// tags and/or digests:
///
/// ```rust
/// use spfs::tracking::EnvSpec;
///
/// let spec = EnvSpec::parse("sometag~1+my-other-tag").unwrap();
/// let items: Vec<_> = spec.iter().map(ToString::to_string).collect();
/// assert_eq!(items, vec!["sometag~1", "my-other-tag"]);
///
/// let spec = EnvSpec::parse("3YDG35SUMJS67N2QPQ4NQCYJ6QGKMEB5H4MHC76VRGMRWBRBLFHA====+my-tag").unwrap();
/// let items: Vec<_> = spec.iter().map(ToString::to_string).collect();
/// assert_eq!(items, vec!["3YDG35SUMJS67N2QPQ4NQCYJ6QGKMEB5H4MHC76VRGMRWBRBLFHA====", "my-tag"]);
/// ```
#[derive(Debug)]
pub struct EnvSpec {
    items: Vec<EnvSpecItem>,
}

impl EnvSpec {
    /// Parse the provided string into an environment spec.
    pub fn parse<S: AsRef<str>>(spec: S) -> Result<Self> {
        Self::from_str(spec.as_ref())
    }
}

impl std::ops::Deref for EnvSpec {
    type Target = Vec<EnvSpecItem>;

    fn deref(&self) -> &Self::Target {
        &self.items
    }
}

impl std::iter::IntoIterator for EnvSpec {
    type Item = EnvSpecItem;

    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}

impl FromStr for EnvSpec {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        Ok(Self {
            items: parse_env_spec_items(s)?,
        })
    }
}

impl From<encoding::Digest> for EnvSpec {
    fn from(digest: encoding::Digest) -> Self {
        EnvSpec {
            items: vec![EnvSpecItem::Digest(digest)],
        }
    }
}

impl std::string::ToString for EnvSpec {
    fn to_string(&self) -> String {
        let items: Vec<_> = self.items.iter().map(|i| i.to_string()).collect();
        items.join(ENV_SPEC_SEPARATOR)
    }
}

/// Return the items identified in an environment spec string.
fn parse_env_spec_items<S: AsRef<str>>(spec: S) -> Result<Vec<EnvSpecItem>> {
    let mut items = Vec::new();
    for layer in spec.as_ref().split(ENV_SPEC_SEPARATOR) {
        items.push(parse_env_spec_item(layer)?);
    }

    if items.is_empty() {
        return Err(Error::new("must specify at least one digest or tag"));
    }

    Ok(items)
}

/// Parse the given string as an single environment spec item.
fn parse_env_spec_item<S: AsRef<str>>(spec: S) -> Result<EnvSpecItem> {
    let spec = spec.as_ref();
    encoding::parse_digest(spec)
        .map(EnvSpecItem::Digest)
        .or_else(|_| encoding::PartialDigest::parse(spec).map(EnvSpecItem::PartialDigest))
        .or_else(|_| TagSpec::parse(spec).map(EnvSpecItem::TagSpec))
}
