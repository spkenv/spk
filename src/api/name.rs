// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::{borrow::Borrow, convert::TryFrom, str::FromStr};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::Result;

#[cfg(test)]
#[path = "./name_test.rs"]
mod name_test;

/// Parse a package name from a string.
///
/// This will panic if the name is invalid,
/// and should only be used for testing.
///
/// ```
/// # #[macro_use] extern crate spk;
/// # fn main() {
/// pkg_name!("my-pkg");
/// # }
/// ```
#[macro_export]
macro_rules! pkg_name {
    ($name:literal) => {
        $crate::api::PkgName::new($name).unwrap()
    };
}

/// Parse an option name from a string.
///
/// This will panic if the name is invalid,
/// and should only be used for testing.
///
/// ```
/// # #[macro_use] extern crate spk;
/// # fn main() {
/// opt_name!("my_option");
/// opt_name!("python.abi");
/// # }
/// ```
#[macro_export]
macro_rules! opt_name {
    ($name:literal) => {
        $crate::api::OptName::new($name).unwrap()
    };
}

/// Denotes that an invalid package name was given.
#[derive(Debug, Error)]
#[error("Invalid name: {message}")]
pub struct InvalidNameError {
    pub message: String,
}

impl InvalidNameError {
    pub fn new_error(msg: String) -> crate::Error {
        crate::Error::InvalidNameError(Self { message: msg })
    }
}

/// An owned, mutable package name
#[derive(Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd, Deserialize, Serialize)]
pub struct PkgNameBuf(String);

impl std::ops::Deref for PkgNameBuf {
    type Target = PkgName;

    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

impl AsRef<str> for PkgNameBuf {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl AsRef<PkgName> for PkgNameBuf {
    fn as_ref(&self) -> &PkgName {
        // Safety: from_str bypasses validation but the contents
        // of PkgNameBuf must already be valid
        unsafe { PkgName::from_str(&self.0) }
    }
}

impl std::fmt::Display for PkgNameBuf {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl AsRef<std::path::Path> for PkgNameBuf {
    fn as_ref(&self) -> &std::path::Path {
        std::path::Path::new(&self.0)
    }
}

impl Borrow<String> for PkgNameBuf {
    fn borrow(&self) -> &String {
        &self.0
    }
}

impl From<PkgNameBuf> for String {
    fn from(val: PkgNameBuf) -> Self {
        val.0
    }
}

impl FromStr for PkgNameBuf {
    type Err = crate::Error;

    fn from_str(s: &str) -> Result<Self> {
        PkgName::new(&s).map(ToOwned::to_owned)
    }
}

impl TryFrom<&str> for PkgNameBuf {
    type Error = crate::Error;

    fn try_from(s: &str) -> Result<Self> {
        s.parse()
    }
}

impl TryFrom<String> for PkgNameBuf {
    type Error = crate::Error;

    fn try_from(s: String) -> Result<Self> {
        validate_pkg_name(&s)?;
        Ok(Self(s))
    }
}

impl Borrow<PkgName> for PkgNameBuf {
    fn borrow(&self) -> &PkgName {
        self.as_ref()
    }
}

impl std::cmp::PartialEq<PkgName> for PkgNameBuf {
    fn eq(&self, other: &PkgName) -> bool {
        &**self == other
    }
}

impl From<&PkgName> for PkgNameBuf {
    fn from(name: &PkgName) -> Self {
        name.to_owned()
    }
}

impl Borrow<OptName> for PkgNameBuf {
    fn borrow(&self) -> &OptName {
        self.as_opt_name()
    }
}

// Allow tests to manufacture `PkgNameBuf`s with known good values.
#[cfg(test)]
impl PkgNameBuf {
    /// Create a `PkgNameBuf` from a `String`
    ///
    /// # Safety
    ///
    /// No validation is performed on `name`.
    pub(crate) unsafe fn from_string(name: String) -> Self {
        Self(name)
    }

    /// Consume the `PkgNameBuf`, returning the inner `String`.
    pub(crate) fn into_inner(self) -> String {
        self.0
    }
}

/// A valid package name
#[derive(Debug, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct PkgName(str);

impl PkgName {
    pub(crate) const MIN_LEN: usize = 2;
    pub(crate) const MAX_LEN: usize = 64;

    /// Wrap a str as a PkgName
    ///
    /// # Safety:
    ///
    /// This function bypasses validation and should not be used
    /// unless the given argument is known to be valid
    pub(crate) const unsafe fn from_str(inner: &str) -> &Self {
        unsafe { &*(inner as *const str as *const PkgName) }
    }

