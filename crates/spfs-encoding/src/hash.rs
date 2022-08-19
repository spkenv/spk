// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::convert::{TryFrom, TryInto};
use std::fmt::Display;
use std::io::{Read, Write};
use std::pin::Pin;
use std::task::Poll;

use data_encoding::BASE32;
use ring::digest::{Context, SHA256, SHA256_OUTPUT_LEN};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncWrite};

use super::binary;
use crate::{Error, Result};

#[cfg(test)]
#[path = "./hash_test.rs"]
mod hash_test;

/// The Hasher calculates a [`Digest`] from the bytes written to it.
///
/// A write-though target can optionally specified
/// at creation time using the `Hasher::with_target` constructor.
/// In this form, the hasher will write to the given target
/// while also being able to provide the final digest of
/// everything that was written.
///
/// If constructed with a [`tokio::io::AsyncRead`] instance,
/// the hasher will instead act like an `AsyncRead`.
pub struct Hasher<T> {
    ctx: Context,
    target: T,
}

impl<T> Hasher<T> {
    /// The target of the hasher will receive a copy
    /// of all bytes that are written to it
    pub fn with_target(writer: T) -> Self {
        Self {
            ctx: Context::new(&SHA256),
            target: writer,
        }
    }

    /// Finalize the hasher and return the digest
    pub fn digest(self) -> Digest {
        let ring_digest = self.ctx.finish();
        let bytes = match ring_digest.as_ref().try_into() {
            Err(err) => panic!("internal error: {:?}", err),
            Ok(b) => b,
        };
        Digest(bytes)
    }
}

impl Default for Hasher<std::io::Sink> {
    fn default() -> Self {
        Self {
            ctx: Context::new(&SHA256),
            target: std::io::sink(),
        }
    }
}

impl<T> std::ops::Deref for Hasher<T> {
    type Target = Context;

    fn deref(&self) -> &Self::Target {
        &self.ctx
    }
}
impl<T> std::ops::DerefMut for Hasher<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.ctx
    }
}

impl<T> Write for Hasher<T>
where
    T: Write,
{
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.ctx.update(buf);
        self.target.write_all(buf)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.target.flush()
    }
}

impl<T> AsyncWrite for Hasher<T>
where
    T: AsyncWrite + Unpin,
{
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let written = match Pin::new(&mut self.target).poll_write(cx, buf) {
            Poll::Pending => return Poll::Pending,
            Poll::Ready(Err(err)) => return Poll::Ready(Err(err)),
            Poll::Ready(Ok(count)) => count,
        };
        self.ctx.update(&buf[..written]);
        Poll::Ready(Ok(written))
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.target).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.target).poll_shutdown(cx)
    }
}

/// Encodable is a type that can be binary-encoded to a byte stream
pub trait Encodable
where
    Self: Sized,
{
    /// Compute the digest for this instance, by
    /// encoding it into binary form and hashing the result
    fn digest(&self) -> Result<Digest> {
        let mut hasher = Hasher::default();
        self.encode(&mut hasher)?;
        Ok(hasher.digest())
    }

    /// Write this object in binary format.
    fn encode(&self, writer: &mut impl Write) -> Result<()>;

    /// Encode this object into it's binary form in memory.
    fn encode_to_bytes(&self) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        self.encode(&mut buf)?;
        Ok(buf)
    }
}

/// Decodable is a type that can be rebuilt from a previously encoded binary stream
pub trait Decodable
where
    Self: Encodable,
{
    /// Read a previously encoded object from the given binary stream.
    fn decode(reader: &mut impl std::io::BufRead) -> Result<Self>;
}

impl Encodable for String {
    fn encode(&self, writer: &mut impl Write) -> Result<()> {
        super::binary::write_string(writer, self)
    }
}
impl Decodable for String {
    fn decode(reader: &mut impl std::io::BufRead) -> Result<Self> {
        super::binary::read_string(reader)
    }
}

/// The first N bytes of a digest that may still be unambiguous as a reference
#[derive(Debug, Hash, Eq, PartialEq, Ord, PartialOrd, Clone)]
pub struct PartialDigest(Vec<u8>);

