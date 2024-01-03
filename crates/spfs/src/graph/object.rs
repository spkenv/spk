// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::io::{BufRead, Write};
use std::marker::PhantomData;

use bytes::BufMut;
use encoding::prelude::*;
use serde::{Deserialize, Serialize};

use super::error::{ObjectError, ObjectResult};
use super::{Blob, DatabaseView, HasKind, Kind, Layer, Manifest, ObjectKind, Platform};
use crate::encoding;
use crate::storage::RepositoryHandle;

#[cfg(test)]
#[path = "./object_test.rs"]
mod object_test;

/// An node in the spfs object graph
pub type Object = FlatObject<spfs_proto::AnyObject<'static>>;

impl<T: ObjectProto> From<FlatObject<T>> for Object
where
    T: Kind,
{
    fn from(value: FlatObject<T>) -> Self {
        let FlatObject { buf, offset, _t } = value;
        Self {
            buf,
            offset,
            _t: PhantomData,
        }
    }
}

impl std::fmt::Display for Object {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Object")
            .field(&self.kind())
            .field(&self.digest().unwrap())
            .finish()
    }
}

impl std::fmt::Debug for Object {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.to_enum().fmt(f)
    }
}

impl Object {
    /// A salt use to prime the digest calculation so it is less likely to
    /// collide with digests produced for user content (blob and payloads)
    const DIGEST_SALT: &'static [u8] = b"spfs digest da8d8e62-9459-11ee-adab-00155dcb338b\0";

    /// Create an object from the encoded bytes.
    ///
    /// In memory, objects always use the latest flatbuffer
    /// format. The given bytes may be discarded or reconstructed
    /// if a conversion is necessary, but the header is preserved
    /// in order to ensure that the object does not change it's
    /// digest unless explicitly marked to do so.
    pub fn new<B: Into<bytes::Bytes>>(buf: B) -> crate::Result<Self> {
        let bytes = buf.into();
        let header = Header::new(&bytes)?;
        let Some(kind) = header.object_kind() else {
            return Err(ObjectError::UnexpectedKind(header.object_kind_number()).into());
        };
        let Some(format) = header.encoding_format() else {
            return Err(ObjectError::UnknownEncoding(header.encoding_format_number()).into());
        };
        match format {
            EncodingFormat::Legacy => {
                let mut reader = std::io::BufReader::new(&bytes[Header::SIZE..]);
                let object = match kind {
                    ObjectKind::Blob => Blob::builder()
                        .with_header(|h| h.copy_from(header))
                        .legacy_decode(&mut reader)?
                        .into_object(),
                    ObjectKind::Manifest => Manifest::builder()
                        .with_header(|h| h.copy_from(header))
                        .legacy_decode(&mut reader)?
                        .into_object(),
                    ObjectKind::Layer => Layer::builder()
                        .with_header(|h| h.copy_from(header))
                        .legacy_decode(&mut reader)?
                        .into_object(),
                    ObjectKind::Platform => Platform::builder()
                        .with_header(|h| h.copy_from(header))
                        .legacy_decode(&mut reader)?
                        .into_object(),
                    ObjectKind::Tree | ObjectKind::Mask => {
                        // although these kinds used to be supported, they were never actually encoded
                        // separately into files and so should not appear in this context
                        return Err(ObjectError::UnexpectedKind(kind as u8).into());
                    }
                };
                Ok(object)
            }
            EncodingFormat::FlatBuffers => {
                // all we need to do with a flatbuffer is validate it, without
                // any need to change or reallocate the buffer
                flatbuffers::root::<spfs_proto::AnyObject>(&bytes[Header::SIZE..])
                    .map_err(ObjectError::InvalidFlatbuffer)?;
                Ok(Object {
                    buf: bytes,
                    offset: 0,
                    _t: PhantomData,
                })
            }
        }
    }

    /// Constructs a new [`Object`] instance from the provided flatbuffer.
    ///
    /// # Safety
    /// `buf` must contain a valid flatbuffer with an [`spfs_proto::AnyObject`]
    /// at its root of the provided kind.
    pub unsafe fn new_with_default_header(buf: &[u8], kind: ObjectKind) -> Self {
        unsafe {
            // Safety: We are building a valid header and passing the other
            // requirements up to the caller
            Self::new_with_header(Header::builder(kind).build(), buf, 0)
        }
    }

