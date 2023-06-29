// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::io::BufRead;

use chrono::prelude::*;

use crate::encoding::Encodable;
use crate::{encoding, Error, Result};

#[cfg(test)]
#[path = "./tag_test.rs"]
mod tag_test;

/// Tag links a human name to a storage object at some point in time.
///
/// Much like a commit, tags form a linked-list of entries to track history.
#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub struct Tag {
    org: Option<String>,
    name: String,
    pub target: encoding::Digest,
    pub parent: encoding::Digest,
    pub user: String,
    pub time: DateTime<Utc>,
}

impl Tag {
    pub fn new(
        org: Option<String>,
        name: impl Into<String>,
        target: encoding::Digest,
    ) -> Result<Self> {
        let config = crate::config::get_config()?;
        // we want to ensure that these components
        // can build a valid tag spec
        let spec = build_tag_spec(org, name.into(), 0)?;
        Ok(Self {
            org: spec.org(),
            name: spec.name(),
            target,
            parent: encoding::NULL_DIGEST.into(),
            user: format!("{}@{}", config.user.name, config.user.domain),
            time: Utc::now().trunc_subsecs(6), // ignore microseconds
        })
    }

    pub fn to_spec(&self, version: u64) -> TagSpec {
        TagSpec(self.org.clone(), self.name.clone(), version)
    }

    pub fn org(&self) -> Option<String> {
        self.org.clone()
    }

    pub fn name(&self) -> String {
        self.name.clone()
    }

    /// Return this tag with no version number.
    pub fn path(&self) -> String {
        if let Some(org) = self.org.as_ref() {
            format!("{org}/{}", self.name)
        } else {
            self.name.clone()
        }
    }

    pub fn username_without_org(&self) -> &str {
        self.user
            .split('@')
            .next()
            .expect("Always one item from str::split")
    }
}

impl std::cmp::Ord for Tag {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.org.cmp(&other.org) {
            core::cmp::Ordering::Equal => {}
            ord => return ord,
        }
        match self.name.cmp(&other.name) {
            core::cmp::Ordering::Equal => {}
            ord => return ord,
        }
        match self.time.cmp(&other.time) {
            core::cmp::Ordering::Equal => {}
            ord => return ord,
        }
        match self.target.cmp(&other.target) {
            core::cmp::Ordering::Equal => {}
            ord => return ord,
        }
        match self.parent.cmp(&other.parent) {
            core::cmp::Ordering::Equal => {}
            ord => return ord,
        }
        self.user.cmp(&other.user)
    }
}

impl std::cmp::PartialOrd for Tag {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl std::fmt::Display for Tag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "\
            tag: {}
            digest: {:?}
            target: {:?}
            parent: {:?}
            user: {}
            time: {}",
            self.path(),
            self.digest().map_err(|_| std::fmt::Error)?,
            self.target,
            self.parent,
            self.user,
            self.time,
        ))
    }
}

impl Encodable for Tag {
    type Error = Error;

    fn encode(&self, mut writer: &mut impl std::io::Write) -> Result<()> {
        if let Some(org) = self.org.as_ref() {
            encoding::write_string(&mut writer, org)?;
        } else {
            encoding::write_string(&mut writer, "")?;
        }
        encoding::write_string(&mut writer, &self.name)?;
        encoding::write_digest(&mut writer, &self.target)?;
        encoding::write_string(&mut writer, &self.user)?;
        encoding::write_string(&mut writer, &self.time.to_rfc3339())?;
        encoding::write_digest(writer, &self.parent)?;
        Ok(())
    }
}

impl encoding::Decodable for Tag {
    fn decode(mut reader: &mut impl BufRead) -> Result<Self> {
        let org = encoding::read_string(&mut reader)?;
        let org = match org.as_str() {
            "" => None,
            _ => Some(org),
        };
        Ok(Tag {
            org,
            name: encoding::read_string(&mut reader)?,
            target: encoding::read_digest(&mut reader)?,
            user: encoding::read_string(&mut reader)?,
            time: DateTime::parse_from_rfc3339(&encoding::read_string(&mut reader)?)?.into(),
            parent: encoding::read_digest(reader)?,
        })
    }
}

/// TagSpec identifies a tag within a tag stream.
///
/// The tag spec represents a string specifier or the form:
///     [org /] name [~ version]
/// where org is a slash-separated path denoting a group-like organization for the tag
/// where name is the tag identifier, can only include alphanumeric, '-', ':', '.', and '_'
/// where version is an integer version number specifying a position in the tag
/// stream. The integer '0' always refers to the latest tag in the stream. All other
/// version numbers must be negative, referring to the number of steps back in
/// the version stream to go.
///     eg: spi/main   # latest tag in the spi/main stream
///         spi/main~0 # latest tag in the spi/main stream
///         spi/main~4 # the tag 4 versions behind the latest in the stream
#[derive(Eq, PartialEq, Hash, Clone)]
pub struct TagSpec(Option<String>, String, u64);

