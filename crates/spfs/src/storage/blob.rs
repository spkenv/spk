// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::pin::Pin;

use futures::Stream;
use tokio_stream::StreamExt;

use crate::{encoding, graph, Error, Result};

pub type BlobStreamItem = Result<(encoding::Digest, graph::Blob)>;

#[async_trait::async_trait]
pub trait BlobStorage: graph::Database + Sync + Send {
    /// Iterate the objects in this storage which are blobs.
    fn iter_blobs<'db>(&'db self) -> Pin<Box<dyn Stream<Item = BlobStreamItem> + 'db>> {
        let stream = self.iter_objects().filter_map(|res| match res {
            Ok((digest, obj)) => obj.into_blob().map(|b| Ok((digest, b))),
            Err(err) => Some(Err(err)),
        });
        Box::pin(stream)
    }

    /// Return the blob identified by the given digest.
    async fn read_blob(&self, digest: encoding::Digest) -> Result<graph::Blob> {
        match self.read_object(digest).await.map(graph::Object::into_blob) {
            Err(err) => Err(err),
            Ok(Some(blob)) => Ok(blob),
            Ok(None) => Err(Error::NotCorrectKind {
                desired: graph::ObjectKind::Blob,
                digest,
            }),
        }
    }

    /// Store the given blob
    async fn write_blob(&self, blob: graph::Blob) -> Result<()> {
        self.write_object(&blob).await
    }
}

impl<T: BlobStorage> BlobStorage for &T {}
