// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::borrow::Cow;
use std::fmt::Display;
use std::str::FromStr;

use nonempty::{NonEmpty, nonempty};
use serde::Deserialize;

use super::tag::TagSpec;
use crate::tracking::env::EnvLayersFile;
use crate::tracking::{EnvSpec, EnvSpecItem, SpecFile};
use crate::{Error, Result, encoding, graph};

#[cfg(test)]
#[path = "./ref_spec_test.rs"]
mod ref_spec_test;

/// The pattern used to split components of an ref spec string
pub const REF_SPEC_SEPARATOR: &str = "+";

/// Enum of all the spfs ref spec things that can be constructed from
/// filepaths given on the command line.
#[derive(Deserialize, Debug, Clone, Eq, PartialEq)]
pub enum RefSpecFile {
    EnvLayersFile(EnvLayersFile),
}

impl Display for RefSpecFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EnvLayersFile(x) => x.fmt(f),
        }
    }
}

/// Specifies an spfs item.
///
/// This represents something that, e.g., can be `spfs push`ed from one
/// repository to another.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum RefSpecItem {
    TagSpec(TagSpec),
    /// May refer to an object or a payload
    PartialDigest(encoding::PartialDigest),
    /// May refer to an object or a payload
    Digest(encoding::Digest),
    SpecFile(RefSpecFile),
}

impl RefSpecItem {
    /// Find the digest for this ref spec item.
    ///
    /// Any necessary lookups are done using the provided repository.
    ///
    /// It is possible for this to succeed for tags even when no object or
    /// payload exists with the digest.
    ///
    /// The returned digest may refer to an object, a payload, or a non-existent
    /// item.
    pub async fn resolve_digest<R>(&self, repo: &R) -> Result<encoding::Digest>
    where
        R: crate::storage::Repository + ?Sized,
    {
        match self {
            Self::TagSpec(spec) => repo.resolve_tag(spec).await.map(|t| t.target),
            Self::PartialDigest(part) => repo
                .resolve_full_digest(part, graph::PartialDigestType::Unknown)
                .await
                .map(|found_digest| found_digest.into_digest()),
            Self::Digest(digest) => Ok(*digest),
            Self::SpecFile(_) => Err(Error::String(String::from(
                "impossible operation: spfs env files do not have digests",
            ))),
        }
    }

    /// RefSpecItem::TagSpec item variants return a
    /// RefSpecItem::Digest item variant built from the TagSpec's
    /// tag's underlying digest. All other item variants return the
    /// existing item unchanged.
    ///
    /// Any necessary lookups are done using the provided repository
    pub async fn with_tag_resolved<R>(&self, repo: &R) -> Result<Cow<'_, RefSpecItem>>
    where
        R: crate::storage::Repository + ?Sized,
    {
        match self {
            Self::TagSpec(_spec) => Ok(Cow::Owned(RefSpecItem::Digest(
                self.resolve_digest(repo).await?,
            ))),
            _ => Ok(Cow::Borrowed(self)),
        }
    }
}

impl<'de> Deserialize<'de> for RefSpecItem {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;

        RefSpecItem::from_str(&value).map_err(|err| {
            serde::de::Error::custom(format!("deserializing RefSpecItem failed: {err}"))
        })
    }
}

impl std::fmt::Display for RefSpecItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TagSpec(x) => x.fmt(f),
            Self::PartialDigest(x) => x.fmt(f),
            Self::Digest(x) => x.fmt(f),
            Self::SpecFile(x) => x.fmt(f),
        }
    }
}

impl FromStr for RefSpecItem {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        parse_ref_spec_item(s)
    }
}

impl From<TagSpec> for RefSpecItem {
    fn from(item: TagSpec) -> Self {
        Self::TagSpec(item)
    }
}

impl From<encoding::PartialDigest> for RefSpecItem {
    fn from(item: encoding::PartialDigest) -> Self {
        Self::PartialDigest(item)
    }
}

impl From<encoding::Digest> for RefSpecItem {
    fn from(item: encoding::Digest) -> Self {
        Self::Digest(item)
    }
}

impl TryFrom<EnvSpecItem> for RefSpecItem {
    type Error = Error;

