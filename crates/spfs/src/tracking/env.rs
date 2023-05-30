// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::borrow::Cow;
use std::str::FromStr;

use super::tag::TagSpec;
use crate::runtime::{LiveLayer, LiveLayerFile};
use crate::{encoding, Error, Result};

#[cfg(test)]
#[path = "./env_test.rs"]
mod env_test;

/// The pattern used to split components of an env spec string
pub const ENV_SPEC_SEPARATOR: &str = "+";

/// Recognized as an empty environment spec with no items
pub const ENV_SPEC_EMPTY: &str = "-";

/// Specifies an spfs item that can appear in a runtime environment.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum EnvSpecItem {
    TagSpec(TagSpec),
    PartialDigest(encoding::PartialDigest),
    Digest(encoding::Digest),
    LiveLayerFile(LiveLayerFile),
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
            Self::LiveLayerFile(_) => Err(Error::String(String::from(
                "Impossible operation: live layers do not have digests",
            ))),
        }
    }

    /// EnvSpecItem::TagSpec item variants return a
    /// EnvSpecItem::Digest item variant built from the TagSpec's
    /// tag's underlying digest. All other item variants return the
    /// existing item unchanged.
    ///
    /// Any necessary lookups are done using the provided repository
    pub async fn with_tag_resolved<R>(&self, repo: &R) -> Result<Cow<'_, EnvSpecItem>>
    where
        R: crate::storage::Repository + ?Sized,
    {
        match self {
            Self::TagSpec(_spec) => Ok(Cow::Owned(EnvSpecItem::Digest(
                self.resolve_digest(repo).await?,
            ))),
            _ => Ok(Cow::Borrowed(self)),
        }
    }
}

impl std::fmt::Display for EnvSpecItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TagSpec(x) => x.fmt(f),
            Self::PartialDigest(x) => x.fmt(f),
            Self::Digest(x) => x.fmt(f),
            Self::LiveLayerFile(x) => x.fmt(f),
        }
    }
}

impl FromStr for EnvSpecItem {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        parse_env_spec_item(s)
    }
}

impl From<TagSpec> for EnvSpecItem {
    fn from(item: TagSpec) -> Self {
        Self::TagSpec(item)
    }
}

impl From<encoding::PartialDigest> for EnvSpecItem {
    fn from(item: encoding::PartialDigest) -> Self {
        Self::PartialDigest(item)
    }
}

impl From<encoding::Digest> for EnvSpecItem {
    fn from(item: encoding::Digest) -> Self {
        Self::Digest(item)
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
///
/// let spec = EnvSpec::parse("").unwrap();
/// let items: Vec<_> = spec.iter().map(ToString::to_string).collect();
/// assert!(items.is_empty());
/// ```
#[derive(Debug, Default, Clone, Eq, PartialEq)]
pub struct EnvSpec {
    items: Vec<EnvSpecItem>,
}

impl EnvSpec {
    /// Parse the provided string into an environment spec.
    pub fn parse<S: AsRef<str>>(spec: S) -> Result<Self> {
        Self::from_str(spec.as_ref())
    }

    /// True if there are no items in this spec
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Return a list of the live layers filtered out from the spec
    pub fn load_live_layers(&self) -> Result<Vec<LiveLayer>> {
        let mut live_layers = Vec::new();
        for item in self.items.iter() {
            if let EnvSpecItem::LiveLayerFile(llf) = item {
                live_layers.push(llf.load()?);
            }
        }
        Ok(live_layers)
    }

    /// TagSpec items are turned into Digest items using the digest
    /// resolved from the tag. All other items are returned as is.
    /// This will error when trying to resolve a tag that is not in
    /// any of the repos. The repos are searched in order for the tag,
    /// and first repo with the tag is used.
    pub async fn resolve_tag_item_to_digest_item<R>(
        &self,
        item: &EnvSpecItem,
        repos: &Vec<&R>,
    ) -> Result<EnvSpecItem>
    where
        R: crate::storage::Repository + ?Sized,
    {
        for repo in repos {
            match item.with_tag_resolved(*repo).await {
                Ok(resolved_item) => return Ok(resolved_item.into_owned()),
                Err(err) => {
                    tracing::debug!("{err}")
                }
            }
        }

        Err(Error::UnknownReference(item.to_string()))
    }

    /// Return a new EnvSpec based on this one, with all the tag items
    /// converted to digest items using the tags' underlying digests.
    pub async fn with_tag_items_resolved_to_digest_items<R>(
        &self,
        repos: &Vec<&R>,
    ) -> Result<EnvSpec>
    where
        R: crate::storage::Repository + ?Sized,
    {
        let mut new_items: Vec<EnvSpecItem> = Vec::with_capacity(self.items.len());

        for item in &self.items {
            // Filter out the LiveLayers entirely because they do not have digests
            if let EnvSpecItem::LiveLayerFile(_) = item {
                continue;
            }
            new_items.push(self.resolve_tag_item_to_digest_item(item, repos).await?);
        }

        Ok(EnvSpec { items: new_items })
    }
}

impl std::ops::Deref for EnvSpec {
    type Target = Vec<EnvSpecItem>;

    fn deref(&self) -> &Self::Target {
        &self.items
    }
}

impl std::ops::DerefMut for EnvSpec {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.items
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

impl<I> From<I> for EnvSpec
where
    I: Into<EnvSpecItem>,
{
    fn from(item: I) -> Self {
        EnvSpec {
            items: vec![item.into()],
        }
    }
}

impl<I: Into<EnvSpecItem>> std::iter::FromIterator<I> for EnvSpec {
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = I>,
    {
        Self {
            items: iter.into_iter().map(Into::into).collect(),
        }
    }
}

impl std::iter::FromIterator<EnvSpec> for EnvSpec {
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = EnvSpec>,
    {
        Self {
            items: iter.into_iter().flat_map(|e| e.into_iter()).collect(),
        }
    }
}

impl<I: Into<EnvSpecItem>> From<Vec<I>> for EnvSpec {
    fn from(value: Vec<I>) -> Self {
        Self::from_iter(value)
    }
}

impl<I> std::iter::Extend<I> for EnvSpec
where
    I: Into<EnvSpecItem>,
{
    fn extend<T: IntoIterator<Item = I>>(&mut self, iter: T) {
        self.items.extend(iter.into_iter().map(Into::into))
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
    if spec.as_ref() == ENV_SPEC_EMPTY || spec.as_ref() == "" {
        return Ok(Vec::new());
    }
    let mut items = Vec::new();
    for layer in spec.as_ref().split(ENV_SPEC_SEPARATOR) {
        items.push(parse_env_spec_item(layer)?);
    }

    Ok(items)
}

/// Parse the given string as an single environment spec item.
fn parse_env_spec_item<S: AsRef<str>>(spec: S) -> Result<EnvSpecItem> {
    let spec = spec.as_ref();
    encoding::parse_digest(spec)
        .map(EnvSpecItem::Digest)
        .or_else(|_| encoding::PartialDigest::parse(spec).map(EnvSpecItem::PartialDigest))
        .or_else(|_| LiveLayerFile::parse(spec).map(EnvSpecItem::LiveLayerFile))
        .or_else(|_| TagSpec::parse(spec).map(EnvSpecItem::TagSpec))
}
