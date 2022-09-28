// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

mod error;
pub mod parsing;

use std::borrow::Borrow;
use std::convert::TryFrom;

pub use error::{Error, Result};
use thiserror::Error;

#[cfg(test)]
#[path = "./name_test.rs"]
mod name_test;

/// Parse a package name from a string.
///
/// This will panic if the name is invalid,
/// and should only be used for testing.
///
/// ```
/// # #[macro_use] extern crate spk_schema_foundation;
/// # fn main() {
/// pkg_name!("my-pkg");
/// # }
/// ```
#[macro_export]
macro_rules! pkg_name {
    ($name:literal) => {
        $crate::name::PkgName::new($name).unwrap()
    };
}

/// Parse an option name from a string.
///
/// This will panic if the name is invalid,
/// and should only be used for testing.
///
/// ```
/// # #[macro_use] extern crate spk_schema_foundation;
/// # fn main() {
/// opt_name!("my_option");
/// opt_name!("python.abi");
/// # }
/// ```
#[macro_export]
macro_rules! opt_name {
    ($name:literal) => {
        $crate::name::OptName::new($name).unwrap()
    };
}

/// Denotes that an invalid package name was given.
#[derive(Debug, Error)]
#[error("Invalid name: {message}")]
pub struct InvalidNameError {
    pub message: String,
}

impl InvalidNameError {
    pub fn new_error(msg: String) -> Error {
        Error::InvalidNameError(Self { message: msg })
    }
}

parsedbuf::parsed!(OptName, Error, "option");
parsedbuf::parsed!(PkgName, Error, "package");
parsedbuf::parsed!(RepositoryName, Error, "repository");

impl Borrow<OptName> for PkgName {
    fn borrow(&self) -> &OptName {
        self.as_ref()
    }
}

impl Borrow<OptName> for PkgNameBuf {
    fn borrow(&self) -> &OptName {
        self.as_opt_name()
    }
}

impl PkgName {
    pub const MIN_LEN: usize = 2;
    pub const MAX_LEN: usize = 64;

    /// Validate the given string as a package name
    pub fn validate<S: AsRef<str> + ?Sized>(s: &S) -> Result<()> {
        validate_pkg_name(s)
    }

    /// Interpret this package name as an option name
    pub fn as_opt_name(&self) -> &OptName {
        self.borrow()
    }
}

impl AsRef<OptName> for PkgName {
    fn as_ref(&self) -> &OptName {
        // Safety: from_str skips validation, but we assume (and hopefully test)
        // that the set of all packages names is a subset of all option names
        unsafe { OptName::from_str(&self.0) }
    }
}

/// Ensure that the provided string is a valid package name
fn validate_pkg_name<S: AsRef<str>>(name: S) -> Result<()> {
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
    if let Some('-') = name.as_ref().chars().next() {
        return Err(InvalidNameError::new_error(format!(
            "Invalid package name, must begin with a letter and not a hyphen, got {}",
            name.as_ref()
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

    /// Validate the given string as an option name
    pub fn validate<S: AsRef<str> + ?Sized>(s: &S) -> Result<()> {
        validate_opt_name(s)
    }

    /// The non-namespace portion of this option. To get an [`OptName`]
    /// with any leading namespace removed, use [`Self::without_namespace`].
    ///
    /// ```
    /// # #[macro_use] extern crate spk_schema_foundation;
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
}

/// Ensure that the provided string is a valid option name.
///
/// This is for checking option names with or without any leading
/// package namespace.
fn validate_opt_name<S: AsRef<str>>(name: S) -> Result<()> {
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
fn validate_opt_base_name<S: AsRef<str>>(name: S) -> Result<()> {
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
pub fn validate_tag_name<S: AsRef<str>>(name: S) -> Result<()> {
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

impl RepositoryName {
    /// Validate the given string as a repository name
    pub fn validate<S: AsRef<str> + ?Sized>(s: &S) -> Result<()> {
        // Using the same validation strategy as package names.
        validate_pkg_name(s)
    }
}

impl RepositoryNameBuf {
    /// Return if this RepositoryName names the "local" repository
    pub fn is_local(&self) -> bool {
        self.0 == "local"
    }
}
