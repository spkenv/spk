// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

mod compat;
mod error;
pub mod parsing;
mod parts_iter;

use std::borrow::Cow;
use std::cmp::{Ord, Ordering};
use std::convert::TryFrom;
use std::fmt::Write;
use std::ops::{Deref, DerefMut};
use std::str::FromStr;

pub use compat::{
    API_STR,
    BINARY_STR,
    BuildIdProblem,
    CommaSeparated,
    Compat,
    CompatRule,
    CompatRuleSet,
    Compatibility,
    ComponentsMissingProblem,
    ConflictingRequirementProblem,
    ImpossibleRequestProblem,
    InclusionPolicyProblem,
    IncompatibleReason,
    IsSameReasonAs,
    PackageNameProblem,
    PackageRepoProblem,
    RangeSupersetProblem,
    VarOptionProblem,
    VarRequestProblem,
    VersionForClause,
    VersionNotDifferentProblem,
    VersionNotEqualProblem,
    VersionRangeProblem,
    parse_compat,
};
pub use error::{Error, Result};
use itertools::Itertools;
use miette::Diagnostic;
use relative_path::RelativePathBuf;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

use self::parts_iter::{MinimumPartsPartIter, NormalizedPartsIter};
use crate::ident_ops::{MetadataPath, TagPath};
use crate::name::validate_tag_name;

#[cfg(test)]
#[path = "./version_test.rs"]
mod version_test;

pub const VERSION_SEP: &str = ".";
pub const TAG_SET_SEP: &str = ",";
pub const TAG_SEP: &str = ".";

// Labels for the names of the components, or positions, in a version
// number.
pub const SENTINEL_LABEL: &str = "Tail";
pub const POSITION_LABELS: &[&str] = &["Major", "Minor", "Patch"];

/// Returns the name of the version component at the given position.
///
/// Position zero corresponds to 'Major', 1 to 'Minor' and so on.
/// Positions beyond the known component range return 'Tail'.
pub fn get_version_position_label(pos: usize) -> &'static str {
    if pos >= POSITION_LABELS.len() {
        return SENTINEL_LABEL;
    }
    POSITION_LABELS[pos]
}

/// Denotes that an invalid version number was given.
#[derive(Diagnostic, Debug, Error)]
#[error("Invalid version: {message}")]
pub struct InvalidVersionError {
    pub message: String,
}

impl InvalidVersionError {
    pub fn new_error(msg: String) -> Error {
        Error::InvalidVersionError(Self { message: msg })
    }
}

/// TagSet contains a set of pre or post release version tags
#[derive(Debug, Default, Clone, Eq, PartialEq, Hash)]
pub struct TagSet {
    pub tags: std::collections::BTreeMap<String, u32>,
}

impl Deref for TagSet {
    type Target = std::collections::BTreeMap<String, u32>;

    fn deref(&self) -> &Self::Target {
        &self.tags
    }
}

impl TagSet {
    pub fn single<S: Into<String>>(name: S, value: u32) -> TagSet {
        let mut tag_set = TagSet::default();
        tag_set.tags.insert(name.into(), value);
        tag_set
    }
    pub fn double<S: Into<String>>(name_1: S, value_1: u32, name_2: S, value_2: u32) -> TagSet {
        let mut tag_set = TagSet::default();
        tag_set.tags.insert(name_1.into(), value_1);
        tag_set.tags.insert(name_2.into(), value_2);
        tag_set
    }
    pub fn is_empty(&self) -> bool {
        self.tags.keys().len() == 0
    }
}

impl std::fmt::Display for TagSet {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let parts: Vec<_> = self
            .tags
            .iter()
            .map(|(name, num)| format!("{name}.{num}"))
            .collect();
        write!(f, "{}", parts.join(TAG_SET_SEP))
    }
}

