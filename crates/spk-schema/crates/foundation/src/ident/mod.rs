// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

mod error;
mod format;
mod ident_any;
mod ident_build;
mod ident_located;
mod ident_optversion;
mod ident_version;
pub mod parsing;
mod pinnable_request;
mod pinned_request;
mod pkg_request_with_options;
mod range_ident;
mod request_with_options;
mod satisfy;

pub use error::{Error, Result};
pub use ident_any::{AnyIdent, ToAnyIdentWithoutBuild, parse_ident};
pub use ident_build::{BuildIdent, parse_build_ident};
pub use ident_located::{LocatedBuildIdent, LocatedVersionIdent};
pub use ident_optversion::{OptVersionIdent, parse_optversion_ident};
pub use ident_version::{VersionIdent, parse_version_ident};
pub(crate) use pinnable_request::PinValue;
pub use pinnable_request::{
    InclusionPolicy,
    InitialRawRequest,
    NameAndValue,
    PinPolicy,
    PinnableRequest,
    PinnableValue,
    PkgRequest,
    PreReleasePolicy,
    RequestedBy,
    VarRequest,
    is_false,
};
pub use pinned_request::{PinnedRequest, PinnedValue};
pub use pkg_request_with_options::{
    PkgRequestOptionValue,
    PkgRequestOptions,
    PkgRequestWithOptions,
};
pub use range_ident::{RangeIdent, parse_ident_range, parse_ident_range_list};
pub use request_with_options::RequestWithOptions;
pub use satisfy::Satisfy;

pub mod prelude {
    pub use super::Satisfy;
}

use std::marker::PhantomData;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[cfg(test)]
#[path = "./ident_test.rs"]
mod ident_test;

/// Parse a version identifier from a string.
///
/// This will panic if the identifier is wrong,
/// and should only be used for testing.
///
/// ```
/// # #[macro_use]
/// # pub extern crate spk_schema_foundation;
/// # fn main() {
/// version_ident!("my-pkg/1.0.0");
/// # }
/// ```
#[macro_export]
macro_rules! version_ident {
    ($ident:expr) => {
        $crate::ident::parse_version_ident($ident).unwrap()
    };
}

/// Parse a build identifier from a string.
///
/// This will panic if the identifier is wrong,
/// and should only be used for testing.
///
/// ```
/// # #[macro_use]
/// # pub extern crate spk_schema_foundation;
/// # fn main() {
/// build_ident!("my-pkg/1.0.0/src");
/// # }
/// ```
#[macro_export]
macro_rules! build_ident {
    ($ident:expr) => {
        $crate::ident::parse_build_ident($ident).unwrap()
    };
}

/// Identifies a package in some way.
///
/// Every identifier is made up of a base and target,
/// where the full identifier can be roughly represented
/// as `<base>/<target>`. Using this pattern, different
/// identifiers can be defined and composed to identify
/// packages with varying levels of specificity.
///
/// See: [`VersionIdent`], [`BuildIdent`], [`LocatedBuildIdent`]
#[derive(Clone, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub struct Ident<Base, Target> {
    pub(crate) base: Base,
    pub(crate) target: Target,
}

impl<Base, Target> Ident<Base, Target> {
    /// Construct a new identifier from its base and target components
    pub fn new(base: Base, target: Target) -> Self {
        Self { base, target }
    }

    /// Get a reference to this identifier's base (`<base>/<target>`)
    pub fn base(&self) -> &Base {
        &self.base
    }

    /// Get a reference to this identifier's target (`<base>/<target>`)
    pub fn target(&self) -> &Target {
        &self.target
    }

    /// Extract this identifier's base (`<base>/<target>`)
    pub fn into_base(self) -> Base {
        self.base
    }

    /// Extract this identifier's target (`<base>/<target>`)
    pub fn into_target(self) -> Target {
        self.target
    }

    /// Break this identifier into its components (`<base>/<target>`)
    pub fn into_inner(self) -> (Base, Target) {
        (self.base, self.target)
    }

    /// Set the base component of this identifier (`<base>/<target>`)
    pub fn set_base<B: Into<Base>>(&mut self, base: B) -> Base {
        std::mem::replace(&mut self.base, base.into())
    }

    /// Set the target component of this identifier (`<base>/<target>`)
    pub fn set_target<T: Into<Target>>(&mut self, target: T) -> Target {
        std::mem::replace(&mut self.target, target.into())
    }
}

impl<Base, Target> Ident<Base, Target>
where
    Target: Clone,
{
    /// Copy this identifier swapping out the base (`<base>/<target>`)
    pub fn with_base<B: Into<Base>>(&self, base: B) -> Self {
        Self {
            base: base.into(),
            target: self.target.clone(),
        }
    }
}

impl<Base, Target> Ident<Base, Target>
where
    Base: Clone,
{
    /// Copy this identifier swapping out the target (`<base>/<target>`)
    pub fn with_target<T: Into<Target>>(&self, target: T) -> Self {
        Self {
            base: self.base.clone(),
            target: target.into(),
        }
    }
}

impl<Base, Target> AsRef<Base> for Ident<Base, Target> {
    fn as_ref(&self) -> &Base {
        self.base()
    }
}

impl<Base, Target> AsRef<Self> for Ident<Base, Target> {
    fn as_ref(&self) -> &Self {
        self
    }
}

impl<Base, Target> std::fmt::Debug for Ident<Base, Target>
where
    Self: std::fmt::Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // When debugging, we want to see the version string verbatim.
        let ident_string = format!("{:#}", self);
        f.debug_tuple("Ident").field(&ident_string).finish()
    }
}

impl<Base, Target> Serialize for Ident<Base, Target>
where
    Self: std::fmt::Display,
{
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Preserve the original version string as parsed.
        let ident_string = format!("{:#}", self);
        serializer.serialize_str(&ident_string)
    }
}

impl<'de, Base, Target> Deserialize<'de> for Ident<Base, Target>
where
    Self: FromStr,
    <Self as FromStr>::Err: std::fmt::Display,
{
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct IdentVisitor<I>(PhantomData<I>);

        impl<I> serde::de::Visitor<'_> for IdentVisitor<I>
        where
            I: FromStr,
            <I as FromStr>::Err: std::fmt::Display,
        {
            type Value = I;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a package identifier (<NAME>[/<VERSION>[/<BUILD>]])")
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<I, E>
            where
                E: serde::de::Error,
            {
                I::from_str(value).map_err(serde::de::Error::custom)
            }
        }
        deserializer.deserialize_str(IdentVisitor(PhantomData))
    }
}

/// Idents that can be turned into a [`VersionIdent`] can implement this trait.
pub trait AsVersionIdent {
    /// Return a borrowed version of this ident converted into a
    /// [`VersionIdent`].
    fn as_version_ident(&self) -> &VersionIdent;
}

impl<T> AsVersionIdent for Ident<VersionIdent, T> {
    fn as_version_ident(&self) -> &VersionIdent {
        self.base().as_version_ident()
    }
}
