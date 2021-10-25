// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::pin::Pin;

use futures::Stream;

use crate::{encoding, graph, storage, Result};

#[async_trait::async_trait]
impl graph::DatabaseView for super::RpcRepository {
    async fn read_object(&self, _digest: encoding::Digest) -> Result<graph::Object> {
        todo!()
    }

    fn iter_digests(&self) -> Pin<Box<dyn Stream<Item = Result<encoding::Digest>> + Send>> {
        todo!()
    }

    fn iter_objects(&self) -> graph::DatabaseIterator<'_> {
        todo!()
    }

    fn walk_objects<'db>(&'db self, _root: &encoding::Digest) -> graph::DatabaseWalker<'db> {
        todo!()
    }
}

#[async_trait::async_trait]
impl graph::Database for super::RpcRepository {
    async fn write_object(&self, _obj: &graph::Object) -> Result<()> {
        todo!()
    }

    async fn remove_object(&self, _digest: encoding::Digest) -> Result<()> {
        todo!()
    }
}

impl storage::PlatformStorage for super::RpcRepository {}
impl storage::LayerStorage for super::RpcRepository {}
impl storage::ManifestStorage for super::RpcRepository {}
impl storage::BlobStorage for super::RpcRepository {}
