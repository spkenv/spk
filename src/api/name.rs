// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::{convert::TryFrom, str::FromStr};
use thiserror::Error;

#[cfg(test)]
#[path = "./name_test.rs"]
mod name_test;

const NAME_MIN_LEN: usize = 2;
const NAME_MAX_LEN: usize = 64;

/// Denotes that an invalid package name was given.
#[derive(Debug, Error)]
#[error("Invalid name error: {message}")]
pub struct InvalidNameError {
    pub message: String,
}

impl InvalidNameError {
    pub fn new_error(msg: String) -> crate::Error {
        crate::Error::InvalidNameError(Self { message: msg })
    }
}

/// A valid package name
#[derive(Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct PkgName(String);

impl std::ops::Deref for PkgName {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::fmt::Display for PkgName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl AsRef<str> for PkgName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl AsRef<std::path::Path> for PkgName {
    fn as_ref(&self) -> &std::path::Path {
        std::path::Path::new(&self.0)
    }
}

impl AsRef<std::ffi::OsStr> for PkgName {
    fn as_ref(&self) -> &std::ffi::OsStr {
        std::ffi::OsStr::new(&self.0)
    }
}

impl From<PkgName> for String {
    fn from(val: PkgName) -> Self {
        val.0
    }
}

impl FromStr for PkgName {
    type Err = crate::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        validate_name(s)?;
        Ok(PkgName(s.to_string()))
    }
}

impl TryFrom<String> for PkgName {
    type Error = crate::Error;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        validate_name(&s)?;
        Ok(PkgName(s))
    }
}

/// Return 'name' if it's a valide package name
pub fn validate_name<S: AsRef<str>>(name: S) -> crate::Result<()> {
    if name.as_ref().len() < NAME_MIN_LEN {
        return Err(InvalidNameError::new_error(format!(
            "Invalid package name, must be at least {NAME_MIN_LEN} characters, got {} [{}]",
            name.as_ref(),
            name.as_ref().len(),
        )));
    }
    if name.as_ref().len() > NAME_MAX_LEN {
        return Err(InvalidNameError::new_error(format!(
            "Invalid package name, must be no more than {NAME_MAX_LEN} characters, got {} [{}]",
            name.as_ref(),
            name.as_ref().len(),
        )));
    }
    let index = validate_source_str(&name, |c: char| {
        c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'
    });
    if index > -1 {
        let name = name.as_ref();
        let index = index as usize;
        let err_str = format!(
            "{} > {} < {}",
            &name[..index],
            name.chars().nth(index).unwrap(),
            &name[(index + 1)..]
        );
        Err(InvalidNameError::new_error(format!(
            "Invalid package name at pos {}: {}",
            index, err_str
        )))
    } else {
        Ok(())
    }
}

/// Check if a name is a valid pre/post release tag name
pub fn validate_tag_name<S: AsRef<str>>(name: S) -> crate::Result<()> {
    let index = validate_source_str(&name, |c: char| c.is_ascii_alphanumeric());
    if index > -1 {
        let name = name.as_ref();
        let index = index as usize;
        let err_str = format!(
            "{} > {} < {}",
            &name[..index],
            name.chars().nth(index).unwrap(),
            &name[(index + 1)..]
        );
        Err(InvalidNameError::new_error(format!(
            "Invalid release tag name at pos {}: {}",
            index, err_str
        )))
    } else {
        Ok(())
    }
}

/// Returns -1 if valid, or the index of the invalid character
fn validate_source_str<S, V>(source: S, validator: V) -> isize
where
    S: AsRef<str>,
    V: Fn(char) -> bool,
{
    let source = source.as_ref();
    for (i, c) in source.chars().enumerate() {
        if validator(c) {
            continue;
        }
        return i as isize;
    }
    -1
}