impl PartialOrd for TagSet {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TagSet {
    fn cmp(&self, other: &Self) -> Ordering {
        let mut self_entries: Vec<_> = self.tags.iter().collect();
        let mut other_entries: Vec<_> = other.tags.iter().collect();
        self_entries.sort_unstable();
        other_entries.sort_unstable();
        let self_entries = self_entries.into_iter();
        let other_entries = other_entries.into_iter();

        for ((self_name, self_value), (other_name, other_value)) in self_entries.zip(other_entries)
        {
            match self_name.cmp(other_name) {
                Ordering::Equal => (),
                res => return res,
            }
            match self_value.cmp(other_value) {
                Ordering::Equal => (),
                res => return res,
            }
        }

        self.tags.len().cmp(&other.tags.len())
    }
}

impl FromStr for TagSet {
    type Err = Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        parse_tag_set(s)
    }
}

/// Parse the given string as a set of version tags.
///
/// ```
/// let tag_set = spk_schema_foundation::version::parse_tag_set("dev.4,alpha.1").unwrap();
/// assert_eq!(tag_set.get("alpha"), Some(&1));
/// ```
pub fn parse_tag_set<S: AsRef<str>>(tags: S) -> Result<TagSet> {
    let tags = tags.as_ref();
    let mut tag_set = TagSet::default();
    if tags.is_empty() {
        return Ok(tag_set);
    }

    for tag in tags.split(TAG_SET_SEP) {
        let (name, num) = break_string(tag, TAG_SEP);
        match (name, num) {
            ("", _) | (_, "") => {
                return Err(InvalidVersionError::new_error(format!(
                    "Version tag segment must be of the form <name>.<int>, got [{tag}]"
                )));
            }
            _ => {
                if tag_set.tags.contains_key(name) {
                    return Err(InvalidVersionError::new_error(format!(
                        "duplicate tag: {name}"
                    )));
                }
                validate_tag_name(name)?;
                match num.parse() {
                    Ok(num) => {
                        tag_set.tags.insert(name.to_string(), num);
                    }
                    Err(_) => {
                        return Err(InvalidVersionError::new_error(format!(
                            "Version tag segment must be of the form <name>.<int>, got [{tag}]"
                        )));
                    }
                }
            }
        }
    }

    Ok(tag_set)
}

/// The numeric portion of a version.
#[derive(Clone, Debug, Default)]
pub struct VersionParts {
    pub parts: Vec<u32>,
    pub plus_epsilon: bool,
}

impl VersionParts {
    /// Return an iterator over the parts of this version.
    fn iter_for_display(&self, minimum_parts: usize) -> impl Iterator<Item = u32> + '_ {
        MinimumPartsPartIter::new(self.parts.as_slice(), minimum_parts)
    }

    /// Return an iterator over the normalized parts of this version.
    fn iter_for_storage(&self) -> impl Iterator<Item = u32> + '_ {
        NormalizedPartsIter::new(self.parts.as_slice())
    }

    /// Return a copy of this version with no trailing zeros.
    ///
    /// A version with all zeros, or an empty parts list, will be returned as
    /// a version with a single part of 0.
    ///
    /// ```
    /// # use spk_schema_foundation::version::VersionParts;
    /// assert_eq!(
    ///     VersionParts::from(vec![1, 0, 0, 0]).strip_trailing_zeros().parts,
    ///     VersionParts::from(vec![1]).parts
    /// );
    /// assert_eq!(
    ///     VersionParts::from(vec![1, 0, 0, 1]).strip_trailing_zeros().parts,
    ///     VersionParts::from(vec![1, 0, 0, 1]).parts
    /// );
    /// assert_eq!(
    ///     VersionParts::from(vec![1, 0, 0, 1, 0]).strip_trailing_zeros().parts,
    ///     VersionParts::from(vec![1, 0, 0, 1]).parts
    /// );
    /// assert_eq!(
    ///     VersionParts::from(vec![1, 2]).strip_trailing_zeros().parts,
    ///     VersionParts::from(vec![1, 2]).parts
    /// );
    /// assert_eq!(
    ///     VersionParts::from(vec![1, 2, 3]).strip_trailing_zeros().parts,
    ///     VersionParts::from(vec![1, 2, 3]).parts
    /// );
    /// assert_eq!(
    ///     VersionParts::from(vec![0, 0, 0]).strip_trailing_zeros().parts,
    ///     VersionParts::from(vec![0]).parts
    /// );
    /// ```
    pub fn strip_trailing_zeros(&self) -> Cow<'_, Self> {
        match self.parts.iter().rposition(|p| p != &0) {
            Some(index_of_last_non_zero) if index_of_last_non_zero == self.parts.len() - 1 => {
                // Already normalized; can borrow.
                Cow::Borrowed(self)
            }
            Some(index_of_last_non_zero) => Cow::Owned(VersionParts {
                parts: self
                    .parts
                    .iter()
                    .take(index_of_last_non_zero + 1)
                    .copied()
                    .collect(),
                plus_epsilon: self.plus_epsilon,
            }),
            None => Cow::Owned(VersionParts {
                parts: vec![0],
                plus_epsilon: self.plus_epsilon,
            }),
        }
    }
}