    pub fn new<S: AsRef<str> + ?Sized>(s: &S) -> Result<&PkgName> {
        validate_pkg_name(s)?;
        // Safety: from_str bypasses validation but we've just done that
        Ok(unsafe { Self::from_str(s.as_ref()) })
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        false // names are not allowed to be empty
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Interpret this package name as an option name
    pub fn as_opt_name(&self) -> &OptName {
        self.borrow()
    }
}

impl std::ops::Deref for PkgName {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl std::fmt::Display for PkgName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl AsRef<PkgName> for PkgName {
    fn as_ref(&self) -> &PkgName {
        self
    }
}

impl AsRef<OptName> for PkgName {
    fn as_ref(&self) -> &OptName {
        // Safety: from_str skips validation, but we assume (and hopefully test)
        // that the set of all packages names is a subset of all option names
        unsafe { OptName::from_str(&self.0) }
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

impl Borrow<OptName> for PkgName {
    fn borrow(&self) -> &OptName {
        self.as_ref()
    }
}

impl ToOwned for PkgName {
    type Owned = PkgNameBuf;

    fn to_owned(&self) -> Self::Owned {
        PkgNameBuf(self.0.to_owned())
    }
}

impl std::cmp::PartialEq<PkgNameBuf> for PkgName {
    fn eq(&self, other: &PkgNameBuf) -> bool {
        self == &**other
    }
}

impl std::cmp::PartialEq<str> for PkgName {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

/// Ensure that the provided string is a valid package name
fn validate_pkg_name<S: AsRef<str>>(name: S) -> crate::Result<()> {
    if name.as_ref().len() < PkgName::MIN_LEN {
        return Err(InvalidNameError::new_error(format!(
            "Invalid package name, must be at least {} characters, got {} [{}]",
            PkgName::MIN_LEN,
            name.as_ref(),
            name.as_ref().len(),
        )));
    }
    if name.as_ref().len() > PkgName::MAX_LEN {
        return Err(InvalidNameError::new_error(format!(
            "Invalid package name, must be no more than {} characters, got {} [{}]",
            PkgName::MAX_LEN,
            name.as_ref(),
            name.as_ref().len(),
        )));
    }
    let index = validate_source_str(&name, is_valid_pkg_name_char);
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

fn is_valid_pkg_name_char(c: char) -> bool {
    c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'
}

/// An owned, mutable package name
#[derive(Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd, Deserialize, Serialize)]
pub struct OptNameBuf(String);

impl std::ops::Deref for OptNameBuf {
    type Target = OptName;

    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

impl AsRef<str> for OptNameBuf {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl AsRef<OptName> for OptNameBuf {
    fn as_ref(&self) -> &OptName {
        // Safety: from_str skips validation but we've already done that
        unsafe { OptName::from_str(&self.0) }
    }
}

impl std::fmt::Display for OptNameBuf {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl AsRef<std::path::Path> for OptNameBuf {
    fn as_ref(&self) -> &std::path::Path {
        std::path::Path::new(&self.0)
    }
}

impl Borrow<String> for OptNameBuf {
    fn borrow(&self) -> &String {
        &self.0
    }
}

impl From<OptNameBuf> for String {
    fn from(val: OptNameBuf) -> Self {
        val.0
    }
}

impl FromStr for OptNameBuf {
    type Err = crate::Error;

    fn from_str(s: &str) -> Result<Self> {
        OptName::new(s).map(ToOwned::to_owned)
    }
}

impl TryFrom<&str> for OptNameBuf {
    type Error = crate::Error;

    fn try_from(s: &str) -> Result<Self> {
        s.parse()
    }
}

impl TryFrom<String> for OptNameBuf {
    type Error = crate::Error;

    fn try_from(s: String) -> Result<Self> {
        validate_opt_name(&s)?;
        Ok(Self(s))
    }
}

impl Borrow<OptName> for OptNameBuf {
    fn borrow(&self) -> &OptName {
        self.as_ref()
    }
}

impl std::cmp::PartialEq<OptName> for OptNameBuf {
    fn eq(&self, other: &OptName) -> bool {
        &**self == other
    }
}

impl From<&OptName> for OptNameBuf {
    fn from(name: &OptName) -> Self {
        name.to_owned()
    }
}

/// A valid package name
#[derive(Debug, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct OptName(str);

impl OptName {
    const SEP: char = '.';
    // all valid package names are assumed to/must be
    // valid option names, so options are constrained
    // by the same size limits
    const MIN_LEN: usize = PkgName::MIN_LEN;
    const MAX_LEN: usize = PkgName::MAX_LEN;

    /// Standard option used to identify the operating system
    pub const fn os() -> &'static Self {
        // Safety: from_str skips validation, but this is a known good value
        unsafe { Self::from_str("os") }
    }

    /// Standard option used to identify the target architecture
    pub const fn arch() -> &'static Self {
        // Safety: from_str skips validation, but this is a known good value
        unsafe { Self::from_str("arch") }
    }

    /// Standard option used to identify the os distribution
    pub const fn distro() -> &'static Self {
        // Safety: from_str skips validation, but this is a known good value
        unsafe { Self::from_str("distro") }
    }

    /// Wrap the given string as an OptName
    ///
    /// # Safety:
    /// This function does not check that the provided string
    /// is a valid optname and should only be called for known
    /// valid values.
    const unsafe fn from_str(inner: &str) -> &Self {
        unsafe { &*(inner as *const str as *const OptName) }
    }

    /// Validate and wrap the given string as an OptName.
    pub fn new<S: AsRef<str> + ?Sized>(s: &S) -> Result<&OptName> {
        validate_opt_name(s)?;
        // Safety: from_str skips validation but we've just done that
        Ok(unsafe { Self::from_str(s.as_ref()) })
    }

    /// The non-namespace portion of this option. To get an [`OptName`]
    /// with any leading namespace removed, use [`Self::without_namespace`].
    ///
    /// ```
    /// # #[macro_use] extern crate spk;
    /// # fn main() {
    /// assert_eq!(opt_name!("my_option").base_name(), "my_option");
    /// assert_eq!(opt_name!("python.abi").base_name(), "abi");
    /// # }
    /// ```
    pub fn base_name(&self) -> &str {
        self.split_once(Self::SEP)
            .map(|(_, n)| n)
            .unwrap_or(&self.0)
    }

    /// The package namespace defined in this option, if any
    pub fn namespace(&self) -> Option<&PkgName> {
        self.0
            .split_once(Self::SEP)
            // Safety: from_str skips validation, but we've already validated
            // the namespace as a package name in [`Self::new`] if it is set
            .map(|(ns, _)| unsafe { PkgName::from_str(ns) })
    }

    /// Return a copy of this option, adding the provided namespace if there isn't already one set
    pub fn with_default_namespace<N: AsRef<PkgName>>(&self, ns: N) -> OptNameBuf {
        OptNameBuf(format!(
            "{}{}{}",
            self.namespace().unwrap_or_else(|| ns.as_ref()),
            Self::SEP,
            self.base_name()
        ))
    }

    /// Return a copy of this option, replacing any namespace with the provided one
    pub fn with_namespace<N: AsRef<PkgName>>(&self, ns: N) -> OptNameBuf {
        OptNameBuf(format!("{}{}{}", ns.as_ref(), Self::SEP, self.base_name()))
    }

    /// Return an option with the same name but no associated namespace
    pub fn without_namespace(&self) -> &OptName {
        // Safety: from_str skips validation, but the base name of
        // any option is also a valid option, it simply doesn't have a namespace
        unsafe { Self::from_str(self.base_name()) }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        false // names are not allowed to be empty
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl std::ops::Deref for OptName {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::fmt::Display for OptName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl AsRef<str> for OptName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl AsRef<std::path::Path> for OptName {
    fn as_ref(&self) -> &std::path::Path {
        std::path::Path::new(&self.0)
    }
}

impl AsRef<std::ffi::OsStr> for OptName {
    fn as_ref(&self) -> &std::ffi::OsStr {
        std::ffi::OsStr::new(&self.0)
    }
}

impl ToOwned for OptName {
    type Owned = OptNameBuf;

    fn to_owned(&self) -> Self::Owned {
        OptNameBuf(self.0.to_owned())
    }
}

impl std::cmp::PartialEq<OptNameBuf> for OptName {
    fn eq(&self, other: &OptNameBuf) -> bool {
        self == &**other
    }
}

impl std::cmp::PartialEq<str> for OptName {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

/// Ensure that the provided string is a valid option name.
///
/// This is for checking option names with or without any leading
/// package namespace.
fn validate_opt_name<S: AsRef<str>>(name: S) -> crate::Result<()> {
    match name.as_ref().split_once(OptName::SEP) {
        Some((ns, opt)) => {
            validate_pkg_name(ns)?;
            validate_opt_base_name(opt)
        }
        None => validate_opt_base_name(name),
    }
}

/// Ensure that the provided string is a valid option name.
///
/// This is for checking option names without any leading
/// package specifier. Complete option names can be validated
/// with [`validate_opt_name`], or leading package names can
/// be validated separately with [`validate_pkg_name`].
fn validate_opt_base_name<S: AsRef<str>>(name: S) -> crate::Result<()> {
    if name.as_ref().len() < OptName::MIN_LEN {
        return Err(InvalidNameError::new_error(format!(
            "Invalid option name, must be at least {} characters, got {} [{}]",
            OptName::MIN_LEN,
            name.as_ref(),
            name.as_ref().len(),
        )));
    }
    if name.as_ref().len() > OptName::MAX_LEN {
        return Err(InvalidNameError::new_error(format!(
            "Invalid option name, must be no more than {} characters, got {} [{}]",
            OptName::MAX_LEN,
            name.as_ref(),
            name.as_ref().len(),
        )));
    }
    let index = validate_source_str(&name, is_valid_opt_name_char);
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
            "Invalid option name at pos {}: {}",
            index, err_str
        )))
    } else {
        Ok(())
    }
}

fn is_valid_opt_name_char(c: char) -> bool {
    // option names are a superset of all valid package names
    is_valid_pkg_name_char(c) || c == '_'
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
