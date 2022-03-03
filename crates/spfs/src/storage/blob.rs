// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::pin::Pin;

use futures::Stream;
use tokio_stream::StreamExt;

use crate::{encoding, graph, Result};

pub type BlobStreamItem = Result<(encoding::Digest, graph::Blob)>;

#[async_trait::async_trait]
pub trait BlobStorage: graph::Database + Sync + Send {
    /// Iterate the objects in this storage which are blobs.
    fn iter_blobs<'db>(&'db self) -> Pin<Box<dyn Stream<Item = BlobStreamItem> + 'db>> {
        use graph::Object;
        let stream = self.iter_objects().filter_map(|res| match res {
            Ok((digest, obj)) => match obj {
                Object::Blob(manifest) => Some(Ok((digest, manifest))),
                _ => None,
            },
            Err(err) => Some(Err(err)),
        });
        Box::pin(stream)
    }

    /// Return true if the identified blob exists in this storage.
    async fn has_blob(&self, digest: encoding::Digest) -> bool {
        self.read_blob(digest).await.is_ok()
    }

    /// Return the blob identified by the given digest.
    async fn read_blob(&self, digest: encoding::Digest) -> Result<graph::Blob> {
        use graph::Object;
        match self.read_object(digest).await {
            Err(err) => Err(err),
            Ok(Object::Blob(blob)) => Ok(blob),
            Ok(_) => Err(format!("Object is not a blob: {:?}", digest).into()),
        }
    }

    /// Store the given blob
    async fn write_blob(&self, blob: graph::Blob) -> Result<()> {
        self.write_object(&graph::Object::Blob(blob)).await
    }
}

impl<T: BlobStorage> BlobStorage for &T {}