impl Deref for VersionParts {
    type Target = Vec<u32>;

    fn deref(&self) -> &Self::Target {
        &self.parts
    }
}

impl DerefMut for VersionParts {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.parts
    }
}

impl From<Vec<u32>> for VersionParts {
    fn from(parts: Vec<u32>) -> Self {
        Self {
            parts,
            plus_epsilon: false,
        }
    }
}

impl std::cmp::PartialEq for VersionParts {
    fn eq(&self, other: &Self) -> bool {
        if self.plus_epsilon != other.plus_epsilon {
            return false;
        }

        let self_last_nonzero_digit = self.parts.iter().rposition(|d| d != &0);
        let other_last_nonzero_digit = other.parts.iter().rposition(|d| d != &0);

        match (self_last_nonzero_digit, other_last_nonzero_digit) {
            (Some(self_last), Some(other_last)) => {
                self.parts[..=self_last] == other.parts[..=other_last]
            }
            (None, None) => true,
            _ => false,
        }
    }
}

impl std::cmp::Eq for VersionParts {}

impl std::hash::Hash for VersionParts {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // trailing zeros do not alter the hash/cmp for a version
        if let Some(last_nonzero) = self.parts.iter().rposition(|d| d != &0) {
            self.parts[..=last_nonzero].hash(state)
        };
        self.plus_epsilon.hash(state);
    }
}

/// Version specifies a package version number.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct Version {
    pub parts: VersionParts,
    pub pre: TagSet,
    pub post: TagSet,
}

impl<S> std::cmp::PartialEq<S> for Version
where
    S: AsRef<str>,
{
    fn eq(&self, other: &S) -> bool {
        match Self::from_str(other.as_ref()) {
            Ok(v) => self == &v,
            Err(_) => false,
        }
    }
}

