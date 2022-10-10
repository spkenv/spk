// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

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
/// # pub extern crate spk_schema_ident;
/// # fn main() {
/// version_ident!("my-pkg/1.0.0");
/// # }
/// ```
#[macro_export]
macro_rules! version_ident {
    ($ident:expr) => {
        $crate::parse_version_ident($ident).unwrap()
    };
}

/// Parse a build identifier from a string.
///
/// This will panic if the identifier is wrong,
/// and should only be used for testing.
///
/// ```
/// # #[macro_use]
/// # pub extern crate spk_schema_ident;
/// # fn main() {
/// build_ident!("my-pkg/1.0.0/src");
/// # }
/// ```
#[macro_export]
macro_rules! build_ident {
    ($ident:expr) => {
        $crate::parse_build_ident($ident).unwrap()
    };
}

/// Identifies a package in some way.
///
/// Every identifier is made up of a base and target,
/// where the full identifier can be roughly represented
/// as <base>/<target>. Using this pattern, different
/// identifiers can be defined and composed to identify
/// packages with varying levels of specificity.
///
/// See: [`super::VersionIdent`], [`super::BuildIdent`], [`super::LocatedBuildIdent`]
#[derive(Clone, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub struct Ident<Base, Target> {
    pub(crate) base: Base,
    pub(crate) target: Target,
}

impl<Base, Target> Ident<Base, Target> {
    pub fn new(base: Base, target: Target) -> Self {
        Self { base, target }
    }

    pub fn base(&self) -> &Base {
        &self.base
    }

    pub fn target(&self) -> &Target {
        &self.target
    }

    pub fn into_base(self) -> Base {
        self.base
    }

    pub fn into_target(self) -> Target {
        self.target
    }

    pub fn into_inner(self) -> (Base, Target) {
        (self.base, self.target)
    }

    pub fn with_base<B: Into<Base>>(mut self, base: B) -> Self {
        self.set_base(base);
        self
    }

    pub fn with_target<T: Into<Target>>(mut self, target: T) -> Self {
        self.set_target(target);
        self
    }

    pub fn set_base<B: Into<Base>>(&mut self, base: B) -> Base {
        std::mem::replace(&mut self.base, base.into())
    }

    pub fn set_target<T: Into<Target>>(&mut self, target: T) -> Target {
        std::mem::replace(&mut self.target, target.into())
    }
}

impl<Base, T> std::ops::Deref for Ident<Base, T> {
    type Target = Base;

    fn deref(&self) -> &Self::Target {
        &self.base
    }
}

impl<Base, T> std::ops::DerefMut for Ident<Base, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.base
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
        f.debug_tuple("Ident").field(&self.to_string()).finish()
    }
}

// impl From<PkgNameBuf> for Ident {
//     fn from(n: PkgNameBuf) -> Self {
//         Self::new(n)
//     }
// }

// impl TryFrom<&str> for Ident {
//     type Error = crate::Error;

//     fn try_from(value: &str) -> Result<Self> {
//         Self::from_str(value)
//     }
// }

// impl TryFrom<&String> for Ident {
//     type Error = crate::Error;

//     fn try_from(value: &String) -> Result<Self> {
//         Self::from_str(value.as_str())
//     }
// }

// impl TryFrom<String> for Ident {
//     type Error = crate::Error;

//     fn try_from(value: String) -> Result<Self> {
//         Self::from_str(value.as_str())
//     }
// }

impl<Base, Target> Serialize for Ident<Base, Target>
where
    Self: std::fmt::Display,
{
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
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

        impl<'de, I> serde::de::Visitor<'de> for IdentVisitor<I>
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