    pub fn child_objects(&self) -> Vec<encoding::Digest> {
        match self.to_enum() {
            Enum::Platform(platform) => platform.child_objects(),
            Enum::Layer(layer) => layer.child_objects(),
            Enum::Manifest(manifest) => manifest.child_objects(),
            Enum::Blob(_blob) => Vec::new(),
        }
    }

    /// Return true if this Object kind also has a payload
    #[inline]
    pub fn has_payload(&self) -> bool {
        self.kind() == ObjectKind::Blob
    }

    /// Calculates the total size of the object and all children, recursively
    pub async fn calculate_object_size(&self, repo: &RepositoryHandle) -> crate::Result<u64> {
        let mut total_size: u64 = 0;
        let mut items_to_process: Vec<Object> = vec![self.clone()];

        while !items_to_process.is_empty() {
            let mut next_iter_objects: Vec<Object> = Vec::new();
            for object in items_to_process.iter() {
                match object.to_enum() {
                    Enum::Platform(object) => {
                        for digest in object.iter_bottom_up() {
                            let item = repo.read_object(*digest).await?;
                            next_iter_objects.push(item);
                        }
                    }
                    Enum::Layer(object) => {
                        let item = repo.read_object(*object.manifest()).await?;
                        next_iter_objects.push(item);
                    }
                    Enum::Manifest(object) => {
                        for node in object.to_tracking_manifest().walk_abs("/spfs") {
                            total_size += node.entry.size
                        }
                    }
                    Enum::Blob(object) => total_size += object.size(),
                }
            }
            items_to_process = std::mem::take(&mut next_iter_objects);
        }
        Ok(total_size)
    }
}

impl HasKind for Object {
    fn kind(&self) -> super::ObjectKind {
        self.header()
            .object_kind()
            .expect("buffer already validated")
    }
}

impl<T: ObjectProto> encoding::Digestible for FlatObject<T> {
    type Error = crate::Error;

    fn digest(&self) -> crate::Result<encoding::Digest> {
        let header = self.header();
        let strategy = header.digest_strategy().ok_or_else(|| {
            super::error::ObjectError::UnknownDigestStrategy(header.digest_strategy_number())
        })?;
        let variant = self.to_enum();
        if let Enum::Blob(b) = variant {
            // blobs share a digest with the payload that they represent.
            // Much of the codebase leverages this fact to skip additional
            // steps, just as we are doing here to avoid running the hasher
            return Ok(*b.payload());
        };
        let mut hasher = encoding::Hasher::new_sync();
        match strategy {
            DigestStrategy::Legacy => {
                // the original digest strategy did
                // not include the kind or any special salting
            }
            DigestStrategy::WithKindAndSalt => {
                hasher
                    .write_all(Object::DIGEST_SALT)
                    .map_err(encoding::Error::FailedWrite)?;
                hasher
                    .write_all(&[header.object_kind_number()])
                    .map_err(encoding::Error::FailedWrite)?;
            }
        }
        match variant {
            Enum::Platform(obj) => obj.legacy_encode(&mut hasher)?,
            Enum::Layer(obj) => obj.legacy_encode(&mut hasher)?,
            Enum::Manifest(obj) => obj.legacy_encode(&mut hasher)?,
            Enum::Blob(_obj) => unreachable!("handled above"),
        }
        Ok(hasher.digest())
    }
}

impl<T: ObjectProto> encoding::Encodable for FlatObject<T> {
    type Error = crate::Error;

    fn encode(&self, mut writer: &mut impl std::io::Write) -> crate::Result<()> {
        let format = self
            .header()
            .encoding_format()
            .expect("an already validated header");
        match format {
            EncodingFormat::Legacy => {
                writer
                    .write_all(&self.buf[..Header::SIZE])
                    .map_err(encoding::Error::FailedWrite)?;
                match self.to_enum() {
                    Enum::Blob(obj) => obj.legacy_encode(&mut writer),
                    Enum::Manifest(obj) => obj.legacy_encode(&mut writer),
                    Enum::Layer(obj) => obj.legacy_encode(&mut writer),
                    Enum::Platform(obj) => obj.legacy_encode(&mut writer),
                }
            }
            EncodingFormat::FlatBuffers => {
                // the flatbuffer format is useful exactly because it does
                // not require the data to be encoded or decoded from the wire
                // format
                writer
                    .write_all(&self.buf)
                    .map_err(encoding::Error::FailedWrite)?;
                Ok(())
            }
        }
    }
}

