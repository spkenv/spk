// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::pin::Pin;

use futures::{Stream, StreamExt};
use relative_path::RelativePath;

use super::proto;
use crate::{
    encoding,
    storage::{self, tag::TagSpecAndTagStream},
    tracking, Result,
};

#[async_trait::async_trait]
impl storage::TagStorage for super::RpcRepository {
    fn ls_tags(&self, path: &RelativePath) -> Pin<Box<dyn Stream<Item = Result<String>> + Send>> {
        let request = proto::LsTagsRequest {
            path: path.to_string(),
        };
        let mut client = self.tag_client.clone();
        let stream = futures::stream::once(async move {
            tracing::trace!("sending request");
            client.ls_tags(request).await
        })
        .map(|resp| {
            tracing::trace!("recevied resp");
            let resp = resp.unwrap().into_inner();
            futures::stream::iter(resp.entries.into_iter().map(Ok))
        })
        .flatten();
        Box::pin(stream)
    }

    fn find_tags(
        &self,
        _digest: &encoding::Digest,
    ) -> Pin<Box<dyn Stream<Item = Result<tracking::TagSpec>> + Send>> {
        todo!()
    }

    fn iter_tag_streams(&self) -> Pin<Box<dyn Stream<Item = Result<TagSpecAndTagStream>> + Send>> {
        todo!()
    }

    async fn read_tag(
        &self,
        _tag: &tracking::TagSpec,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<tracking::Tag>> + Send>>> {
        todo!()
    }

    async fn push_raw_tag(&self, _tag: &tracking::Tag) -> Result<()> {
        todo!()
    }

    async fn remove_tag_stream(&self, _tag: &tracking::TagSpec) -> Result<()> {
        todo!()
    }

    async fn remove_tag(&self, _tag: &tracking::Tag) -> Result<()> {
        todo!()
    }
}