impl PartialDigest {
    /// Parse the given string as a partial digest.
    ///
    /// Not every subset of characters from a digest is valid
    /// or useful, but we can extract as much useful information
    /// as we can in order to (hopefully) create a non-ambiguous
    /// prefix of bytes.
    pub fn parse<S: AsRef<str>>(source: S) -> Result<Self> {
        use std::borrow::Cow;
        /// The specific length multiple required by our encoding
        const PAD_TO_MULTIPLE: usize = 8;

        let mut partial = Cow::Borrowed(source.as_ref());

        // the static BASE32 implementation rejects inputs which
        // are not valid outputs, but that doesn't mean that we
        // can't get valuable partial data from a set of characters
        let mut spec = data_encoding::Specification::new();
        spec.symbols.push_str("ABCDEFGHIJKLMNOPQRSTUVWXYZ234567");
        spec.padding = Some('=');
        spec.check_trailing_bits = false;
        let permissive_base32 = spec
            .encoding()
            .expect("hard-coded encoding should be valid");

        // an empty digest string is always ambiguous and not valid
        if partial.is_empty() {
            return Err(Error::InvalidPartialDigest {
                reason: "partial digest cannot be empty".to_string(),
                given: String::new(),
            });
        }
        // BASE32 requires padding in specific multiples
        let trailing_character_count = partial.len() % PAD_TO_MULTIPLE;
        if trailing_character_count > 0 {
            partial = Cow::Owned(format!(
                "{partial}{}",
                "=".repeat(PAD_TO_MULTIPLE - trailing_character_count)
            ));
        }
        let decoded = permissive_base32
            .decode(partial.as_bytes())
            .map_err(|err| {
                use data_encoding::DecodeKind::*;
                let source = source.as_ref();
                match err.kind {
                    Padding => Error::InvalidPartialDigest {
                        reason: "len must be a multiple of 2".to_string(),
                        given: source.to_owned(),
                    },
                    _ => Error::InvalidPartialDigest {
                        reason: err.to_string(),
                        given: source.to_owned(),
                    },
                }
            })?;

        Ok(Self(decoded))
    }

    /// Return true if this partial digest is actually a full digest
    pub fn is_full(&self) -> bool {
        self.len() == DIGEST_SIZE
    }

    /// If this partial digest is actually a full digest, convert it
    pub fn to_digest(&self) -> Option<Digest> {
        if let Ok(d) = Digest::from_bytes(self.as_slice()) {
            Some(d)
        } else {
            None
        }
    }
}

impl std::str::FromStr for PartialDigest {
    type Err = Error;

    fn from_str(source: &str) -> Result<Self> {
        Self::parse(source)
    }
}

impl Display for PartialDigest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let encoded = BASE32.encode(self.as_slice());
        // ignore padding as it's not needed to reparse this value
        // eg: "LCI3LNJC2XPQ====" => "LCI3LNJC2XPQ"
        f.write_str(encoded.trim_end_matches('='))
    }
}

impl From<&[u8]> for PartialDigest {
    fn from(bytes: &[u8]) -> Self {
        Self(bytes.to_vec())
    }
}

impl From<Vec<u8>> for PartialDigest {
    fn from(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }
}

impl AsRef<[u8]> for PartialDigest {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl From<PartialDigest> for Vec<u8> {
    fn from(partial: PartialDigest) -> Self {
        partial.0
    }
}

impl std::ops::Deref for PartialDigest {
    type Target = Vec<u8>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<PartialDigest> for PartialDigest {
    fn as_ref(&self) -> &Self {
        self
    }
}

/// Digest is the result of a hashing operation over binary data.
#[derive(PartialEq, Eq, Hash, Copy, Clone, Ord, PartialOrd)]
pub struct Digest([u8; DIGEST_SIZE]);

impl std::ops::Deref for Digest {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.0[..]
    }
}

impl Default for Digest {
    fn default() -> Self {
        NULL_DIGEST.into()
    }
}

impl std::fmt::Debug for Digest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.to_string().as_ref())
    }
}

impl std::str::FromStr for Digest {
    type Err = crate::Error;

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

impl<'a> Digest {
    /// Yields a view of the underlying bytes for this digest
    pub fn as_bytes(&'a self) -> &'a [u8] {
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
            Err(_err) => Err(Error::DigestLengthError(digest_bytes.len())),
            Ok(bytes) => Ok(Self(bytes)),
        }
    }

    /// Parse the given string as an encoded digest
    pub fn parse(digest_str: &str) -> Result<Digest> {
        digest_str.try_into()
    }

