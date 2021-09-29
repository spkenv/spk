// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::convert::{TryFrom, TryInto};
use std::fmt::Display;
use std::io::{Read, Write};

use data_encoding::BASE32;
use ring::digest::{Context, SHA256, SHA256_OUTPUT_LEN};
use serde::{Deserialize, Serialize};

use super::binary;
use crate::{Error, Result};

pub struct Hasher<'t> {
    ctx: Context,
    target: Option<&'t mut dyn Write>,
}

impl<'t> Hasher<'t> {
    pub fn new() -> Self {
        Self {
            ctx: Context::new(&SHA256),
            target: None,
        }
    }

    pub fn with_target(mut self, writer: &'t mut impl Write) -> Self {
        self.target.replace(writer);
        self
    }

    pub fn digest(self) -> Digest {
        let ring_digest = self.ctx.finish();
        let bytes = match ring_digest.as_ref().try_into() {
            Err(err) => panic!("internal error: {:?}", err),
            Ok(b) => b,
        };
        Digest(bytes)
    }
}

impl<'t> std::ops::Deref for Hasher<'t> {
    type Target = Context;

    fn deref(&self) -> &Self::Target {
        &self.ctx
    }
}
impl<'t> std::ops::DerefMut for Hasher<'t> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.ctx
    }
}

impl<'t> Write for Hasher<'t> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.ctx.update(buf);
        if let Some(target) = self.target.as_mut() {
            target.write_all(buf)?;
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Encodable is a type that can be binary encoded to a byte stream.
pub trait Encodable
where
    Self: Sized,
{
    fn digest(&self) -> Result<Digest> {
        let mut hasher = Hasher::new();
        self.encode(&mut hasher)?;
        Ok(hasher.digest())
    }

    /// Write this object in binary format.
    fn encode(&self, writer: &mut impl Write) -> Result<()>;
}

pub trait Decodable
where
    Self: Encodable,
{
    /// Read a previously encoded object from the given binary stream.
    fn decode(reader: &mut impl Read) -> Result<Self>;
}

impl Encodable for String {
    fn encode(&self, writer: &mut impl Write) -> Result<()> {
        super::binary::write_string(writer, self)
    }
}
impl Decodable for String {
    fn decode(reader: &mut impl Read) -> Result<Self> {
        super::binary::read_string(reader)
    }
}

pub struct PartialDigest(Vec<u8>);

impl PartialDigest {
    pub fn parse<S: AsRef<str>>(source: S) -> Result<Self> {
        use std::borrow::Cow;

        let mut partial = Cow::Borrowed(source.as_ref());
        // BASE32 requires padding in mutliples of 8
        let missing = partial.len() % 8;
        if missing > 0 {
            partial = Cow::Owned(format!("{}{}", partial, "=".repeat(missing)));
        }
        let decoded = BASE32
            .decode(partial.as_bytes())
            .map_err(|err| Error::new(format!("invalid partial digest: {:?}", err)))?;
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

impl AsRef<[u8]> for PartialDigest {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
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
    pub fn as_bytes(&'a self) -> &'a [u8] {
        self.0.as_ref()
    }
    pub fn to_string(&self) -> String {
        BASE32.encode(self.as_bytes())
    }
    pub fn from_bytes(digest_bytes: &[u8]) -> Result<Self> {
        match digest_bytes.try_into() {
            Err(err) => Err(Error::new(format!(
                "{} ({} != {})",
                err,
                digest_bytes.len(),
                SHA256_OUTPUT_LEN
            ))),
            Ok(bytes) => Ok(Self(bytes)),
        }
    }
    pub fn parse(digest_str: &str) -> Result<Digest> {
        digest_str.try_into()
    }

    pub fn from_reader(reader: &mut impl Read) -> Result<Self> {
        let mut ctx = Context::new(&SHA256);
        let mut buf = Vec::with_capacity(4096);
        let mut count;
        buf.resize(4096, 0);
        loop {
            count = reader.read(buf.as_mut_slice())?;
            if count == 0 {
                break;
            }
            ctx.update(&buf.as_slice()[..count]);
        }
        let ring_digest = ctx.finish();
        let bytes = match ring_digest.as_ref().try_into() {
            Err(err) => return Err(Error::new(format!("internal error: {:?}", err))),
            Ok(b) => b,
        };
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
    fn decode(mut reader: &mut impl Read) -> Result<Self> {
        binary::read_digest(&mut reader)
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

pub const DIGEST_SIZE: usize = SHA256_OUTPUT_LEN;

/// The bytes of an empty digest. This represents the result of hashing no bytes - the initial state.
///
/// ```
/// use std::convert::TryInto;
/// use ring::digest;
/// use spfs::encoding::{EMPTY_DIGEST, DIGEST_SIZE};
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
    let digest_bytes = match BASE32.decode(digest_str.as_ref().as_bytes()) {
        Ok(bytes) => bytes,
        Err(err) => return Err(Error::new(format!("invalid digest: {:?}", err))),
    };
    Digest::from_bytes(digest_bytes.as_slice())
}
