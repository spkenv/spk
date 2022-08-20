// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

mod error;
pub mod parsing;

pub use error::{Error, Result};

use std::{borrow::Borrow, convert::TryFrom};

use paste::paste;
use serde::{Deserialize, Serialize};
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

/// Generate a pair of types to represent a name.
///
/// A `$typ_name::new()` method must be manually implemented.
macro_rules! name {
    ($typ_name:ident, $owned_typ_name:ident, $comment:tt) => {
        paste! {
            #[derive(Debug, Hash, Eq, PartialEq, Ord, PartialOrd)]
            #[doc = "A borrowed " $comment " name"]
            pub struct $typ_name(str);
        }

        paste! {
            #[derive(Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd, Deserialize, Serialize)]
            #[doc = "An owned, mutable " $comment " name"]
            pub struct $owned_typ_name(String);
        }

        impl $typ_name {
            pub fn as_str(&self) -> &str {
                &self.0
            }

            paste! {
                #[doc = "Wrap a str as a `" $typ_name "`"]
                #[doc = ""]
                #[doc = "# Safety:"]
                #[doc = ""]
                #[doc = "This function bypasses validation and should not be used"]
                #[doc = "unless the given argument is known to be valid"]
                pub(crate) const unsafe fn from_str(inner: &str) -> &Self {
                    unsafe { &*(inner as *const str as *const $typ_name) }
                }
            }

            pub fn is_empty(&self) -> bool {
                self.0.is_empty()
            }

            pub fn len(&self) -> usize {
                self.0.len()
            }
        }

        // Allow tests to manufacture owned instances with known good values.
        #[allow(dead_code)]
        impl $owned_typ_name {
            paste! {
                #[doc = "Create a `" $owned_typ_name "` from a `String`"]
                #[doc = ""]
                #[doc = "# Safety"]
                #[doc = ""]
                #[doc = "No validation is performed on `name`."]
                pub unsafe fn from_string(name: String) -> Self {
                    Self(name)
                }
            }

            paste! {
                #[doc = "Consume the `" $owned_typ_name "`, returning the inner `String`."]
                pub fn into_inner(self) -> String {
                    self.0
                }
            }
        }

        impl std::borrow::Borrow<$typ_name> for $owned_typ_name {
            fn borrow(&self) -> &$typ_name {
                self.as_ref()
            }
        }

        impl std::borrow::Borrow<String> for $owned_typ_name {
            fn borrow(&self) -> &String {
                &self.0
            }
        }

        impl std::borrow::ToOwned for $typ_name {
            type Owned = $owned_typ_name;

            fn to_owned(&self) -> Self::Owned {
                $owned_typ_name(self.0.to_owned())
            }
        }

        impl std::cmp::PartialEq<$typ_name> for $owned_typ_name {
            fn eq(&self, other: &$typ_name) -> bool {
                &**self == other
            }
        }

        impl std::cmp::PartialEq<$owned_typ_name> for $typ_name {
            fn eq(&self, other: &$owned_typ_name) -> bool {
                &self.0 == other.as_str()
            }
        }

        impl std::cmp::PartialEq<$owned_typ_name> for &$typ_name {
            fn eq(&self, other: &$owned_typ_name) -> bool {
                &self.0 == other.as_str()
            }
        }

        impl std::cmp::PartialEq<str> for $typ_name {
            fn eq(&self, other: &str) -> bool {
                self.as_str() == other
            }
        }

        impl std::cmp::PartialEq<str> for $owned_typ_name {
            fn eq(&self, other: &str) -> bool {
                &**self == other
            }
        }

        impl std::convert::AsRef<$typ_name> for $typ_name {
            fn as_ref(&self) -> &$typ_name {
                self
            }
        }

        impl std::convert::AsRef<$typ_name> for $owned_typ_name {
            fn as_ref(&self) -> &$typ_name {
                // Safety: from_str bypasses validation but the contents
                // of owned instance must already be valid
                unsafe { $typ_name::from_str(&self.0) }
            }
        }

        impl std::convert::AsRef<std::ffi::OsStr> for $typ_name {
            fn as_ref(&self) -> &std::ffi::OsStr {
                std::ffi::OsStr::new(&self.0)
            }
        }

        impl std::convert::AsRef<std::path::Path> for $typ_name {
            fn as_ref(&self) -> &std::path::Path {
                std::path::Path::new(&self.0)
            }
        }

        impl std::convert::AsRef<std::path::Path> for $owned_typ_name {
            fn as_ref(&self) -> &std::path::Path {
                std::path::Path::new(&self.0)
            }
        }

        impl std::convert::AsRef<str> for $owned_typ_name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl std::convert::From<&$typ_name> for $owned_typ_name {
            fn from(name: &$typ_name) -> Self {
                name.to_owned()
            }
        }

        impl std::convert::From<$owned_typ_name> for String {
            fn from(val: $owned_typ_name) -> Self {
                val.0
            }
        }

        impl std::convert::TryFrom<&str> for $owned_typ_name {
            type Error = $crate::name::Error;

            fn try_from(s: &str) -> Result<Self> {
                s.parse()
            }
        }

        impl std::ops::Deref for $typ_name {
            type Target = str;

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl std::ops::Deref for $owned_typ_name {
            type Target = $typ_name;

            fn deref(&self) -> &Self::Target {
                self.as_ref()
            }
        }

        impl std::fmt::Display for $typ_name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }

        impl std::fmt::Display for $owned_typ_name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }

        impl std::str::FromStr for $owned_typ_name {
            type Err = $crate::name::Error;

            fn from_str(s: &str) -> Result<Self> {
                $typ_name::new(&s).map(std::borrow::ToOwned::to_owned)
            }
        }
    };
    ($typ_name:ident, $comment:tt) => {
        paste! {
            name!($typ_name, [<$typ_name Buf>], $comment);
        }
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

name!(OptName, "option");
name!(PkgName, "package");
name!(RepositoryName, "repository");

impl TryFrom<String> for PkgNameBuf {
    type Error = Error;

    fn try_from(s: String) -> Result<Self> {
        validate_pkg_name(&s)?;
        Ok(Self(s))
    }
}

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

    pub fn new<S: AsRef<str> + ?Sized>(s: &S) -> Result<&PkgName> {
        validate_pkg_name(s)?;
        // Safety: from_str bypasses validation but we've just done that
        Ok(unsafe { Self::from_str(s.as_ref()) })
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

impl TryFrom<String> for OptNameBuf {
    type Error = Error;

    fn try_from(s: String) -> Result<Self> {
        validate_opt_name(&s)?;
        Ok(Self(s))
    }
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
    pub fn new<S: AsRef<str> + ?Sized>(s: &S) -> Result<&RepositoryName> {
        // Using the same validation strategy as package names.
        validate_pkg_name(s)?;
        // Safety: from_str bypasses validation but we've just done that
        Ok(unsafe { Self::from_str(s.as_ref()) })
    }
}

impl RepositoryNameBuf {
    /// Return if this RepositoryName names the "local" repository
    pub fn is_local(&self) -> bool {
        self.0 == "local"
    }
}
