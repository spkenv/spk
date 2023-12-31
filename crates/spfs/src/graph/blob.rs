// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use super::object::HeaderBuilder;
use super::ObjectKind;
use crate::encoding::Digest;
use crate::{encoding, Result};

/// Blobs represent an arbitrary chunk of binary data, usually a file.
pub type Blob = super::FlatObject<spfs_proto::Blob<'static>>;

impl std::fmt::Debug for Blob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Blob")
            .field("payload", &self.payload().to_string())
            .field("size", &self.size())
            .finish()
    }
}

impl Blob {
    /// Construct a new blob with default header values,
    /// for more configuration use [`Self::builder`]
    pub fn new(payload: Digest, size: u64) -> Self {
        Self::builder()
            .with_payload(payload)
            .with_size(size)
            .build()
    }

    pub fn builder() -> BlobBuilder {
        BlobBuilder::default()
    }

    #[inline]
    pub fn digest(&self) -> &Digest {
        self.proto().payload()
    }

    #[inline]
    pub fn payload(&self) -> &Digest {
        self.digest()
    }

    #[inline]
    pub fn size(&self) -> u64 {
        self.proto().size_()
    }

    pub(super) fn legacy_encode(&self, mut writer: &mut impl std::io::Write) -> Result<()> {
        encoding::write_digest(&mut writer, self.payload())?;
        encoding::write_uint64(writer, self.size())?;
        Ok(())
    }

    pub(super) fn legacy_decode(mut reader: &mut impl std::io::Read) -> Result<Self> {
        Ok(Self::new(
            encoding::read_digest(&mut reader)?,
            encoding::read_uint64(reader)?,
        ))
    }
}

#[derive(Debug)]
pub struct BlobBuilder {
    header: super::object::HeaderBuilder,
    payload: encoding::Digest,
    size: u64,
}

impl Default for BlobBuilder {
    fn default() -> Self {
        Self {
            header: super::object::HeaderBuilder::new(ObjectKind::Blob),
            payload: Default::default(),
            size: Default::default(),
        }
    }
}

impl BlobBuilder {
    pub fn with_header<F>(mut self, mut header: F) -> Self
    where
        F: FnMut(HeaderBuilder) -> HeaderBuilder,
    {
        self.header = header(self.header).with_object_kind(ObjectKind::Blob);
        self
    }

    pub fn with_payload(mut self, payload: Digest) -> Self {
        self.payload = payload;
        self
    }

    pub fn with_size(mut self, size: u64) -> Self {
        self.size = size;
        self
    }

    pub fn build(&self) -> Blob {
        super::BUILDER.with_borrow_mut(|builder| {
            let blob = spfs_proto::Blob::create(
                builder,
                &spfs_proto::BlobArgs {
                    payload: Some(&self.payload),
                    size_: self.size,
                },
            );
            let any = spfs_proto::AnyObject::create(
                builder,
                &spfs_proto::AnyObjectArgs {
                    object_type: spfs_proto::Object::Blob,
                    object: Some(blob.as_union_value()),
                },
            );
            builder.finish_minimal(any);
            let offset = unsafe {
                // Safety: we have just created this buffer
                // so already know the root type with certainty
                flatbuffers::root_unchecked::<spfs_proto::AnyObject>(builder.finished_data())
                    .object_as_blob()
                    .unwrap()
                    ._tab
                    .loc()
            };
            let obj = unsafe {
                // Safety: the provided buf and offset mut contain
                // a valid object and point to the contained blob
                // which is what we've done
                Blob::new_with_header(self.header.build(), builder.finished_data(), offset)
            };
            builder.reset(); // to be used again
            obj
        })
    }
}