impl encoding::Decodable for Object {
    fn decode(reader: &mut impl BufRead) -> crate::Result<Self> {
        let mut bytes = bytes::BytesMut::new().writer();
        std::io::copy(reader, &mut bytes).map_err(encoding::Error::FailedRead)?;
        Self::new(bytes.into_inner())
    }
}

/// A cheaply clone-able, disambiguation of an [`Object`]
#[derive(Debug, Clone, strum::Display)]
pub enum Enum {
    Platform(super::Platform),
    Layer(super::Layer),
    Manifest(super::Manifest),
    Blob(super::Blob),
}

impl HasKind for Enum {
    fn kind(&self) -> super::ObjectKind {
        match self {
            Enum::Platform(_) => super::ObjectKind::Platform,
            Enum::Layer(_) => super::ObjectKind::Layer,
            Enum::Manifest(_) => super::ObjectKind::Manifest,
            Enum::Blob(_) => super::ObjectKind::Blob,
        }
    }
}

pub struct FlatObject<T: ObjectProto> {
    /// The underlying flatbuffer is setup to always contain
    /// an [`spfs_proto::AnyObject`] with the generic `T` used
    /// to disambiguate the actual object kind stored within for
    /// easier API usage and adding specific behaviors to each kind.
    buf: bytes::Bytes,
    /// For any specific object type (not `Object<AnyObject>`) this
    /// field stores the pre-validated offset to the underlying
    /// flatbuffer table for the more specific type.
    offset: usize,
    // using an fn type allows this type to still be Send/Sync even
    // if T is not, which is appropriate because it does not actually
    // contain an instance of T
    _t: PhantomData<fn() -> T>,
}

impl<T: ObjectProto> Clone for FlatObject<T> {
    fn clone(&self) -> Self {
        Self {
            buf: self.buf.clone(),
            offset: self.offset,
            _t: PhantomData,
        }
    }
}

impl<T: ObjectProto + Kind> FlatObject<T> {
    /// Constructs a new [`FlatObject`] instance from the provided
    /// flatbuffer and offset value.
    ///
    /// # Safety
    /// `buf` must contain a valid flatbuffer with an [`spfs_proto::AnyObject`]
    /// at its root. Additionally, offset must point to the start of a
    /// valid instance of `T` within the flatbuffer.
    pub unsafe fn new_with_default_header(buf: &[u8], offset: usize) -> Self {
        unsafe {
            // Safety: we are ensuring a good header and pass the other
            // requirements up to our caller
            Self::new_with_header(Header::builder(T::kind()).build(), buf, offset)
        }
    }
}

impl<T: ObjectProto> FlatObject<T> {
    /// Constructs a new [`FlatObject`] instance from the provided
    /// header, flatbuffer and offset value.
    ///
    /// # Safety
    /// `buf` must contain a valid flatbuffer with an [`spfs_proto::AnyObject`]
    /// at its root. Additionally, offset must point to the start of a
    /// valid instance of `T` within the flatbuffer and the header must
    /// be valid and contain the appropriate type of `T`
    pub unsafe fn new_with_header<H>(header: H, buf: &[u8], offset: usize) -> Self
    where
        H: AsRef<Header>,
    {
        let mut bytes = bytes::BytesMut::with_capacity(buf.len() + Header::SIZE);
        bytes.put(&header.as_ref()[..]);
        bytes.put(buf);
        Self {
            buf: bytes.freeze(),
            offset,
            _t: PhantomData,
        }
    }

