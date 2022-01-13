// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::pin::Pin;

use futures::Stream;

use crate::encoding;
use crate::Result;

/// Stores arbitrary binary data payloads using their content digest.
#[async_trait::async_trait]
pub trait PayloadStorage {
    /// Iterate all the payloads in this storage.
    fn iter_payload_digests(&self) -> Pin<Box<dyn Stream<Item = Result<encoding::Digest>>>>;

    /// Return true if the identified payload exists.
    fn has_payload(&self, digest: &encoding::Digest) -> bool {
        self.open_payload(digest).is_ok()
    }

    /// Store the contents of the given stream, returning its digest and size
    fn write_data(
        &mut self,
        reader: Box<dyn std::io::Read + Send + 'static>,
    ) -> Result<(encoding::Digest, u64)>;

    /// Return a handle to the full content of a payload.
    ///
    /// # Errors:
    /// - [`spfs::Error::UnknownObject`]: if the payload does not exist in this storage
    fn open_payload(
        &self,
        digest: &encoding::Digest,
    ) -> Result<Box<dyn std::io::Read + Send + 'static>>;

    /// Remove the payload idetified by the given digest.
    ///
    /// Errors:
    /// - [`spfs::Error::UnknownObject`]: if the payload does not exist in this storage
    fn remove_payload(&mut self, digest: &encoding::Digest) -> Result<()>;
}

impl<T: PayloadStorage> PayloadStorage for &mut T {
    fn iter_payload_digests(&self) -> Pin<Box<dyn Stream<Item = Result<encoding::Digest>>>> {
        PayloadStorage::iter_payload_digests(&**self)
    }

    fn write_data(
        &mut self,
        reader: Box<dyn std::io::Read + Send + 'static>,
    ) -> Result<(encoding::Digest, u64)> {
        PayloadStorage::write_data(&mut **self, reader)
    }

    fn open_payload(
        &self,
        digest: &encoding::Digest,
    ) -> Result<Box<dyn std::io::Read + Send + 'static>> {
        PayloadStorage::open_payload(&**self, digest)
    }

    fn remove_payload(&mut self, digest: &encoding::Digest) -> Result<()> {
        PayloadStorage::remove_payload(&mut **self, digest)
    }
}
