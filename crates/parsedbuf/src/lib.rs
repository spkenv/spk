// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

#![deny(unsafe_op_in_unsafe_fn)]
#![warn(clippy::fn_params_excessive_bools)]

pub use paste;

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
            #[derive(Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
            #[doc = "An owned, mutable, and validated " $what " string"]
            pub struct $owned_type_name(String);
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

            pub fn is_empty(&self) -> bool {
                self.0.is_empty()
            }

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
                    Self(name)
                }
            }

            $crate::paste::paste! {
                #[doc = "Consume the [`" $owned_type_name "`], returning the inner [`String`]."]
                pub fn into_inner(self) -> String {
                    self.0
                }
            }
        }

        impl std::borrow::Borrow<$type_name> for $owned_type_name {
            fn borrow(&self) -> &$type_name {
                self.as_ref()
            }
        }

        impl std::borrow::Borrow<String> for $owned_type_name {
            fn borrow(&self) -> &String {
                &self.0
            }
        }

        impl std::borrow::ToOwned for $type_name {
            type Owned = $owned_type_name;

            fn to_owned(&self) -> Self::Owned {
                $owned_type_name(self.0.to_owned())
            }
        }

        impl std::cmp::PartialEq<$type_name> for $owned_type_name {
            fn eq(&self, other: &$type_name) -> bool {
                &**self == other
            }
        }

        impl std::cmp::PartialEq<$owned_type_name> for $type_name {
            fn eq(&self, other: &$owned_type_name) -> bool {
                &self.0 == other.as_str()
            }
        }

        impl std::cmp::PartialEq<$owned_type_name> for &$type_name {
            fn eq(&self, other: &$owned_type_name) -> bool {
                &self.0 == other.as_str()
            }
        }

        impl std::cmp::PartialEq<str> for $type_name {
            fn eq(&self, other: &str) -> bool {
                self.as_str() == other
            }
        }

        impl std::cmp::PartialEq<str> for $owned_type_name {
            fn eq(&self, other: &str) -> bool {
                &**self == other
            }
        }

        impl std::convert::AsRef<$type_name> for $type_name {
            fn as_ref(&self) -> &$type_name {
                self
            }
        }

        impl std::convert::AsRef<$type_name> for $owned_type_name {
            fn as_ref(&self) -> &$type_name {
                // Safety: from_str bypasses validation but the contents
                // of owned instance must already be valid
                unsafe { $type_name::from_str(&self.0) }
            }
        }

        impl std::convert::AsRef<std::ffi::OsStr> for $type_name {
            fn as_ref(&self) -> &std::ffi::OsStr {
                std::ffi::OsStr::new(&self.0)
            }
        }

        impl std::convert::AsRef<std::path::Path> for $type_name {
            fn as_ref(&self) -> &std::path::Path {
                std::path::Path::new(&self.0)
            }
        }

        impl std::convert::AsRef<std::path::Path> for $owned_type_name {
            fn as_ref(&self) -> &std::path::Path {
                std::path::Path::new(&self.0)
            }
        }

        impl std::convert::AsRef<str> for $owned_type_name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl std::convert::From<&$type_name> for $owned_type_name {
            fn from(name: &$type_name) -> Self {
                name.to_owned()
            }
        }

        impl std::convert::From<$owned_type_name> for String {
            fn from(val: $owned_type_name) -> Self {
                val.0
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
                self.0.fmt(f)
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