    #[inline]
    pub fn header(&self) -> &'_ Header {
        #[cfg(debug_assertions)]
        {
            Header::new(&self.buf[..]).expect("header should be already validated")
        }
        #[cfg(not(debug_assertions))]
        // Safety: the header is validated when the object is built
        unsafe {
            Header::new_unchecked(&self.buf[..])
        }
    }

    pub fn into_object(self) -> super::Object {
        let Self { buf, offset: _, _t } = self;
        super::Object {
            buf,
            offset: 0,
            _t: PhantomData,
        }
    }

    pub fn into_enum(self) -> Enum {
        let proto = self.root_proto();
        let offset = proto.object().loc();
        match proto.object_type() {
            spfs_proto::Object::Blob => Enum::Blob(Blob {
                buf: self.buf,
                offset,
                _t: PhantomData,
            }),
            spfs_proto::Object::Layer => Enum::Layer(Layer {
                buf: self.buf,
                offset,
                _t: PhantomData,
            }),
            spfs_proto::Object::Manifest => Enum::Manifest(Manifest {
                buf: self.buf,
                offset,
                _t: PhantomData,
            }),
            spfs_proto::Object::Platform => Enum::Platform(Platform {
                buf: self.buf,
                offset,
                _t: PhantomData,
            }),
            spfs_proto::Object::NONE | spfs_proto::Object(spfs_proto::Object::ENUM_MAX..) => {
                unreachable!("already recognized kind")
            }
        }
    }

    pub fn into_layer(self) -> Option<super::Layer> {
        if let Enum::Layer(l) = self.into_enum() {
            Some(l)
        } else {
            None
        }
    }

    pub fn into_manifest(self) -> Option<super::Manifest> {
        if let Enum::Manifest(l) = self.into_enum() {
            Some(l)
        } else {
            None
        }
    }

    pub fn into_blob(self) -> Option<super::Blob> {
        if let Enum::Blob(l) = self.into_enum() {
            Some(l)
        } else {
            None
        }
    }

    pub fn into_platform(self) -> Option<super::Platform> {
        if let Enum::Platform(l) = self.into_enum() {
            Some(l)
        } else {
            None
        }
    }

    /// Clone (cheaply) this object and make a generic one
    #[inline]
    pub fn to_object(&self) -> Object {
        self.clone().into_object()
    }

    /// Clone (cheaply) this object and identify its type
    #[inline]
    pub fn to_enum(&self) -> Enum {
        self.clone().into_enum()
    }

    /// Read the underlying [`spfs_proto::AnyObject`] flatbuffer
    #[inline]
    pub fn root_proto(&self) -> spfs_proto::AnyObject {
        let buf = &self.buf[Header::SIZE..];
        #[cfg(debug_assertions)]
        {
            flatbuffers::root::<'_, spfs_proto::AnyObject>(buf)
                .expect("object should already be validated")
        }
        #[cfg(not(debug_assertions))]
        // Safety: root_unchecked does no validation, but this type
        // promises that the internal buffer is already valid
        unsafe {
            flatbuffers::root_unchecked::<'_, spfs_proto::AnyObject>(buf)
        }
    }
}

impl<'buf, T> FlatObject<T>
where
    T: flatbuffers::Follow<'buf> + ObjectProto,
    T::Inner: 'buf,
{
    #[inline]
    pub fn proto(&'buf self) -> T::Inner {
        use flatbuffers::Follow;
        // Safety: we trust that the buffer and offset have been
        // validated already when this instance was created
        unsafe { <T as Follow>::follow(&self.buf[..], Header::SIZE + self.offset) }
    }
}

impl<T> Kind for FlatObject<T>
where
    T: ObjectProto + Kind,
{
    #[inline]
    fn kind() -> super::ObjectKind {
        T::kind()
    }
}

impl<T> HasKind for FlatObject<T>
where
    T: ObjectProto + Kind,
{
    #[inline]
    fn kind(&self) -> super::ObjectKind {
        T::kind()
    }
}

/// Each encoded object consists of the magic header string
/// followed by 8 bytes containing information about the rest
/// of the encoded data:
///
/// ```txt
/// |               0 |               1 | 2 | 3 | 4 | 5 | 6 |           7 |
/// | digest strategy | encoding format | _ | _ | _ | _ | _ | object kind |
/// ```
///
/// - digest strategy
///   The strategy used to compute this object's digest. If the strategy
///   is not known then the library cannot faithfully recompute the digest
///   for this object and should consider it unusable.
/// - encoding format
///   The format that the rest of the data is encoded with. If this is not
///   known then the library cannot safely interpret the data that follows
///   the header and should consider the object unusable.
/// - bytes 2..6 are reserved for future use
/// - object kind
///   Denotes the kind of the object that is encoded after the header. This
///   byte may not be used by all encoding formats.
///
/// The original header format was a single 8-byte u64 to denote
/// the kind of the object, but never defined more than 6
#[derive(Debug, Eq, PartialEq, Hash)]
pub struct Header([u8]);

