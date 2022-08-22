// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::convert::TryInto;
use std::pin::Pin;

use futures::{Stream, TryStreamExt};

use crate::{encoding, graph, proto, storage, Result};
use proto::RpcResult;

#[async_trait::async_trait]
impl graph::DatabaseView for super::RpcRepository {
    async fn read_object(&self, digest: encoding::Digest) -> Result<graph::Object> {
        let request = proto::ReadObjectRequest {
            digest: Some(digest.into()),
        };
        let obj = self
            .db_client
            .clone()
            .read_object(request)
            .await?
            .into_inner()
            .to_result()?;
        obj.try_into()
    }

    fn find_digests(
        &self,
        search_criteria: graph::DigestSearchCriteria,
    ) -> Pin<Box<dyn Stream<Item = Result<encoding::Digest>> + Send>> {
        let request = proto::FindDigestsRequest {
            search_criteria: Some(search_criteria.into()),
        };
        let mut client = self.db_client.clone();
        let stream = futures::stream::once(async move { client.find_digests(request).await })
            .map_err(crate::Error::from)
            .map_ok(|r| r.into_inner().map_err(crate::Error::from))
            .try_flatten()
            .and_then(|d| async { d.to_result() })
            .and_then(|d| async { d.try_into() });
        Box::pin(stream)
    }

    fn iter_objects(&self) -> graph::DatabaseIterator<'_> {
        graph::DatabaseIterator::new(self)
    }

    fn walk_objects<'db>(&'db self, root: &encoding::Digest) -> graph::DatabaseWalker<'db> {
        graph::DatabaseWalker::new(self, *root)
    }
}

#[async_trait::async_trait]
impl graph::Database for super::RpcRepository {
    async fn write_object(&self, obj: &graph::Object) -> Result<()> {
        let request = proto::WriteObjectRequest {
            object: Some(obj.into()),
        };
        self.db_client
            .clone()
            .write_object(request)
            .await?
            .into_inner()
            .to_result()?;
        Ok(())
    }

    async fn remove_object(&self, digest: encoding::Digest) -> Result<()> {
        let request = proto::RemoveObjectRequest {
            digest: Some(digest.into()),
        };
        self.db_client
            .clone()
            .remove_object(request)
            .await?
            .into_inner()
            .to_result()?;
        Ok(())
    }
}

impl storage::PlatformStorage for super::RpcRepository {}
impl storage::LayerStorage for super::RpcRepository {}
impl storage::ManifestStorage for super::RpcRepository {}
impl storage::BlobStorage for super::RpcRepository {}