    /// Reads the given async reader to completion, returning
    /// the digest of it's contents.
    pub async fn from_async_reader(mut reader: impl AsyncRead + Unpin) -> Result<Self> {
        use tokio::io::AsyncReadExt;
        let mut ctx = Context::new(&SHA256);
        let mut buf = Vec::with_capacity(4096);
        let mut count;
        buf.resize(4096, 0);
        loop {
            count = reader
                .read(buf.as_mut_slice())
                .await
                .map_err(Error::EncodingReadError)?;
            if count == 0 {
                break;
            }
            ctx.update(&buf.as_slice()[..count]);
        }
        let ring_digest = ctx.finish();
        let bytes = ring_digest
            .as_ref()
            .try_into()
            .expect("sha256 digest should be the exact desired length");
        Ok(Digest(bytes))
    }

    /// Reads the given reader to completion, returning
    /// the digest of it's contents.
    pub fn from_reader(mut reader: impl Read) -> Result<Self> {
        let mut ctx = Context::new(&SHA256);
        let mut buf = Vec::with_capacity(4096);
        let mut count;
        buf.resize(4096, 0);
        loop {
            count = reader
                .read(buf.as_mut_slice())
                .map_err(Error::EncodingReadError)?;
            if count == 0 {
                break;
            }
            ctx.update(&buf.as_slice()[..count]);
        }
        let ring_digest = ctx.finish();
        let bytes = ring_digest
            .as_ref()
            .try_into()
            .expect("sha256 digest should be the exact desired length");
        Ok(Digest(bytes))
    }
}

impl Serialize for Digest {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.to_string().as_ref())
    }
}
impl<'de> Deserialize<'de> for Digest {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        /// Visits a serialized string, decoding it as a digest
        struct StringVisitor<'de>(&'de u8);
        impl<'de> serde::de::Visitor<'de> for StringVisitor<'de> {
            type Value = Digest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("base32 encoded digest")
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Digest, E>
            where
                E: serde::de::Error,
            {
                match Digest::try_from(value) {
                    Ok(digest) => Ok(digest),
                    Err(_) => Err(serde::de::Error::invalid_value(
                        serde::de::Unexpected::Str(value),
                        &self,
                    )),
                }
            }
        }
        deserializer.deserialize_str(StringVisitor(&0))
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

impl Display for Digest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(BASE32.encode(self.as_bytes()).as_ref())
    }
}

impl Encodable for Digest {
    fn encode(&self, mut writer: &mut impl Write) -> Result<()> {
        binary::write_digest(&mut writer, self)
    }

    fn digest(&self) -> Result<Digest> {
        Ok(*self)
    }
}

impl Decodable for Digest {
    fn decode(reader: &mut impl std::io::BufRead) -> Result<Self> {
        binary::read_digest(reader)
    }
}

impl Encodable for &Digest {
    fn encode(&self, mut writer: &mut impl Write) -> Result<()> {
        binary::write_digest(&mut writer, self)
    }

    fn digest(&self) -> Result<Digest> {
        Ok(*self.to_owned())
    }
}

/// The number of bytes that make up an spfs digest
pub const DIGEST_SIZE: usize = SHA256_OUTPUT_LEN;

/// The bytes of an empty digest. This represents the result of hashing no bytes - the initial state.
///
/// ```
/// use std::convert::TryInto;
/// use ring::digest;
/// use spfs_encoding::{EMPTY_DIGEST, DIGEST_SIZE};
///
/// let empty_digest: [u8; DIGEST_SIZE] = digest::digest(&digest::SHA256, b"").as_ref().try_into().unwrap();
/// assert_eq!(empty_digest, EMPTY_DIGEST);
/// ```
pub const EMPTY_DIGEST: [u8; DIGEST_SIZE] = [
    227, 176, 196, 66, 152, 252, 28, 20, 154, 251, 244, 200, 153, 111, 185, 36, 39, 174, 65, 228,
    100, 155, 147, 76, 164, 149, 153, 27, 120, 82, 184, 85,
];

/// The bytes of an entirely null digest. This does not represent the result of hashing no bytes, because
/// sha256 has a defined initial state. This is an explicitly unique result of entirely null bytes.
pub const NULL_DIGEST: [u8; DIGEST_SIZE] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

/// Parse a string-digest.
pub fn parse_digest(digest_str: impl AsRef<str>) -> Result<Digest> {
    let digest_bytes = BASE32
        .decode(digest_str.as_ref().as_bytes())
        .map_err(Error::DigestDecodeError)?;
    Digest::from_bytes(digest_bytes.as_slice())
}