impl std::ops::Deref for Header {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        // dereferences to bytes, but only the header portion
        &self.0[..Self::SIZE]
    }
}

impl Header {
    /// A special string required for all headers
    const PREFIX: &'static [u8] = "--SPFS--\n".as_bytes();
    /// A fixed-size prefix followed by 8 separated byte fields
    /// (this was originally a single u64)
    const SIZE: usize = Self::PREFIX.len() + std::mem::size_of::<u64>();
    const DIGEST_OFFSET: usize = Self::PREFIX.len();
    const ENCODING_OFFSET: usize = Self::PREFIX.len() + 1;
    const KIND_OFFSET: usize = Self::PREFIX.len() + 7;

    #[inline]
    pub fn builder(kind: ObjectKind) -> HeaderBuilder {
        HeaderBuilder::new(kind)
    }

    /// Read the first bytes of `buf` as a header
    ///
    /// The buffer can be be longer than just a header
    /// as only the initial header bytes will be validated
    pub fn new(buf: &[u8]) -> ObjectResult<&Self> {
        if buf.len() < Self::SIZE {
            return Err(ObjectError::HeaderTooShort);
        }
        if &buf[..Self::PREFIX.len()] != Self::PREFIX {
            return Err(ObjectError::HeaderMissingPrefix);
        }
        Ok(unsafe {
            // Safety: we have just validated the buffer above
            Self::new_unchecked(buf)
        })
    }

    /// Read the first bytes of `buf` as a header
    ///
    /// # Safety
    /// This function does not validate that the data buffer
    /// is long enough or has the right shape to be a header.
    pub unsafe fn new_unchecked(buf: &[u8]) -> &Self {
        // Safety: raw pointer casting is usually unsafe but our type
        // wraps/is exactly a slice of bytes
        unsafe { &*(buf as *const [u8] as *const Self) }
    }

    /// The [`DigestStrategy`] in this header, if recognized
    #[inline]
    pub fn digest_strategy(&self) -> Option<DigestStrategy> {
        DigestStrategy::from_u8(self.digest_strategy_number())
    }

    #[inline]
    fn digest_strategy_number(&self) -> u8 {
        self.0[Self::DIGEST_OFFSET]
    }

    /// The [`EncodingFormat`] in this header, if recognized
    #[inline]
    pub fn encoding_format(&self) -> Option<EncodingFormat> {
        EncodingFormat::from_u8(self.encoding_format_number())
    }

    #[inline]
    fn encoding_format_number(&self) -> u8 {
        self.0[Self::ENCODING_OFFSET]
    }

    /// The [`ObjectKind`] in this header, if recognized
    #[inline]
    pub fn object_kind(&self) -> Option<ObjectKind> {
        ObjectKind::from_u8(self.object_kind_number())
    }

    #[inline]
    fn object_kind_number(&self) -> u8 {
        self.0[Self::KIND_OFFSET]
    }
}

/// An owned, mutable [`Header`]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct HeaderBuf([u8; Header::SIZE]);

impl std::ops::Deref for HeaderBuf {
    type Target = Header;

    fn deref(&self) -> &Self::Target {
        // Safety: we always contain a valid header
        unsafe { Header::new_unchecked(&self.0) }
    }
}

impl AsRef<Header> for HeaderBuf {
    #[inline]
    fn as_ref(&self) -> &Header {
        self
    }
}

impl HeaderBuf {
    #[inline]
    pub fn new(kind: ObjectKind) -> Self {
        HeaderBuilder::new(kind).build()
    }

    #[inline]
    pub fn set_object_kind(&mut self, object_kind: ObjectKind) {
        self.0[Header::KIND_OFFSET] = object_kind as u8;
    }

    #[inline]
    pub fn set_digest_strategy(&mut self, digest_strategy: DigestStrategy) {
        self.0[Header::DIGEST_OFFSET] = digest_strategy as u8;
    }

    #[inline]
    pub fn set_encoding_format(&mut self, encoding_format: EncodingFormat) {
        self.0[Header::ENCODING_OFFSET] = encoding_format as u8;
    }
}

#[derive(Debug)]
pub struct HeaderBuilder {
    digest_strategy: DigestStrategy,
    encoding_format: EncodingFormat,
    object_kind: ObjectKind,
}

