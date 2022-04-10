// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::{
    cmp::{Ord, Ordering},
    convert::TryFrom,
    str::FromStr,
};

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

use super::validate_tag_name;
use crate::Error;

#[cfg(test)]
#[path = "./version_test.rs"]
mod version_test;

pub const VERSION_SEP: &str = ".";
pub const TAG_SET_SEP: &str = ",";
pub const TAG_SEP: &str = ".";

/// Denotes that an invalid verison number was given.
#[derive(Debug)]
pub struct InvalidVersionError {
    pub message: String,
}

impl InvalidVersionError {
    pub fn new_error(msg: String) -> crate::Error {
        crate::Error::InvalidVersionError(Self { message: msg })
    }
}

/// TagSet contains a set of pre or post release version tags
#[derive(Debug, Default, Clone, Eq, PartialEq, Hash)]
pub struct TagSet {
    tags: std::collections::BTreeMap<String, u32>,
}

impl std::ops::Deref for TagSet {
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

impl ToString for TagSet {
    fn to_string(&self) -> String {
        let parts: Vec<_> = self
            .tags
            .iter()
            .map(|(name, num)| format!("{}.{}", name, num))
            .collect();
        parts.join(TAG_SET_SEP)
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

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_tag_set(s)
    }
}

/// Parse the given string as a set of version tags.
///
/// ```
/// let tag_set = spk::api::parse_tag_set("dev.4,alpha.1").unwrap();
/// assert_eq!(tag_set.get("alpha"), Some(&1));
/// ```
pub fn parse_tag_set<S: AsRef<str>>(tags: S) -> crate::Result<TagSet> {
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
                    "Version tag segment must be of the form <name>.<int>, got [{}]",
                    tag
                )))
            }
            _ => {
                if tag_set.tags.contains_key(name) {
                    return Err(InvalidVersionError::new_error(format!(
                        "duplicate tag: {}",
                        name
                    )));
                }
                validate_tag_name(name)?;
                match num.parse() {
                    Ok(num) => {
                        tag_set.tags.insert(name.to_string(), num);
                    }
                    Err(_) => {
                        return Err(InvalidVersionError::new_error(format!(
                            "Version tag segment must be of the form <name>.<int>, got [{}]",
                            tag
                        )))
                    }
                }
            }
        }
    }

    Ok(tag_set)
}

/// Version specifies a package version number.
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub tail: Vec<u32>,
    pub pre: TagSet,
    pub post: TagSet,
}

impl std::cmp::PartialEq<&str> for Version {
    fn eq(&self, other: &&str) -> bool {
        self.to_string().eq(other)
    }
}

impl Version {
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Version {
            major,
            minor,
            patch,
            ..Default::default()
        }
    }

    /// Build a new version number from any number of digits
    pub fn from_parts<P: Iterator<Item = u32>>(mut parts: P) -> Self {
        Version {
            major: parts.next().unwrap_or_default(),
            minor: parts.next().unwrap_or_default(),
            patch: parts.next().unwrap_or_default(),
            tail: parts.collect(),
            ..Default::default()
        }
    }

    /// The integer pieces of this version number
    pub fn parts(&self) -> Vec<u32> {
        let mut parts = vec![self.major, self.minor, self.patch];
        parts.append(&mut self.tail.clone());
        parts
    }

    /// The base integer portion of this version as a string
    pub fn base(&self) -> String {
        let part_strings: Vec<_> = self.parts().into_iter().map(|p| p.to_string()).collect();
        part_strings.join(VERSION_SEP)
    }

    /// Reports if this version is exactly 0.0.0
    pub fn is_zero(&self) -> bool {
        if let Some(0) = self.parts().iter().max() {
            self.pre.is_empty() && self.post.is_empty()
        } else {
            false
        }
    }
}

impl TryFrom<&str> for Version {
    type Error = crate::Error;

    fn try_from(value: &str) -> crate::Result<Self> {
        parse_version(value)
    }
}

impl FromStr for Version {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_version(s)
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut base = self.base();
        if !self.pre.tags.is_empty() {
            base = format!("{}-{}", base, self.pre.to_string());
        }
        if !self.post.tags.is_empty() {
            base = format!("{}+{}", base, self.post.to_string());
        }
        f.write_str(&base)
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        let self_parts = self.parts().into_iter();
        let mut other_parts = other.parts().into_iter();

        for self_part in self_parts {
            match other_parts.next() {
                Some(other_part) => match self_part.cmp(&other_part) {
                    Ordering::Equal => continue,
                    res => return res,
                },
                None => {
                    // we have more base parts than other
                    return Ordering::Greater;
                }
            }
        }

        if other_parts.next().is_some() {
            // other has more base parts than we do
            return Ordering::Less;
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

        self.post.cmp(&other.post)
    }
}

/// Parse a string as a version specifier.
pub fn parse_version<S: AsRef<str>>(version: S) -> crate::Result<Version> {
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
                    "Version must be a sequence of integers, got '{}' in position {} [{}]",
                    p, i, version
                )))
            }
        }
    }

    let mut v = Version::from_parts(parts.into_iter());
    v.pre = parse_tag_set(pre)?;
    v.post = parse_tag_set(post)?;
    Ok(v)
}

impl Serialize for Version {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
impl<'de> Deserialize<'de> for Version {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s).map_err(de::Error::custom)
    }
}

fn break_string<'a>(string: &'a str, sep: &str) -> (&'a str, &'a str) {
    let mut parts = string.splitn(2, sep);
    (parts.next().unwrap_or(string), parts.next().unwrap_or(""))
}
