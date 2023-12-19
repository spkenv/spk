// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use crate::encoding::Digest;
use crate::{encoding, Result};

/// Blobs represent an arbitrary chunk of binary data, usually a file.
pub type Blob = super::FlatObject<spfs_proto::Blob<'static>>;

impl std::fmt::Debug for Blob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Blob")
            .field("payload", self.payload())
            .field("size", &self.size())
            .finish()
    }
}

impl Blob {
    pub fn new(payload: &Digest, size: u64) -> Self {
        super::BUILDER.with_borrow_mut(|builder| {
            let offset = Self::build(builder, payload, size);
            let obj = unsafe {
                // Safety: the provided buf and offset mut contain
                // a valid object and point to the contained blob
                // which is what we've done
                Blob::with_default_header(builder.finished_data(), offset)
            };
            builder.reset(); // to be used again
            obj
        })
    }

    /// Builds an [`spfs_proto::AnyObject`] that contains
    /// a blob, returning the offset of the blob.
    pub(super) fn build(
        builder: &mut flatbuffers::FlatBufferBuilder<'_>,
        payload: &Digest,
        size: u64,
    ) -> usize {
        let blob = spfs_proto::Blob::create(
            builder,
            &spfs_proto::BlobArgs {
                payload: Some(payload),
                size_: size,
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
        unsafe {
            // Safety: we have just created this buffer
            // so already know the root type with certainty
            flatbuffers::root_unchecked::<spfs_proto::AnyObject>(builder.finished_data())
                .object_as_blob()
                .unwrap()
                ._tab
                .loc()
        }
    }
}

impl Blob {
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
            &encoding::read_digest(&mut reader)?,
            encoding::read_uint64(reader)?,
        ))
    }
}
