// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::io::BufRead;
use std::marker::PhantomData;

use bytes::BufMut;
use encoding::prelude::*;

use super::error::{ObjectError, ObjectResult};
use super::{Blob, DatabaseView, HasKind, Kind, Layer, Manifest, ObjectKind, Platform};
use crate::encoding;
use crate::storage::RepositoryHandle;

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
        let Some(kind) = ObjectKind::from_u8(header.object_kind()) else {
            return Err(ObjectError::UnexpectedKind(header.object_kind()).into());
        };
        let object = match header.encoding_format() {
            0 => {
                let mut reader = std::io::BufReader::new(&bytes[Header::SIZE..]);
                match kind {
                    ObjectKind::Blob => Blob::legacy_decode(&mut reader)?.into_object(),
                    ObjectKind::Manifest => Manifest::legacy_decode(&mut reader)?.into_object(),
                    ObjectKind::Layer => Layer::legacy_decode(&mut reader)?.into_object(),
                    ObjectKind::Platform => Platform::legacy_decode(&mut reader)?.into_object(),
                    ObjectKind::Tree | ObjectKind::Mask => {
                        // although these kinds used to be supported, they were never actually encoded
                        // separately into files and so should not appear in this context
                        return Err(ObjectError::UnexpectedKind(kind as u8).into());
                    }
                }
            }
            e => return Err(ObjectError::UnknownEncoding(e).into()),
        };
        // TODO: set the header bits from what was loaded
        //object.set_header(header);
        Ok(object)
    }

    /// Constructs a new [`Object`] instance from the provided flatbuffer.
    ///
    /// # Safety
    /// `buf` must contain a valid flatbuffer with an [`spfs_proto::AnyObject`]
    /// at its root of the provided kind.
    pub unsafe fn with_default_header(buf: &[u8], kind: ObjectKind) -> Self {
        let mut bytes = bytes::BytesMut::with_capacity(buf.len() + Header::SIZE);
        bytes.put(&Header::default_bytes(kind)[..]);
        bytes.put(buf);
        Self {
            buf: bytes.freeze(),
            offset: 0,
            _t: PhantomData,
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
        ObjectKind::from_u8(self.header().object_kind()).expect("buffer already validated")
    }
}

impl<T: ObjectProto> encoding::Digestible for FlatObject<T> {
    type Error = crate::Error;

    fn digest(&self) -> crate::Result<encoding::Digest> {
        let mut hasher = encoding::Hasher::new_sync();
        match self.to_enum() {
            Enum::Platform(obj) => obj.legacy_encode(&mut hasher)?,
            Enum::Layer(obj) => obj.legacy_encode(&mut hasher)?,
            Enum::Manifest(obj) => obj.legacy_encode(&mut hasher)?,
            Enum::Blob(obj) => return Ok(*obj.payload()),
        }
        Ok(hasher.digest())
    }
}

impl<T: ObjectProto> encoding::Encodable for FlatObject<T> {
    type Error = crate::Error;

    fn encode(&self, mut writer: &mut impl std::io::Write) -> crate::Result<()> {
        #[cfg(debug_assertions)]
        Header::new(&self.buf[..]).expect("should have a valid header when writing");
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
    pub unsafe fn with_default_header(buf: &[u8], offset: usize) -> Self {
        let mut bytes = bytes::BytesMut::with_capacity(buf.len() + Header::SIZE);
        bytes.put(&Header::default_bytes(T::kind())[..]);
        bytes.put(buf);
        Self {
            buf: bytes.freeze(),
            offset,
            _t: PhantomData,
        }
    }
}

impl<T: ObjectProto> FlatObject<T> {
    #[inline]
    pub fn header(&self) -> Header<'_> {
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
    pub fn to_object(&self) -> Object {
        self.clone().into_object()
    }

    /// Clone (cheaply) this object and identify its type
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
pub struct Header<'buf>(&'buf [u8]);

impl<'buf> Header<'buf> {
    /// A special string required for all headers
    const PREFIX: &'static [u8] = "--SPFS--\n".as_bytes();
    /// A fixed-size prefix followed by 8 separated byte fields
    /// (this was originally a single u64)
    const SIZE: usize = Self::PREFIX.len() + std::mem::size_of::<u64>();
    const DIGEST_OFFSET: usize = Self::PREFIX.len();
    const ENCODING_OFFSET: usize = Self::PREFIX.len() + 1;
    const KIND_OFFSET: usize = Self::PREFIX.len() + 7;

    /// Default header/object settings when no previous opinion
    /// exists (such as loading from existing storage)
    pub fn default_bytes(kind: ObjectKind) -> [u8; Header::SIZE] {
        let mut header = [0; Self::SIZE];
        header[..Self::PREFIX.len()].copy_from_slice(Self::PREFIX);
        header[Self::KIND_OFFSET] = kind as u8;
        header
    }

    /// Read the first bytes of `buf` as a header
    ///
    /// The buffer can be be longer than just a header
    /// as only the initial header bytes will be validated
    pub fn new(buf: &'buf [u8]) -> ObjectResult<Self> {
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
    pub unsafe fn new_unchecked(buf: &'buf [u8]) -> Self {
        Self(buf)
    }

    pub fn digest_strategy(&self) -> u8 {
        self.0[Self::DIGEST_OFFSET]
    }

    pub fn encoding_format(&self) -> u8 {
        self.0[Self::ENCODING_OFFSET]
    }

    pub fn object_kind(&self) -> u8 {
        self.0[Self::KIND_OFFSET]
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
