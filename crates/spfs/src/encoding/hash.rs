use std::convert::{TryFrom, TryInto};
use std::fmt::Display;
use std::io::{Read, Write};

use data_encoding::BASE32;
use ring::digest::{Context, SHA256, SHA256_OUTPUT_LEN};

use crate::{Error, Result};

/// Encodable is a type that can be binary encoded to a byte stream.
pub trait Encodable
where
    Self: Sized,
{
    fn digest(&self) -> Result<Digest> {
        let mut buffer = std::io::Cursor::new(Vec::<u8>::new());
        self.encode(&mut buffer)?;
        let mut ctx = Context::new(&SHA256);
        ctx.update(&buffer.get_ref().as_slice());
        let ring_digest = ctx.finish();
        let bytes = match ring_digest.as_ref().try_into() {
            Err(err) => return Err(Error::new(format!("internal error: {:?}", err))),
            Ok(b) => b,
        };
        Ok(Digest(bytes))
    }

    /// Write this object in binary format.
    fn encode(&self, writer: impl Write) -> Result<()>;

    /// Read a previously encoded object from the given binary stream.
    fn decode(reader: impl Read) -> Result<Self>;
}

impl Encodable for String {
    fn encode(&self, writer: impl Write) -> Result<()> {
        super::binary::write_string(writer, self)
    }

    fn decode(reader: impl Read) -> Result<Self> {
        super::binary::read_string(reader)
    }
}

/// Digest is the result of a hashing operation over binary data.
#[derive(PartialEq, Eq, Hash, Copy, Clone, Debug)]
pub struct Digest([u8; DIGEST_SIZE]);

impl AsRef<[u8]> for Digest {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl<'a> Digest {
    pub fn as_bytes(&'a self) -> &'a [u8] {
        self.0.as_ref()
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
pub fn parse_digest(digest_str: &str) -> Result<Digest> {
    let digest_bytes = match BASE32.decode(digest_str.as_bytes()) {
        Ok(bytes) => bytes,
        Err(err) => return Err(Error::new(format!("invalid digest: {:?}", err))),
    };
    Digest::from_bytes(digest_bytes.as_slice())
}
