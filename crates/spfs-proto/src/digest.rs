// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use data_encoding::BASE32;
use miette::Diagnostic;

use super::Digest;

#[cfg(test)]
#[path = "./digest_test.rs"]
mod digest_test;

/// The number of bytes that make up an spfs digest
pub const DIGEST_SIZE: usize = std::mem::size_of::<Digest>();

/// The bytes of an empty digest. This represents the result of hashing no bytes - the initial state.
pub const EMPTY_DIGEST: [u8; DIGEST_SIZE] = [
    227, 176, 196, 66, 152, 252, 28, 20, 154, 251, 244, 200, 153, 111, 185, 36, 39, 174, 65, 228,
    100, 155, 147, 76, 164, 149, 153, 27, 120, 82, 184, 85,
];

/// The bytes of an entirely null digest. This does not represent the result of hashing no bytes, because
/// sha256 has a defined initial state. This is an explicitly unique result of entirely null bytes.
pub const NULL_DIGEST: [u8; DIGEST_SIZE] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

impl std::ops::Deref for Digest {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.0[..]
    }
}

impl std::cmp::Eq for Digest {}

impl std::hash::Hash for Digest {
    fn hash<H>(&self, hasher: &mut H)
    where
        H: std::hash::Hasher,
    {
        self.0.hash(hasher)
    }
}

impl std::cmp::Ord for Digest {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.cmp(&other.0)
    }
}

impl std::cmp::PartialOrd for Digest {
    fn partial_cmp(&self, other: &Self) -> std::option::Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl std::str::FromStr for Digest {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        Digest::parse(s)
    }
}

impl AsRef<[u8]> for Digest {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl AsRef<Digest> for Digest {
    fn as_ref(&self) -> &Self {
        self
    }
}

impl Digest {
    /// Yields a view of the underlying bytes for this digest
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_ref()
    }

    /// Extract the raw bytes of this digest
    pub fn into_bytes(self) -> [u8; DIGEST_SIZE] {
        self.0
    }

    /// Create a digest from the provided bytes.
    ///
    /// The exact [`DIGEST_SIZE`] number of bytes must
    /// be given.
    pub fn from_bytes(digest_bytes: &[u8]) -> Result<Self> {
        match digest_bytes.try_into() {
            Err(_err) => Err(Error::InvalidDigestLength(digest_bytes.len())),
            Ok(bytes) => Ok(Self(bytes)),
        }
    }

    /// Parse the given string as an encoded digest
    pub fn parse(digest_str: &str) -> Result<Digest> {
        digest_str.try_into()
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for Digest {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.to_string().as_ref())
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Digest {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        /// Visits a serialized string, decoding it as a digest
        struct StringVisitor;

        impl<'de> serde::de::Visitor<'de> for StringVisitor {
            type Value = Digest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("base32 encoded digest")
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Digest, E>
            where
                E: serde::de::Error,
            {
                Digest::try_from(value).map_err(serde::de::Error::custom)
            }
        }
        deserializer.deserialize_str(StringVisitor)
    }
}

impl From<[u8; DIGEST_SIZE]> for Digest {
    fn from(bytes: [u8; DIGEST_SIZE]) -> Self {
        Digest(bytes)
    }
}

impl TryFrom<&str> for Digest {
    type Error = Error;

    fn try_from(digest_str: &str) -> Result<Digest> {
        parse_digest(digest_str)
    }
}

impl std::fmt::Display for Digest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(BASE32.encode(self.as_bytes()).as_ref())
    }
}

/// Parse a string-encoded digest.
pub fn parse_digest(digest_str: impl AsRef<str>) -> Result<Digest> {
    let digest_bytes = BASE32
        .decode(digest_str.as_ref().as_bytes())
        .map_err(Error::InvalidDigestEncoding)?;
    Digest::from_bytes(digest_bytes.as_slice())
}

/// A specialized result for digest-related operations
pub type Result<T> = std::result::Result<T, Error>;

/// The error type that is returned by digest operations
#[derive(thiserror::Error, Diagnostic, Debug)]
#[diagnostic(
    url(
        "https://spkenv.dev/error_codes#{}",
        self.code().unwrap_or_else(|| Box::new("spfs::generic"))
    )
)]
pub enum Error {
    /// A digest could not be decoded from a string because the
    /// contained invalid data or was otherwise malformed
    #[error("Could not decode digest: {0}")]
    InvalidDigestEncoding(#[source] data_encoding::DecodeError),

    /// A digest could not be created because the wrong number
    /// of bytes were provided
    #[error("Invalid number of bytes for digest: {0} != {}", super::DIGEST_SIZE)]
    InvalidDigestLength(usize),
}
