// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::sync::Arc;

pub use paste;

/// Global string interner for common strings.
pub static RODEO: once_cell::sync::Lazy<Arc<lasso::ThreadedRodeo>> =
    once_cell::sync::Lazy::new(|| Arc::new(lasso::ThreadedRodeo::default()));

/// Generate a pair of types to represent a parsed string type.
///
/// A `$type_name::validate()` method must be manually implemented which
/// takes a [`&str`] and validates it.
///
/// ```
/// #[derive(Debug)]
/// pub struct ParseError(&'static str);
///
/// parsedbuf::parsed!(Integer, ParseError);
///
/// impl Integer {
///     fn validate(candidate: &str) -> Result<(), ParseError> {
///         if !candidate.chars().all(|c| c.is_ascii_digit()) {
///             Err(ParseError("expected all digits"))
///         } else {
///             Ok(())
///         }
///     }
/// }
///
/// assert!(matches!(Integer::new("blue"), Err(_)));
/// assert!(matches!(Integer::new("25"), Ok(_)));
/// assert_eq!(Integer::new("25").unwrap(), "25");
/// ```
#[macro_export]
macro_rules! parsed {
    ($type_name:ident, $owned_type_name:ident, $parse_error:ty, $what:tt) => {
        $crate::paste::paste! {
            #[derive(Debug, Hash, Eq, PartialEq, Ord, PartialOrd)]
            #[doc = "A borrowed, immutable, and validated " $what " string"]
            pub struct $type_name(str);
        }

        $crate::paste::paste! {
            #[derive(Debug, Clone, Eq, PartialEq)]
            #[doc = "An owned, mutable, and validated " $what " string"]
            pub struct $owned_type_name(lasso::Spur);
        }

        impl std::hash::Hash for $owned_type_name {
            #[inline]
            fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
                // Hash the interned string, not the Spur, for consistency
                // with hash() on the borrowed type.
                $crate::RODEO.resolve(&self.0).hash(state)
            }
        }

        impl Ord for $owned_type_name {
            #[inline]
            fn cmp(&self, other: &Self) -> std::cmp::Ordering {
                // Order on the interned string, not the Spur, for consistency
                // with Ord on the borrowed type (as required by BTreeMap).
                self.as_str().cmp(other.as_str())
            }
        }

        impl PartialOrd for $owned_type_name {
            #[inline]
            fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
                Some(self.cmp(other))
            }
        }

        #[cfg(feature = "parsedbuf-serde")]
        impl<'de> serde::Deserialize<'de> for $owned_type_name {
            fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
            where
                D: serde::de::Deserializer<'de>,
            {
                let s = String::deserialize(deserializer)?;
                std::str::FromStr::from_str(&s).map_err(serde::de::Error::custom)
            }
        }

        #[cfg(feature = "parsedbuf-serde")]
        impl serde::Serialize for $owned_type_name {
            fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
            where
                S: serde::ser::Serializer,
            {
                serializer.serialize_str(self.as_str())
            }
        }

        impl $type_name {
            pub fn new(inner: &str) -> std::result::Result<&Self, $parse_error> {
                Self::validate(inner).map(|()| unsafe {
                    // Safety: from_str bypasses validation, but
                    // we've just done the validation ourselves
                    Self::from_str(inner)
                })
            }

            #[inline]
            pub fn as_str(&self) -> &str {
                &self.0
            }

            $crate::paste::paste! {
                #[doc = "Wrap a str as a `" $type_name "`"]
                #[doc = ""]
                #[doc = "# Safety:"]
                #[doc = ""]
                #[doc = "This function bypasses validation and should not be used"]
                #[doc = "unless the given argument is known to be valid"]
                pub(crate) const unsafe fn from_str(inner: &str) -> &Self {
                    unsafe { &*(inner as *const str as *const $type_name) }
                }
            }

            #[inline]
            pub fn is_empty(&self) -> bool {
                self.0.is_empty()
            }

            #[inline]
            pub fn len(&self) -> usize {
                self.0.len()
            }
        }

        // Allow tests to manufacture owned instances with known good values.
        #[allow(dead_code)]
        impl $owned_type_name {
            $crate::paste::paste! {
                #[doc = "Create a [`" $owned_type_name "`] from a [`String`]"]
                #[doc = ""]
                #[doc = "# Safety"]
                #[doc = ""]
                #[doc = "No validation is performed on `name`."]
                pub unsafe fn from_string(name: String) -> Self {
                    let key = $crate::RODEO.try_get_or_intern(name).expect("won't run out of intern slots");
                    Self(key)
                }
            }
        }

        impl std::borrow::Borrow<$type_name> for $owned_type_name {
            #[inline]
            fn borrow(&self) -> &$type_name {
                self.as_ref()
            }
        }

        impl std::borrow::Borrow<str> for $owned_type_name {
            #[inline]
            fn borrow(&self) -> &str {
                $crate::RODEO.resolve(&self.0)
            }
        }

        impl std::borrow::ToOwned for $type_name {
            type Owned = $owned_type_name;

            fn to_owned(&self) -> Self::Owned {
                let key = $crate::RODEO.try_get_or_intern(&self.0).expect("won't run out of intern slots");
                $owned_type_name(key)
            }
        }

        impl std::cmp::PartialEq<$type_name> for $owned_type_name {
            #[inline]
            fn eq(&self, other: &$type_name) -> bool {
                self.as_str() == &other.0
            }
        }

        impl std::cmp::PartialEq<$owned_type_name> for $type_name {
            #[inline]
            fn eq(&self, other: &$owned_type_name) -> bool {
                &self.0 == other.as_str()
            }
        }

        impl std::cmp::PartialEq<$owned_type_name> for &$type_name {
            #[inline]
            fn eq(&self, other: &$owned_type_name) -> bool {
                &self.0 == other.as_str()
            }
        }

        impl std::cmp::PartialEq<str> for $type_name {
            #[inline]
            fn eq(&self, other: &str) -> bool {
                &self.0 == other
            }
        }

        impl std::cmp::PartialEq<str> for $owned_type_name {
            #[inline]
            fn eq(&self, other: &str) -> bool {
                self.as_str() == other
            }
        }

        impl std::convert::AsRef<$type_name> for $type_name {
            #[inline]
            fn as_ref(&self) -> &$type_name {
                self
            }
        }

        impl std::convert::AsRef<$type_name> for $owned_type_name {
            #[inline]
            fn as_ref(&self) -> &$type_name {
                // Safety: from_str bypasses validation but the contents
                // of owned instance must already be valid
                unsafe { $type_name::from_str($crate::RODEO.resolve(&self.0)) }
            }
        }

        impl std::convert::AsRef<std::ffi::OsStr> for $type_name {
            #[inline]
            fn as_ref(&self) -> &std::ffi::OsStr {
                std::ffi::OsStr::new(&self.0)
            }
        }

        impl std::convert::AsRef<std::path::Path> for $type_name {
            #[inline]
            fn as_ref(&self) -> &std::path::Path {
                std::path::Path::new(&self.0)
            }
        }

        impl std::convert::AsRef<std::path::Path> for $owned_type_name {
            #[inline]
            fn as_ref(&self) -> &std::path::Path {
                std::path::Path::new($crate::RODEO.resolve(&self.0))
            }
        }

        impl std::convert::AsRef<str> for $owned_type_name {
            #[inline]
            fn as_ref(&self) -> &str {
                $crate::RODEO.resolve(&self.0)
            }
        }

        impl std::cmp::PartialEq<&str> for $type_name {
            #[inline]
            fn eq(&self, other: &&str) -> bool {
                &self.0 == *other
            }
        }

        impl std::cmp::PartialEq<&str> for $owned_type_name {
            #[inline]
            fn eq(&self, other: &&str) -> bool {
                self.as_str() == *other
            }
        }

        impl std::convert::From<&$type_name> for $owned_type_name {
            #[inline]
            fn from(name: &$type_name) -> Self {
                name.to_owned()
            }
        }

        impl std::convert::From<$owned_type_name> for String {
            #[inline]
            fn from(val: $owned_type_name) -> Self {
                $crate::RODEO.resolve(&val.0).to_string()
            }
        }

        impl std::convert::TryFrom<&str> for $owned_type_name {
            type Error = $parse_error;

            fn try_from(s: &str) -> std::result::Result<Self, Self::Error> {
                s.parse()
            }
        }

        impl std::ops::Deref for $type_name {
            type Target = str;

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl std::ops::Deref for $owned_type_name {
            type Target = $type_name;

            fn deref(&self) -> &Self::Target {
                self.as_ref()
            }
        }

        impl TryFrom<String> for $owned_type_name {
            type Error = $parse_error;

            fn try_from(s: String) -> std::result::Result<Self, Self::Error> {
                $type_name::new(&s).map(ToOwned::to_owned)
            }
        }

        impl std::fmt::Display for $type_name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }

        impl std::fmt::Display for $owned_type_name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                $crate::RODEO.resolve(&self.0).fmt(f)
            }
        }

        impl std::str::FromStr for $owned_type_name {
            type Err = $parse_error;

            fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
                $type_name::new(&s).map(std::borrow::ToOwned::to_owned)
            }
        }
    };
    ($type_name:ident, $parse_error:ty, $what:tt) => {
        $crate::paste::paste! {
            $crate::parsed!($type_name, [<$type_name Buf>], $parse_error, $what);
        }
    };
    ($type_name:ident, $parse_error:ty) => {
        $crate::parsed!($type_name, $parse_error, $type_name);
    };
}
