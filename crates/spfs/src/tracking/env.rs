// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::borrow::Cow;
use std::collections::HashSet;
use std::fmt::Display;
use std::io::Read;
use std::path::PathBuf;
use std::str::FromStr;

use once_cell::sync::Lazy;
use serde::Deserialize;

use super::tag::TagSpec;
use crate::runtime::{LiveLayer, SpecApiVersion};
use crate::{encoding, Error, Result};

#[cfg(test)]
#[path = "./env_test.rs"]
mod env_test;

/// The pattern used to split components of an env spec string
pub const ENV_SPEC_SEPARATOR: &str = "+";

/// Recognized as an empty environment spec with no items
pub const ENV_SPEC_EMPTY: &str = "-";

// For the config file that contains the extra bind mounts
const SPFS_FILE_SUFFIX_YAML: &str = ".spfs.yaml";
const DEFAULT_LIVE_LAYER_FILENAME: &str = "layer.spfs.yaml";

// For preventing recursive loops when reading list of layers spfs
// spec files from the command line
static SEEN_SPEC_FILES: Lazy<std::sync::Mutex<HashSet<PathBuf>>> =
    Lazy::new(|| std::sync::Mutex::new(HashSet::new()));

/// For clearing the seen spec files cache
pub fn clear_seen_spec_file_cache() {
    let mut seen_files = SEEN_SPEC_FILES.lock().unwrap();
    seen_files.clear();
}

/// Used during the initial parsing to determine what kind of data is in a file
#[derive(Deserialize, Debug)]
struct SpecApiVersionMapping {
    #[serde(default = "SpecApiVersion::default")]
    api: SpecApiVersion,
}

/// Enum of all the spfs env spec things that can be constructed from
/// filepaths given on the command line.
#[derive(Deserialize, Debug, Clone, Eq, PartialEq)]
pub enum SpecFile {
    LiveLayer(LiveLayer),
    EnvLayersFile(EnvLayersFile),
}

impl SpecFile {
    pub fn parse(abs_filepath: &str) -> Result<Self> {
        let path = std::path::Path::new(abs_filepath);

        // This has to be an absolute path to distinguish it from tag
        // specs, see EnvSpecItem parsing.
        tracing::debug!("SpecFile::parse: {abs_filepath}");
        if path.is_absolute() {
            let filepath = if path.is_dir() {
                path.join(DEFAULT_LIVE_LAYER_FILENAME)
            } else {
                // Have to use the path as a string because the live
                // layer filename suffix we are looking for has more
                // than one '.' in it.
                let path_string: String = path.display().to_string();
                if !path_string.ends_with(SPFS_FILE_SUFFIX_YAML) {
                    return Err(Error::String(format!(
                        "Invalid: {path_string} does not have the spfs file suffix: {SPFS_FILE_SUFFIX_YAML}"
        )));
                }
                path.to_path_buf()
            };

            // Make sure this doesn't get into a recursive loop before
            // loading this file.
            let mut seen_files = SEEN_SPEC_FILES.lock().unwrap();
            if let Some(file_seen_before) = seen_files.get(path) {
                tracing::debug!(
                    "This is a duplicate spec file: {}",
                    file_seen_before.display()
                );
                return Err(Error::DuplicateSpecFileReference(
                    file_seen_before.to_path_buf(),
                ));
            }
            seen_files.insert(path.to_path_buf());
            drop(seen_files);

            let data = SpecFile::load_data(filepath.clone())?;
            let mut item = SpecFile::from_yaml(&data)?;

            // Live layers also need their parent path added to them
            // and validation run.
            if let SpecFile::LiveLayer(mut live_layer) = item {
                let parent = match filepath.parent() {
                    Some(p) => p,
                    None => {
                        return Err(Error::String(format!(
                            "Cannot have a live layer in the top-level root directory: {}",
                            filepath.display()
                        )))
                    }
                };
                live_layer.set_parent_and_validate(parent.to_path_buf())?;
                item = SpecFile::LiveLayer(live_layer)
            }

            return Ok(item);
        }

        Err(Error::String(format!(
            "Invalid: {} is not an absolute path to a spfs env file. It must be an absolute path to a file",
            path.display()
        )))
    }

    fn load_data(filepath: PathBuf) -> Result<String> {
        tracing::debug!("Opening spfs env file: {}", filepath.display());

        let file = std::fs::File::open(filepath.clone()).map_err(|err| {
            Error::String(format!(
                "Failed to open spfs env file: {} - {err}",
                filepath.display()
            ))
        })?;

        let mut data = String::new();
        std::io::BufReader::new(file)
            .read_to_string(&mut data)
            .map_err(|err| {
                Error::String(format!(
                    "Failed to read data from spfs env file {}: {err}",
                    filepath.display()
                ))
            })?;

        Ok(data)
    }

    /// Create a SpecFile item from the given yaml string
    pub fn from_yaml(s: &str) -> Result<Self> {
        let value: serde_yaml::Value = serde_yaml::from_str(s).map_err(Error::YAML)?;

        // First work out what kind of data this is, based on the
        // SpecApiVersionMapping value.
        let with_version = match serde_yaml::from_value::<SpecApiVersionMapping>(value.clone()) {
            Err(err) => {
                return Err(Error::YAML(err));
            }
            Ok(m) => m,
        };

        // from from_yaml
        let spec = match with_version.api {
            SpecApiVersion::V0Layer => {
                let live_layer: LiveLayer = serde_yaml::from_value(value)?;
                SpecFile::LiveLayer(live_layer)
            }
            SpecApiVersion::V0EnvLayerList => {
                let layers_file: EnvLayersFile = serde_yaml::from_value(value)?;
                SpecFile::EnvLayersFile(layers_file)
            }
        };
        Ok(spec)
    }
}

