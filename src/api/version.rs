// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::cmp::{Ord, Ordering};

use super::validate_tag_name;

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
    pub fn new(msg: String) -> crate::Error {
        crate::Error::InvalidVersionError(Self { message: msg })
    }
}

/// TagSet contains a set of pre or post release version tags
#[derive(Debug, Default, Clone, Eq, PartialEq)]
pub struct TagSet(std::collections::BTreeMap<String, u32>);

impl TagSet {
    pub fn single<S: Into<String>>(name: S, value: u32) -> TagSet {
        let mut tag_set = TagSet::default();
        tag_set.0.insert(name.into(), value);
        return tag_set;
    }
    pub fn double<S: Into<String>>(name_1: S, value_1: u32, name_2: S, value_2: u32) -> TagSet {
        let mut tag_set = TagSet::default();
        tag_set.0.insert(name_1.into(), value_1);
        tag_set.0.insert(name_2.into(), value_2);
        return tag_set;
    }
    pub fn is_empty(&self) -> bool {
        self.0.keys().len() == 0
    }
}

impl ToString for TagSet {
    fn to_string(&self) -> String {
        let parts: Vec<_> = self
            .0
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
        let mut self_entries: Vec<_> = self.0.iter().collect();
        let mut other_entries: Vec<_> = other.0.iter().collect();
        self_entries.sort_unstable();
        other_entries.sort_unstable();
        let self_entries = self_entries.into_iter();
        let other_entries = other_entries.into_iter();

        for ((self_name, self_value), (other_name, other_value)) in self_entries.zip(other_entries)
        {
            println!(
                "{}.{} {}.{}",
                self_name, self_value, other_name, other_value
            );
            match self_name.cmp(&other_name) {
                Ordering::Equal => (),
                res => return res,
            }
            match self_value.cmp(&other_value) {
                Ordering::Equal => (),
                res => return res,
            }
        }

        return self.0.len().cmp(&other.0.len());
    }
}

/// Parse the given string as a set of version tags.
///
/// ```
/// let tag_set = parse_tag_set("release.0,alpha.1").unwrap();
/// assert_eq!(tag_set.get("alpha"), Some(1));
/// ```
pub fn parse_tag_set<S: AsRef<str>>(tags: S) -> crate::Result<TagSet> {
    let tags = tags.as_ref();
    let mut tag_set = TagSet::default();
    if tags.len() == 0 {
        return Ok(tag_set);
    }

    for tag in tags.split(TAG_SET_SEP) {
        let (name, num) = break_string(tag, TAG_SEP);
        match (name, num) {
            ("", _) | (_, "") => {
                return Err(InvalidVersionError::new(format!(
                    "Version tag segment must be of the form <name>.<int>, got [{}]",
                    tag
                )))
            }
            _ => {
                if tag_set.0.contains_key(name) {
                    return Err(InvalidVersionError::new(format!("duplicate tag: {}", name)));
                }
                validate_tag_name(name)?;
                match num.parse() {
                    Ok(num) => {
                        tag_set.0.insert(name.to_string(), num);
                    }
                    Err(_) => {
                        return Err(InvalidVersionError::new(format!(
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
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub tail: Vec<u32>,
    pub pre: TagSet,
    pub post: TagSet,
}

impl Version {
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Version {
            major: major,
            minor: minor,
            patch: patch,
            ..Default::default()
        }
    }

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
            true
        } else {
            self.pre.is_empty() && self.post.is_empty()
        }
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut base = self.base();
        if self.pre.0.len() > 0 {
            base = format!("{}-{}", base, self.pre.to_string());
        }
        if self.post.0.len() > 0 {
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

        if let Some(_) = other_parts.next() {
            // other has more base parts than we do
            return Ordering::Less;
        }

        match self.pre.cmp(&other.pre) {
            Ordering::Equal => (),
            Ordering::Less => return Ordering::Greater,
            Ordering::Greater => return Ordering::Less,
        }

        self.post.cmp(&other.post)
    }
}

/// Parse a string as a version specifier.
pub fn parse_version<S: AsRef<str>>(version: S) -> crate::Result<Version> {
    let version = version.as_ref();
    if version.len() == 0 {
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
                return Err(InvalidVersionError::new(format!(
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

fn break_string<'a>(string: &'a str, sep: &str) -> (&'a str, &'a str) {
    let mut parts = string.splitn(2, sep);
    (parts.next().unwrap_or(string), parts.next().unwrap_or(""))
}