impl Version {
    /// How many version parts are always shown when displaying a version.
    ///
    /// If a version has fewer parts than this, it will be padded with zeros.
    pub const MINIMUM_PARTS_FOR_DISPLAY: usize = 3;
    /// How many version parts are always used when tagging a version.
    ///
    /// If a version has fewer parts than this, it will be padded with zeros.
    pub const MINIMUM_PARTS_FOR_STORAGE: usize = 3;

    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Version {
            parts: vec![major, minor, patch].into(),
            ..Default::default()
        }
    }

    /// The major version number (first component)
    pub fn major(&self) -> u32 {
        self.parts.first().copied().unwrap_or_default()
    }

    /// The minor version number (second component)
    pub fn minor(&self) -> u32 {
        self.parts.get(1).copied().unwrap_or_default()
    }

    /// Enable the `plus_epsilon` bit on this Version.
    ///
    /// For purposes of comparing versions, a version with this bit
    /// enabled is considered infinitesimally bigger than the stated
    /// version.
    ///
    /// Therefore, an expression like "<2.0" can be represented
    /// by a `Version` with parts `[2, 0]` and `plus_epsilon: true`.
    /// Using parts `[2, 1]` is *not* accurate because
    /// `2.0 < 2.0.1 < 2.1`.
    pub fn plus_epsilon(mut self) -> Self {
        self.parts.plus_epsilon = true;
        self
    }

    /// The patch version number (third component)
    pub fn patch(&self) -> u32 {
        self.parts.get(2).copied().unwrap_or_default()
    }

    /// Format just the pre- and post- release tags (if any).
    pub fn format_tags(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if !self.pre.tags.is_empty() {
            f.write_char('-')?;
            f.write_str(&self.pre.to_string())?;
        }
        if !self.post.tags.is_empty() {
            f.write_char('+')?;
            f.write_str(&self.post.to_string())?;
        }
        Ok(())
    }

    /// Build a new version number from any number of digits
    pub fn from_parts<P: IntoIterator<Item = u32>>(parts: P) -> Self {
        Version {
            parts: parts.into_iter().collect::<Vec<_>>().into(),
            ..Default::default()
        }
    }

    /// The base integer portion of this version as a string.
    ///
    /// The version number will be normalized to at least three parts.
    #[inline]
    pub fn base_normalized(&self) -> String {
        self.base(self.parts.iter_for_display(Self::MINIMUM_PARTS_FOR_DISPLAY))
    }

    /// The base integer portion of this version as a string.
    ///
    /// The version number will have the same precision as originally parsed.
    #[inline]
    pub fn base_verbatim(&self) -> String {
        self.base(self.parts.iter())
    }

    /// The base integer portion of this version as a string
    fn base<I, D>(&self, mut iter: I) -> String
    where
        I: Iterator<Item = D>,
        D: std::fmt::Display,
    {
        let mut s = iter.join(VERSION_SEP);
        if s.is_empty() {
            // the base version cannot ever be an empty string, as that
            // is not a valid version
            s.push('0');
        }
        // This suffix is useful to not confuse users when used in
        // rendering the message for `Incompatible` results from
        // `Ranged::intersects` and should generally not show up in
        // other contexts. If it does end up leaking out in other
        // places then some code refactoring is needed here.
        if self.parts.plus_epsilon {
            s.push_str("+ε")
        }
        s
    }

    /// Reports if this version is exactly 0.0.0... etc.
    pub fn is_zero(&self) -> bool {
        if !self.pre.is_empty() || !self.post.is_empty() {
            return false;
        }
        !self.parts.iter().any(|x| x > &0)
    }

    /// Like `to_string` but normalize the base version as it would be for its
    /// spfs tag path.
    pub fn to_storage_string(&self) -> String {
        format!(
            "{}{}{}{}{}",
            self.parts
                .iter_for_storage()
                .map(|p| p.to_string())
                .join(VERSION_SEP),
            { if self.pre.is_empty() { "" } else { "-" } },
            self.pre,
            { if self.post.is_empty() { "" } else { "+" } },
            self.post,
        )
    }
}

impl MetadataPath for Version {
    fn metadata_path(&self) -> RelativePathBuf {
        RelativePathBuf::from(self.to_string())
    }
}

impl TagPath for Version {
    fn tag_path(&self) -> RelativePathBuf {
        RelativePathBuf::from(format!(
            "{base}{pre_sep}{pre}{post_sep}{post}",
            base = self
                .parts
                .iter_for_storage()
                .map(|p| p.to_string())
                .join(VERSION_SEP),
            pre_sep = if self.pre.is_empty() { "" } else { "-" },
            pre = self.pre,
            // the "+" character is not a valid spfs tag character,
            // so we 'encode' it with two dots, which is not a valid sequence
            // for spk package names
            post_sep = if self.post.is_empty() { "" } else { ".." },
            post = self.post,
        ))
    }

    fn verbatim_tag_path(&self) -> RelativePathBuf {
        RelativePathBuf::from(format!(
            "{base}{pre_sep}{pre}{post_sep}{post}",
            base = self
                .parts
                .iter_for_display(1)
                .map(|p| p.to_string())
                .join(VERSION_SEP),
            pre_sep = if self.pre.is_empty() { "" } else { "-" },
            pre = self.pre,
            // the "+" character is not a valid spfs tag character,
            // so we 'encode' it with two dots, which is not a valid sequence
            // for spk package names
            post_sep = if self.post.is_empty() { "" } else { ".." },
            post = self.post,
        ))
    }
}