impl Display for SpecFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LiveLayer(x) => x.fmt(f),
            Self::EnvLayersFile(x) => x.fmt(f),
        }
    }
}

/// A list of env spec references (digests, tags, live layers, or even
/// spfs files) read from a yaml file.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct EnvLayersFile {
    /// The api format version of layers file data
    pub api: SpecApiVersion,
    /// The list of layers (digests, spec file paths, tags) that will
    /// be into /spfs from this spec file. These are not turned into
    /// EnvSpecItem objects until flatten() is called.
    pub layers: Vec<String>,
}

impl EnvLayersFile {
    pub fn flatten(&self) -> Result<Vec<EnvSpecItem>> {
        let mut items = Vec::with_capacity(self.layers.len());
        for reference in &self.layers {
            // Turn the string into a EnvSpecItem, flattening other
            // EnvLayersFile items as we go.
            let item = EnvSpecItem::from_str(reference)?;
            if let EnvSpecItem::SpecFile(SpecFile::EnvLayersFile(nested)) = item {
                items.extend(nested.flatten()?)
            } else {
                items.push(item);
            }
        }

        Ok(items)
    }
}

impl Display for EnvLayersFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: layers: {:?}", self.api, self.layers)
    }
}

/// Specifies an spfs item that can appear in a runtime environment.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum EnvSpecItem {
    TagSpec(TagSpec),
    PartialDigest(encoding::PartialDigest),
    Digest(encoding::Digest),
    SpecFile(SpecFile),
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
            Self::SpecFile(_) => Err(Error::String(String::from(
                "Impossible operation: spfs env files do not have digests",
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

    /// Returns true if this item is a live layer file
    pub fn is_livelayer(&self) -> bool {
        matches!(self, Self::SpecFile(SpecFile::LiveLayer(_)))
    }
}

impl<'de> Deserialize<'de> for EnvSpecItem {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;

        EnvSpecItem::from_str(&value).map_err(|err| {
            serde::de::Error::custom(format!("deserializing EnvSpecItem failed: {err}"))
        })
    }
}

impl std::fmt::Display for EnvSpecItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TagSpec(x) => x.fmt(f),
            Self::PartialDigest(x) => x.fmt(f),
            Self::Digest(x) => x.fmt(f),
            Self::SpecFile(x) => x.fmt(f),
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
    pub fn load_live_layers(&self) -> Vec<LiveLayer> {
        let mut live_layers = Vec::new();
        for item in self.items.iter() {
            if let EnvSpecItem::SpecFile(SpecFile::LiveLayer(ll)) = item {
                live_layers.push(ll.clone());
            }
        }
        live_layers
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
            if let EnvSpecItem::SpecFile(_) = item {
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

impl std::fmt::Display for EnvSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let items: Vec<_> = self.items.iter().map(|i| i.to_string()).collect();
        write!(f, "{}", items.join(ENV_SPEC_SEPARATOR))
    }
}

/// Return the items identified in an environment spec string.
fn parse_env_spec_items<S: AsRef<str>>(spec: S) -> Result<Vec<EnvSpecItem>> {
    if spec.as_ref() == ENV_SPEC_EMPTY || spec.as_ref() == "" {
        return Ok(Vec::new());
    }
    let mut items = Vec::new();
    for layer in spec.as_ref().split(ENV_SPEC_SEPARATOR) {
        let item = parse_env_spec_item(layer)?;
        // Env list of layers files are immediately expanded into the
        // EnvSpec's items list. Other items are just added as is.
        if let EnvSpecItem::SpecFile(SpecFile::EnvLayersFile(layers)) = item {
            items.extend(layers.flatten()?);
        } else {
            items.push(item);
        }
    }
    Ok(items)
}

/// Parse the given string as an single environment spec item.
fn parse_env_spec_item<S: AsRef<str>>(spec: S) -> Result<EnvSpecItem> {
    let spec = spec.as_ref();
    encoding::parse_digest(spec)
        .map(EnvSpecItem::Digest)
        .or_else(|err| {
            tracing::debug!("Unable to parse as a Digest: {err}");
            encoding::PartialDigest::parse(spec).map(EnvSpecItem::PartialDigest)
        })
        .or_else(|err| {
            tracing::debug!("Unable to parse as a Partial Digest: {err}");
            SpecFile::parse(spec).map(EnvSpecItem::SpecFile)
        })
        .or_else(|err| {
            tracing::debug!("Unable to parse as a SpecFile: {err}");
            // A duplicate spec file reference error while parsing a
            // spfs spec file means its filepath had already been read
            // in. Reading it in again would generate an infinite
            // parsing loop, so this should error out now.
            if let Error::DuplicateSpecFileReference(ref _filepath) = err {
                return Err(err);
            }

            TagSpec::parse(spec).map(EnvSpecItem::TagSpec)
        })
}