    fn try_from(item: EnvSpecItem) -> Result<Self> {
        match item {
            EnvSpecItem::TagSpec(tag_spec) => Ok(RefSpecItem::TagSpec(tag_spec)),
            EnvSpecItem::PartialDigest(partial_digest) => {
                Ok(RefSpecItem::PartialDigest(partial_digest))
            }
            EnvSpecItem::Digest(digest) => Ok(RefSpecItem::Digest(digest)),
            EnvSpecItem::SpecFile(spec_file) => Ok(RefSpecItem::SpecFile(match spec_file {
                SpecFile::EnvLayersFile(layers_file) => RefSpecFile::EnvLayersFile(layers_file),
                SpecFile::LiveLayer(_) => {
                    return Err(Error::String(
                        "cannot convert LiveLayer SpecFile to RefSpecItem".into(),
                    ));
                }
            })),
        }
    }
}

/// Specifies a non-empty collection of spfs references.
///
/// It can be easily parsed from a string containing
/// tags and/or digests:
///
/// ```rust
/// use spfs::tracking::RefSpec;
///
/// let spec = RefSpec::parse("sometag~1+my-other-tag").unwrap();
/// let items: Vec<_> = spec.iter().map(ToString::to_string).collect();
/// assert_eq!(items, vec!["sometag~1", "my-other-tag"]);
///
/// let spec = RefSpec::parse("3YDG35SUMJS67N2QPQ4NQCYJ6QGKMEB5H4MHC76VRGMRWBRBLFHA====+my-tag").unwrap();
/// let items: Vec<_> = spec.iter().map(ToString::to_string).collect();
/// assert_eq!(items, vec!["3YDG35SUMJS67N2QPQ4NQCYJ6QGKMEB5H4MHC76VRGMRWBRBLFHA====", "my-tag"]);
/// ```
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RefSpec {
    items: NonEmpty<RefSpecItem>,
}

impl RefSpec {
    /// Combine multiple RefSpecs into a single RefSpec.
    pub fn combine(ref_specs: &[RefSpec]) -> Result<Self> {
        let Some((head, tail)) = ref_specs.split_first() else {
            return Err(Error::String(
                "at least one RefSpec is required to combine".into(),
            ));
        };
        Ok(tail.iter().cloned().fold(head.clone(), |mut acc, spec| {
            acc.items.extend(spec.into_items());
            acc
        }))
    }

    /// Consume this RefSpec and return its items.
    #[inline]
    pub fn into_items(self) -> NonEmpty<RefSpecItem> {
        self.items
    }

    /// Parse the provided string into an ref spec.
    pub fn parse<S: AsRef<str>>(spec: S) -> Result<Self> {
        Self::from_str(spec.as_ref())
    }

    /// TagSpec items are turned into Digest items using the digest
    /// resolved from the tag. All other items are returned as is.
    /// This will error when trying to resolve a tag that is not in
    /// any of the repos. The repos are searched in order for the tag,
    /// and first repo with the tag is used.
    pub async fn resolve_tag_item_to_digest_item<R>(
        &self,
        item: &RefSpecItem,
        repos: &Vec<&R>,
    ) -> Result<RefSpecItem>
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

    /// Create a RefSpec from an iterator of RefSpecItems.
    pub fn try_from_iter<I, R>(iter: I) -> Result<Self>
    where
        I: IntoIterator<Item = R>,
        R: Into<RefSpecItem>,
    {
        let items: Vec<RefSpecItem> = iter.into_iter().map(Into::into).collect();
        Ok(RefSpec {
            items: NonEmpty::from_vec(items)
                .ok_or_else(|| Error::String("a ref spec may not be empty".into()))?,
        })
    }

    /// Return a new RefSpec based on this one, with all the tag items
    /// converted to digest items using the tags' underlying digests.
    pub async fn with_tag_items_resolved_to_digest_items<R>(
        &self,
        repos: &Vec<&R>,
    ) -> Result<RefSpec>
    where
        R: crate::storage::Repository + ?Sized,
    {
        let mut new_items: Vec<RefSpecItem> = Vec::with_capacity(self.items.len());
        for item in &self.items {
            // Filter out the LiveLayers entirely because they do not have digests
            if let RefSpecItem::SpecFile(_) = item {
                continue;
            }
            new_items.push(self.resolve_tag_item_to_digest_item(item, repos).await?);
        }

        Ok(RefSpec {
            items: NonEmpty::from_vec(new_items).ok_or_else(|| {
                Error::String("impossible: empty RefSpec after tag resolution".into())
            })?,
        })
    }
}