impl HeaderBuilder {
    pub fn new(object_kind: ObjectKind) -> Self {
        let config = crate::get_config();
        Self {
            digest_strategy: config
                .as_ref()
                .map(|s| s.storage.digest_strategy)
                // for safety, default to the oldest supported format
                .unwrap_or(DigestStrategy::Legacy),
            encoding_format: config
                .as_ref()
                .map(|s| s.storage.encoding_format)
                // for safety, default to the oldest supported format
                .unwrap_or(EncodingFormat::Legacy),
            object_kind,
        }
    }

    pub fn with_object_kind(mut self, object_kind: ObjectKind) -> Self {
        self.object_kind = object_kind;
        self
    }

    pub fn with_digest_strategy(mut self, digest_strategy: DigestStrategy) -> Self {
        self.digest_strategy = digest_strategy;
        self
    }

    pub fn with_encoding_format(mut self, encoding_format: EncodingFormat) -> Self {
        self.encoding_format = encoding_format;
        self
    }

    /// Copy valid and known components from another header
    pub fn copy_from(mut self, other: &Header) -> Self {
        if let Some(digest_strategy) = other.digest_strategy() {
            self = self.with_digest_strategy(digest_strategy);
        }
        if let Some(encoding_format) = other.encoding_format() {
            self = self.with_encoding_format(encoding_format);
        }
        if let Some(object_kind) = other.object_kind() {
            self = self.with_object_kind(object_kind);
        }
        self
    }

    /// Build the header bytes for the current settings
    pub fn build(&self) -> HeaderBuf {
        let mut bytes = [0_u8; Header::SIZE];
        bytes[..Header::PREFIX.len()].copy_from_slice(Header::PREFIX);
        let mut buf = HeaderBuf(bytes);
        buf.set_object_kind(self.object_kind);
        buf.set_digest_strategy(self.digest_strategy);
        buf.set_encoding_format(self.encoding_format);
        buf
    }
}

/// See [`Header`].
#[derive(Debug, Default, Clone, Copy, Eq, PartialEq, Deserialize, Serialize)]
#[repr(u8)]
pub enum DigestStrategy {
    /// Hash the output of the original spfs encoding, which
    /// has known collision issues. Not recommended for use
    /// except for backwards-compatibility
    Legacy = 0,
    /// Encoding using the original spfs encoding, but adds salt
    /// and the [`ObjectKind`] to mitigate issues found in the
    /// original encoding mechanism
    #[default]
    WithKindAndSalt = 1,
}

impl DigestStrategy {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Legacy),
            1 => Some(Self::WithKindAndSalt),
            2.. => None,
        }
    }
}

/// See [`Header`].
#[derive(Debug, Default, Clone, Copy, Eq, PartialEq, Deserialize, Serialize)]
#[repr(u8)]
pub enum EncodingFormat {
    /// Encode using the original spfs encoding, which uses
    /// a bespoke binary format
    Legacy = 0,
    /// Encode using the [`spfs_proto::AnyObject`] flatbuffers
    /// schema.
    #[default]
    FlatBuffers = 1,
}

impl EncodingFormat {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Legacy),
            1 => Some(Self::FlatBuffers),
            2.. => None,
        }
    }
}

/// A valid variant of the [`spfs_proto::Object`] union type
/// that can be loaded from an [`spfs_proto::AnyObject`] flatbuffer.
#[allow(private_bounds)]
pub trait ObjectProto: private::Sealed {}

mod private {
    /// Seals the [`super::FlatObject`] type from being created
    /// for invalid `flatbuffer` types.
    ///
    /// The [`super::FlatObject`] type is only valid to be generic over
    /// types that are a variant of the [`spfs_proto::Object`] union type
    /// and the higher-level [`spfs_proto::AnyObject`].
    pub(super) trait Sealed {}

    impl<'buf> Sealed for spfs_proto::AnyObject<'buf> {}
    impl<'buf> Sealed for spfs_proto::Platform<'buf> {}
    impl<'buf> Sealed for spfs_proto::Layer<'buf> {}
    impl<'buf> Sealed for spfs_proto::Manifest<'buf> {}
    impl<'buf> Sealed for spfs_proto::Blob<'buf> {}

    impl<T> super::ObjectProto for T where T: Sealed {}
}
