use chrono::prelude::*;

use crate::encoding;
use crate::encoding::Encodable;
use crate::{Error, Result};

#[cfg(test)]
#[path = "./tag_test.rs"]
mod tag_test;

/// Tag links a human name to a storage object at some point in time.
///
/// Much like a commit, tags form a linked-list of entries to track history.
/// Time should always be in UTC.
#[derive(Debug, PartialEq, Eq, Clone)]
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
        org: impl Into<String>,
        name: impl Into<String>,
        target: encoding::Digest,
    ) -> Result<Self> {
        // we want to ensure these components
        // can build a valid tag spec
        let spec = build_tag_spec(org.into(), name.into(), 0)?;
        Ok(Self {
            org: spec.org(),
            name: spec.name(),
            target: target,
            parent: encoding::NULL_DIGEST.into(),
            user: format!("{}@imageworks.com", whoami::username()),
            time: Utc::now().trunc_subsecs(6), // ignore microseconds
        })
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
            format!("{}/{}", org, self.name)
        } else {
            self.name.clone()
        }
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
    fn encode(&self, mut writer: &mut impl std::io::Write) -> Result<()> {
        if let Some(org) = self.org.as_ref() {
            encoding::write_string(&mut writer, org)?;
        } else {
            encoding::write_string(&mut writer, "")?;
        }
        encoding::write_string(&mut writer, &self.name)?;
        encoding::write_digest(&mut writer, self.target)?;
        encoding::write_string(&mut writer, &self.user)?;
        encoding::write_string(&mut writer, &self.time.to_rfc3339())?;
        encoding::write_digest(writer, self.parent)?;
        Ok(())
    }

    fn decode(mut reader: &mut impl std::io::Read) -> Result<Self> {
        let org = encoding::read_string(&mut reader)?;
        let org = match org.as_str() {
            "" => None,
            _ => Some(org),
        };
        Ok(Tag {
            org: org,
            name: encoding::read_string(&mut reader)?,
            target: encoding::read_digest(&mut reader)?,
            user: encoding::read_string(&mut reader)?,
            time: DateTime::parse_from_rfc3339(&encoding::read_string(&mut reader)?)
                .map_err(|_| Error::from("invalid datetime format in stored tag"))?
                .into(),
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
#[derive(Debug, Eq, PartialEq, Hash)]
pub struct TagSpec(Option<String>, String, u64);

impl TagSpec {
    pub fn new(spec: &str) -> Result<Self> {
        split_tag_spec(spec)
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
            format!("{}/{}", org, self.1)
        } else {
            self.1.clone()
        }
    }
}

impl std::fmt::Display for TagSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TagSpec(None, name, 0) => f.write_fmt(format_args!("{}", name)),
            TagSpec(None, name, version) => f.write_fmt(format_args!("{}~{}", name, version)),
            TagSpec(Some(org), name, 0) => f.write_fmt(format_args!("{}/{}", org, name)),
            TagSpec(Some(org), name, version) => {
                f.write_fmt(format_args!("{}/{}~{}", org, name, version))
            }
        }
    }
}

impl Into<(Option<String>, String, u64)> for TagSpec {
    fn into(self) -> (Option<String>, String, u64) {
        (self.0, self.1, self.2)
    }
}

pub fn build_tag_spec(org: String, name: String, version: u64) -> Result<TagSpec> {
    let mut path = name;
    if !org.is_empty() {
        path = org + "/" + &path;
    }
    let mut spec = path;
    if version != 0 {
        spec = format!("{}~{}", &spec, version);
    }
    TagSpec::new(&spec)
}

pub fn split_tag_spec(spec: &str) -> Result<TagSpec> {
    let mut parts: Vec<_> = spec.rsplitn(2, "/").collect();
    parts.reverse();
    if parts.len() == 1 {
        // if there was no leading org, insert an empty one
        parts.insert(0, "");
    }

    let name_version = parts.pop().unwrap();
    let mut name_version: Vec<_> = name_version.splitn(2, "~").collect();
    match name_version.len() {
        1 => name_version.push("0"),
        _ => (),
    };

    let org = parts.pop().unwrap();
    let version = name_version.pop().unwrap();
    let name = name_version.pop().unwrap();

    if name.is_empty() {
        return Err(format!("tag name cannot be empty: {}", spec).into());
    }

    let mut index = _find_org_error(org);
    if let Some(index) = index {
        let err_str = format!(
            "{} > {} < {}",
            &org[..index],
            org.chars().nth(index).unwrap(),
            &org[index + 1..]
        );
        return Err(format!("invalid tag org at pos {}: {}", index, err_str).into());
    }
    index = _find_name_error(name);
    if let Some(index) = index {
        let err_str = format!(
            "{} > {} < {}",
            &name[..index],
            name.chars().nth(index).unwrap(),
            &name[index + 1..]
        );
        return Err(format!("invalid tag name at pos {}: {}", index, err_str).into());
    }
    index = _find_version_error(version);
    if let Some(index) = index {
        let err_str = format!(
            "{} > {} < {}",
            &version[..index],
            version.chars().nth(index).unwrap(),
            &version[index + 1..]
        );
        return Err(format!("invalid tag version at pos {}: {}", index, err_str).into());
    }

    let org = if org.is_empty() {
        None
    } else {
        Some(org.to_string())
    };

    return Ok(TagSpec(
        org,
        name.to_string(),
        version
            .parse()
            .map_err(|_| Error::from("Invalid version number, cannot parse as integer"))?,
    ));
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