impl std::fmt::Debug for TagSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.to_string().as_str())
    }
}

impl std::str::FromStr for TagSpec {
    type Err = crate::Error;

    fn from_str(source: &str) -> Result<Self> {
        TagSpec::parse(source)
    }
}

impl TagSpec {
    pub fn parse<S: AsRef<str>>(spec: S) -> Result<Self> {
        split_tag_spec(spec.as_ref())
    }

    pub fn org(&self) -> Option<String> {
        self.0.clone()
    }

    pub fn name(&self) -> String {
        self.1.clone()
    }

    pub fn version(&self) -> u64 {
        self.2
    }

    /// This tag with no version number.
    pub fn path(&self) -> String {
        if let Some(org) = self.0.as_ref() {
            format!("{org}/{}", self.1)
        } else {
            self.1.clone()
        }
    }

    pub fn with_version(&self, version: u64) -> TagSpec {
        TagSpec(self.0.clone(), self.1.clone(), version)
    }
}

impl std::fmt::Display for TagSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TagSpec(None, name, 0) => f.write_fmt(format_args!("{name}")),
            TagSpec(None, name, version) => f.write_fmt(format_args!("{name}~{version}")),
            TagSpec(Some(org), name, 0) => f.write_fmt(format_args!("{org}/{name}")),
            TagSpec(Some(org), name, version) => {
                f.write_fmt(format_args!("{org}/{name}~{version}"))
            }
        }
    }
}

impl From<TagSpec> for (Option<String>, String, u64) {
    fn from(spec: TagSpec) -> Self {
        (spec.0, spec.1, spec.2)
    }
}

pub fn build_tag_spec(org: Option<String>, name: String, version: u64) -> Result<TagSpec> {
    let mut path = name;
    if let Some(org) = org {
        path = org + "/" + &path;
    }
    let mut spec = path;
    if version != 0 {
        spec = format!("{spec}~{version}");
    }
    TagSpec::parse(&spec)
}

pub fn split_tag_spec(spec: &str) -> Result<TagSpec> {
    let mut parts: Vec<_> = spec.rsplitn(2, '/').collect();
    parts.reverse();
    if parts.len() == 1 {
        // if there was no leading org, insert an empty one
        parts.insert(0, "");
    }

    let name_version = parts.pop().unwrap();
    let mut name_version: Vec<_> = name_version.splitn(2, '~').collect();
    if name_version.len() == 1 {
        name_version.push("0")
    };

    let org = parts.pop().unwrap();
    let version = name_version.pop().unwrap();
    let name = name_version.pop().unwrap();

    if name.is_empty() {
        return Err(format!("tag name cannot be empty: {spec}").into());
    }

    let mut index = _find_org_error(org);
    if let Some(index) = index {
        let err_str = format!(
            "{} > {} < {}",
            &org[..index],
            org.chars().nth(index).unwrap(),
            &org[index + 1..]
        );
        return Err(format!("invalid tag org at pos {index}: {err_str}").into());
    }
    index = _find_name_error(name);
    if let Some(index) = index {
        let err_str = format!(
            "{} > {} < {}",
            &name[..index],
            name.chars().nth(index).unwrap(),
            &name[index + 1..]
        );
        return Err(format!("invalid tag name at pos {index}: {err_str}").into());
    }
    index = _find_version_error(version);
    if let Some(index) = index {
        let err_str = format!(
            "{} > {} < {}",
            &version[..index],
            version.chars().nth(index).unwrap(),
            &version[index + 1..]
        );
        return Err(format!("invalid tag version at pos {index}: {err_str}").into());
    }

    let org = if org.is_empty() {
        None
    } else {
        Some(org.to_string())
    };

    Ok(TagSpec(
        org,
        name.to_string(),
        version
            .parse()
            .map_err(|_| Error::from("Invalid version number, cannot parse as integer"))?,
    ))
}

fn _find_name_error(org: &str) -> Option<usize> {
    for (i, ch) in org.chars().enumerate() {
        match ch {
            '-' | '_' | '.' => (),
            _ => {
                if !ch.is_ascii_alphanumeric() {
                    return Some(i);
                }
            }
        }
    }
    None
}

fn _find_org_error(org: &str) -> Option<usize> {
    for (i, ch) in org.chars().enumerate() {
        match ch {
            '-' | '_' | '.' | '/' => (),
            _ => {
                if !ch.is_ascii_alphanumeric() {
                    return Some(i);
                }
            }
        }
    }
    None
}

fn _find_version_error(version: &str) -> Option<usize> {
    for (i, ch) in version.chars().enumerate() {
        match ch {
            '-' | '_' | '.' => (),
            _ => {
                if !ch.is_ascii_digit() {
                    return Some(i);
                }
            }
        }
    }
    None
}
