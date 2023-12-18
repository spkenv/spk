// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::convert::TryInto;
use std::fmt::Display;
use std::io::{Read, Write};
use std::pin::Pin;
use std::task::Poll;

use data_encoding::BASE32;
use ring::digest::{Context, SHA256};
use tokio::io::{AsyncRead, AsyncWrite};

use super::{binary, Digest};
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
            Err(err) => panic!("internal error: {err:?}"),
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

impl Hasher<std::io::Sink> {
    /// Create an instance that implements Write
    pub fn new_sync() -> Self {
        Self::default()
    }
}

impl Default for Hasher<tokio::io::Sink> {
    fn default() -> Self {
        Self {
            ctx: Context::new(&SHA256),
            target: tokio::io::sink(),
        }
    }
}

impl Hasher<tokio::io::Sink> {
    /// Create an instance that implements AsyncWrite
    pub fn new_async() -> Self {
        Self::default()
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
        let count = self.target.write(buf)?;
        self.ctx.update(&buf[..count]);
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

impl Hasher<()> {
    /// Reads the given async reader to completion, returning
    /// the digest of its contents.
    pub async fn hash_async_reader(mut reader: impl AsyncRead + Unpin) -> Result<Digest> {
        let mut hasher = Hasher::new_async();
        tokio::io::copy(&mut reader, &mut hasher)
            .await
            .map_err(Error::FailedRead)?;
        Ok(hasher.digest())
    }

    /// Reads the given reader to completion, returning
    /// the digest of its contents.
    pub fn hash_reader(mut reader: impl Read) -> Result<Digest> {
        let mut hasher = Hasher::new_sync();
        std::io::copy(&mut reader, &mut hasher).map_err(Error::FailedRead)?;
        Ok(hasher.digest())
    }
}

/// Encodable is a type that can be binary-encoded to a byte stream
pub trait Encodable
where
    Self: Sized,
{
    /// The flavor of error returned by encoding methods
    type Error;

    /// Compute the digest for this instance, by
    /// encoding it into binary form and hashing the result
    fn digest(&self) -> std::result::Result<Digest, Self::Error> {
        let mut hasher = Hasher::new_sync();
        self.encode(&mut hasher)?;
        Ok(hasher.digest())
    }

    /// Write this object in binary format.
    fn encode(&self, writer: &mut impl Write) -> std::result::Result<(), Self::Error>;

    /// Encode this object into it's binary form in memory.
    fn encode_to_bytes(&self) -> std::result::Result<Vec<u8>, Self::Error> {
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
    fn decode(reader: &mut impl std::io::BufRead) -> std::result::Result<Self, Self::Error>;
}

impl<T> Encodable for &T
where
    T: Encodable,
{
    type Error = T::Error;

    fn encode(&self, writer: &mut impl Write) -> std::result::Result<(), Self::Error> {
        (**self).encode(writer)
    }
}

impl Encodable for String {
    type Error = Error;

    fn encode(&self, writer: &mut impl Write) -> std::result::Result<(), Self::Error> {
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
        self.len() == super::DIGEST_SIZE
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

impl Decodable for Digest {
    fn decode(reader: &mut impl std::io::BufRead) -> Result<Self> {
        binary::read_digest(reader)
    }
}

impl Encodable for Digest {
    type Error = Error;

    fn encode(&self, writer: &mut impl Write) -> Result<()> {
        binary::write_digest(writer, self)
    }

    fn digest(&self) -> Result<Digest> {
        Ok(*self)
    }
}