impl From<VersionParts> for Version {
    fn from(parts: VersionParts) -> Self {
        Self {
            parts,
            ..Default::default()
        }
    }
}

impl TryFrom<&str> for Version {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self> {
        parse_version(value)
    }
}

impl TryFrom<String> for Version {
    type Error = Error;

    fn try_from(value: String) -> Result<Self> {
        parse_version(value)
    }
}

impl FromStr for Version {
    type Err = Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        parse_version(s)
    }
}

/// Format a version number as a string.
///
/// If the alternate flag is set, the version will be formatted as written,
/// otherwise it will be normalized to at least three parts.
impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if f.alternate() {
            f.write_str(&self.base_verbatim())?;
        } else {
            f.write_str(&self.base_normalized())?;
        }
        self.format_tags(f)
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        let self_parts = self.parts.iter();
        let mut other_parts = other.parts.iter();

        for self_part in self_parts {
            match other_parts.next() {
                Some(other_part) => match self_part.cmp(other_part) {
                    Ordering::Equal => continue,
                    res => return res,
                },
                None if self_part == &0 => {
                    // having more parts than the other only makes
                    // us greater if it's a non-zero value
                    // eg: 1.2.0 == 1.2.0.0.0
                    continue;
                }
                None => {
                    // we have more base parts than other
                    return Ordering::Greater;
                }
            }
        }

        match other_parts.max() {
            // same as above, having more parts only matters
            // if they are non-zero
            None | Some(0) => {}
            Some(_) => {
                // other has more base parts than we do
                return Ordering::Less;
            }
        }

        match (self.pre.is_empty(), other.pre.is_empty()) {
            (true, true) => (),
            (true, false) => return Ordering::Greater,
            (false, true) => return Ordering::Less,
            (false, false) => match self.pre.cmp(&other.pre) {
                Ordering::Equal => (),
                cmp => return cmp,
            },
        }

        // Compare epsilon _before_ post release:
        //
        //     1.73.0 < 1.73.0+r.1 < 1.73.0+ε
        //
        match (self.parts.plus_epsilon, other.parts.plus_epsilon) {
            (true, false) => return Ordering::Greater,
            (false, true) => return Ordering::Less,
            _ => (),
        }

        self.post.cmp(&other.post)
    }
}

/// Parse a string as a version specifier.
pub fn parse_version<S: AsRef<str>>(version: S) -> Result<Version> {
    let version = version.as_ref();
    if version.is_empty() {
        return Ok(Version::default());
    }

    let (version, post) = break_string(version, "+");
    let (version, pre) = break_string(version, "-");

    let str_parts = version.split(VERSION_SEP);
    let mut parts = Vec::new();
    for (i, p) in str_parts.enumerate() {
        match p.parse() {
            Ok(p) => parts.push(p),
            Err(_) => {
                return Err(InvalidVersionError::new_error(format!(
                    "Version must be a sequence of integers, got '{p}' in position {i} [{version}]"
                )));
            }
        }
    }

    let mut v = Version::from_parts(parts);
    v.pre = parse_tag_set(pre)?;
    v.post = parse_tag_set(post)?;
    Ok(v)
}

impl Serialize for Version {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Preserve the original version string as parsed.
        let version_string = format!("{:#}", self);
        serializer.serialize_str(&version_string)
    }
}

impl<'de> Deserialize<'de> for Version {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct VersionVisitor;

        impl serde::de::Visitor<'_> for VersionVisitor {
            type Value = Version;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a version number (eg: 1.0.0, 1.0.0-pre.1, 1.2.3.4+post.0)")
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Version, E>
            where
                E: serde::de::Error,
            {
                Version::from_str(value).map_err(serde::de::Error::custom)
            }
        }
        deserializer.deserialize_str(VersionVisitor)
    }
}

fn break_string<'a>(string: &'a str, sep: &str) -> (&'a str, &'a str) {
    let mut parts = string.splitn(2, sep);
    (parts.next().unwrap_or(string), parts.next().unwrap_or(""))
}