impl std::ops::Deref for RefSpec {
    type Target = NonEmpty<RefSpecItem>;

    fn deref(&self) -> &Self::Target {
        &self.items
    }
}

impl std::ops::DerefMut for RefSpec {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.items
    }
}

impl std::iter::IntoIterator for RefSpec {
    type Item = RefSpecItem;

    type IntoIter = <NonEmpty<Self::Item> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}

impl FromStr for RefSpec {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        Ok(Self {
            items: parse_ref_spec_items(s)?,
        })
    }
}

impl TryFrom<EnvSpec> for RefSpec {
    type Error = Error;

    fn try_from(env_spec: EnvSpec) -> Result<Self> {
        let env_spec_items = env_spec.into_items();
        let mut items: Vec<RefSpecItem> = Vec::with_capacity(env_spec_items.len());
        for env_item in env_spec_items {
            items.push(env_item.try_into()?);
        }
        Ok(RefSpec {
            items: NonEmpty::from_vec(items)
                .ok_or_else(|| Error::String("a ref spec may not be empty".into()))?,
        })
    }
}

impl<I> From<I> for RefSpec
where
    I: Into<RefSpecItem>,
{
    fn from(item: I) -> Self {
        RefSpec {
            items: nonempty![item.into()],
        }
    }
}

impl<I> std::iter::Extend<I> for RefSpec
where
    I: Into<RefSpecItem>,
{
    fn extend<T: IntoIterator<Item = I>>(&mut self, iter: T) {
        self.items.extend(iter.into_iter().map(Into::into))
    }
}

impl std::fmt::Display for RefSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let items: Vec<_> = self.items.iter().map(|i| i.to_string()).collect();
        write!(f, "{}", items.join(REF_SPEC_SEPARATOR))
    }
}

/// Return the items identified in an ref spec string.
fn parse_ref_spec_items<S: AsRef<str>>(spec: S) -> Result<NonEmpty<RefSpecItem>> {
    let mut items = Vec::new();
    for layer in spec.as_ref().split(REF_SPEC_SEPARATOR) {
        let item = parse_ref_spec_item(layer)?;
        // Env list of layers files are immediately expanded into the
        // RefSpec's items list. Other items are just added as is.
        if let RefSpecItem::SpecFile(RefSpecFile::EnvLayersFile(layers)) = item {
            items.extend(
                layers
                    .flatten()?
                    .into_iter()
                    .map(TryInto::try_into)
                    .collect::<Result<Vec<RefSpecItem>>>()?,
            );
        } else {
            items.push(item);
        }
    }
    NonEmpty::from_vec(items)
        .ok_or_else(|| Error::String("RefSpec must contain at least one valid RefSpecItem".into()))
}

/// Parse the given string as an single ref spec item.
fn parse_ref_spec_item<S: AsRef<str>>(spec: S) -> Result<RefSpecItem> {
    let spec = spec.as_ref();
    if spec.is_empty() || spec == crate::tracking::ENV_SPEC_EMPTY {
        return Err(Error::String("ref spec item may not be empty".into()));
    }
    encoding::parse_digest(spec)
        .map(RefSpecItem::Digest)
        .or_else(|err| {
            tracing::debug!("Unable to parse as a Digest: {err}");
            encoding::PartialDigest::parse(spec).map(RefSpecItem::PartialDigest)
        })
        .or_else(|err| {
            tracing::debug!("Unable to parse as a Partial Digest: {err}");
            SpecFile::parse(spec).and_then(|spec_file| {
                Ok(RefSpecItem::SpecFile(match spec_file {
                    SpecFile::EnvLayersFile(layers_file) => RefSpecFile::EnvLayersFile(layers_file),
                    SpecFile::LiveLayer(_) => {
                        return Err(Error::String(
                            "cannot use LiveLayer spec files in RefSpec".into(),
                        ));
                    }
                }))
            })
        })
        .or_else(|err| {
            tracing::debug!("Unable to parse as a RefSpecFile: {err}");
            // A duplicate spec file reference error while parsing a
            // spfs spec file means its filepath had already been read
            // in. Reading it in again would generate an infinite
            // parsing loop, so this should error out now.
            if let Error::DuplicateSpecFileReference(ref _filepath) = err {
                return Err(err);
            }

            TagSpec::parse(spec).map(RefSpecItem::TagSpec)
        })
}
