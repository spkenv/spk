// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::pin::Pin;

use chrono::{DateTime, Utc};
use futures::Stream;

use crate::tracking::BlobRead;
use crate::{Result, encoding};

#[cfg(test)]
#[path = "payload_test.rs"]
mod payload_test;

/// Stores arbitrary binary data payloads using their content digest.
#[async_trait::async_trait]
pub trait PayloadStorage: Sync + Send {
    /// Iterate all the payloads in this storage.
    fn iter_payload_digests(&self) -> Pin<Box<dyn Stream<Item = Result<encoding::Digest>> + Send>>;

    /// Return true if the identified payload exists.
    async fn has_payload(&self, digest: encoding::Digest) -> bool;

    /// Return the payload size if the identified payload exists.
    async fn payload_size(&self, digest: encoding::Digest) -> Result<u64>;

    /// Store the contents of the given stream, returning its digest and size
    async fn write_data(&self, reader: Pin<Box<dyn BlobRead>>) -> Result<(encoding::Digest, u64)>;

    /// Return a handle and filename to the full content of a payload.
    ///
    /// # Errors:
    /// - [`crate::Error::UnknownObject`]: if the payload does not exist in this storage
    async fn open_payload(
        &self,
        digest: encoding::Digest,
    ) -> Result<(Pin<Box<dyn BlobRead>>, std::path::PathBuf)>;

    /// Remove the payload identified by the given digest.
    ///
    /// Errors:
    /// - [`crate::Error::UnknownObject`]: if the payload does not exist in this storage
    async fn remove_payload(&self, digest: encoding::Digest) -> Result<()>;

    /// Remove the payload identified by the given digest.
    ///
    /// It is only removed if it is older than the given timestamp. Returns true
    /// if the payload was removed, false if it was not.
    ///
    /// Errors:
    /// - [`crate::Error::UnknownObject`]: if the payload does not exist in this storage
    async fn remove_payload_if_older_than(
        &self,
        older_than: DateTime<Utc>,
        digest: encoding::Digest,
    ) -> Result<bool>;
}

#[async_trait::async_trait]
impl<T: PayloadStorage> PayloadStorage for &T {
    async fn has_payload(&self, digest: encoding::Digest) -> bool {
        PayloadStorage::has_payload(&**self, digest).await
    }

    async fn payload_size(&self, digest: encoding::Digest) -> Result<u64> {
        PayloadStorage::payload_size(&**self, digest).await
    }

    fn iter_payload_digests(&self) -> Pin<Box<dyn Stream<Item = Result<encoding::Digest>> + Send>> {
        PayloadStorage::iter_payload_digests(&**self)
    }

    async fn write_data(&self, reader: Pin<Box<dyn BlobRead>>) -> Result<(encoding::Digest, u64)> {
        PayloadStorage::write_data(&**self, reader).await
    }

    async fn open_payload(
        &self,
        digest: encoding::Digest,
    ) -> Result<(Pin<Box<dyn BlobRead>>, std::path::PathBuf)> {
        PayloadStorage::open_payload(&**self, digest).await
    }

    async fn remove_payload(&self, digest: encoding::Digest) -> Result<()> {
        PayloadStorage::remove_payload(&**self, digest).await
    }

    async fn remove_payload_if_older_than(
        &self,
        older_than: DateTime<Utc>,
        digest: encoding::Digest,
    ) -> Result<bool> {
        PayloadStorage::remove_payload_if_older_than(&**self, older_than, digest).await
    }
}
