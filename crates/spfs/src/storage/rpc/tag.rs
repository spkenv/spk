// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::borrow::Cow;
use std::convert::{TryFrom, TryInto};
use std::pin::Pin;
use std::sync::Arc;

use futures::{Stream, TryStreamExt};
use relative_path::RelativePath;

use crate::proto::tag_service_client::TagServiceClient;
use crate::proto::{self, RpcResult};
use crate::storage::tag::TagSpecAndTagStream;
use crate::storage::{self, EntryType, TagNamespace, TagNamespaceBuf};
use crate::{encoding, tracking, Result};

#[async_trait::async_trait]
impl storage::TagStorage for super::RpcRepository {
    #[inline]
    fn get_tag_namespace(&self) -> Option<Cow<'_, TagNamespace>> {
        Self::tag_namespace(self).map(Cow::Borrowed)
    }

    async fn resolve_tag(
        &self,
        tag_spec: &crate::tracking::TagSpec,
    ) -> Result<crate::tracking::Tag> {
        let request = proto::ResolveTagRequest {
            tag_spec: tag_spec.to_string(),
            namespace: self
                .get_tag_namespace()
                .map(|p| p.to_string())
                .unwrap_or_default(),
        };
        let response = self
            .tag_client
            .clone()
            .resolve_tag(request)
            .await?
            .into_inner();
        response.to_result()?.try_into()
    }

    fn ls_tags(
        &self,
        path: &RelativePath,
    ) -> Pin<Box<dyn Stream<Item = Result<EntryType>> + Send>> {
        self.ls_tags_in_namespace(self.get_tag_namespace().as_deref(), path)
    }

    fn ls_tags_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        path: &RelativePath,
    ) -> Pin<Box<dyn Stream<Item = Result<EntryType>> + Send>> {
        let request = proto::LsTagsRequest {
            path: path.to_string(),
            namespace: namespace.map(|p| p.to_string()).unwrap_or_default(),
        };
        let mut client = self.tag_client.clone();
        let stream = futures::stream::once(async move { client.ls_tags(request).await })
            .map_err(crate::Error::from)
            .and_then(|r| async { r.into_inner().to_result() })
            .map_ok(|resp| futures::stream::iter(resp.entries.into_iter().map(TryInto::try_into)))
            .try_flatten();
        Box::pin(stream)
    }

    fn find_tags_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        digest: &encoding::Digest,
    ) -> Pin<Box<dyn Stream<Item = Result<tracking::TagSpec>> + Send>> {
        let request = proto::FindTagsRequest {
            digest: Some(digest.into()),
            namespace: namespace.map(|p| p.to_string()).unwrap_or_default(),
        };
        let mut client = self.tag_client.clone();
        let stream = futures::stream::once(async move { client.find_tags(request).await })
            .map_err(crate::Error::from)
            .and_then(|r| async { r.into_inner().to_result() })
            .map_ok(|tag_list| {
                futures::stream::iter(tag_list.tags.into_iter().map(tracking::TagSpec::parse))
            })
            .try_flatten();
        Box::pin(stream)
    }

    fn iter_tag_streams_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
    ) -> Pin<Box<dyn Stream<Item = Result<TagSpecAndTagStream>> + Send>> {
        let request = proto::IterTagSpecsRequest {
            namespace: namespace.map(|p| p.to_string()).unwrap_or_default(),
        };
        let mut client = self.tag_client.clone();
        let stream = futures::stream::once(async move { client.iter_tag_specs(request).await })
            .map_err(crate::Error::from)
            .and_then(|r| async { r.into_inner().to_result() })
            .map_ok(|response| {
                futures::stream::iter(response.tag_specs.into_iter().map(tracking::TagSpec::parse))
            })
            .try_flatten();
        let client = self.tag_client.clone();
        let tag_namespace = Arc::new(namespace.map(ToOwned::to_owned));
        let stream = stream.and_then(move |spec| {
            let client = client.clone();
            let tag_namespace = Arc::clone(&tag_namespace);
            async move {
                match read_tag(client, tag_namespace.as_deref(), &spec).await {
                    Ok(tags) => Ok((spec, tags)),
                    Err(err) => Err(err),
                }
            }
        });

        Box::pin(stream)
    }

    async fn read_tag_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        tag: &tracking::TagSpec,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<tracking::Tag>> + Send>>> {
        read_tag(self.tag_client.clone(), namespace, tag).await
    }

    async fn insert_tag_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        tag: &tracking::Tag,
    ) -> Result<()> {
        let request = proto::InsertTagRequest {
            tag: Some(tag.into()),
            namespace: namespace.map(|p| p.to_string()).unwrap_or_default(),
        };
        let _response = self
            .tag_client
            .clone()
            .insert_tag(request)
            .await?
            .into_inner()
            .to_result()?;
        Ok(())
    }

    async fn remove_tag_stream_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        tag: &tracking::TagSpec,
    ) -> Result<()> {
        let request = proto::RemoveTagStreamRequest {
            tag_spec: tag.to_string(),
            namespace: namespace.map(|p| p.to_string()).unwrap_or_default(),
        };
        let _response = self
            .tag_client
            .clone()
            .remove_tag_stream(request)
            .await?
            .into_inner()
            .to_result()?;
        Ok(())
    }

    async fn remove_tag_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        tag: &tracking::Tag,
    ) -> Result<()> {
        let request = proto::RemoveTagRequest {
            tag: Some(tag.into()),
            namespace: namespace.map(|p| p.to_string()).unwrap_or_default(),
        };
        let _response = self
            .tag_client
            .clone()
            .remove_tag(request)
            .await?
            .into_inner()
            .to_result()?;
        Ok(())
    }
}

impl storage::TagStorageMut for super::RpcRepository {
    fn try_set_tag_namespace(
        &mut self,
        tag_namespace: Option<TagNamespaceBuf>,
    ) -> Result<Option<TagNamespaceBuf>> {
        Ok(Self::set_tag_namespace(self, tag_namespace))
    }
}

async fn read_tag(
    mut client: TagServiceClient<tonic::transport::Channel>,
    tag_namespace: Option<&TagNamespace>,
    tag: &tracking::TagSpec,
) -> Result<Pin<Box<dyn Stream<Item = Result<tracking::Tag>> + Send>>> {
    let request = proto::ReadTagRequest {
        tag_spec: tag.to_string(),
        namespace: tag_namespace.map(|p| p.to_string()).unwrap_or_default(),
    };
    let response = client.read_tag(request).await?.into_inner().to_result()?;
    let items: Result<Vec<_>> = response
        .tags
        .into_iter()
        .map(tracking::Tag::try_from)
        .collect();
    Ok(Box::pin(futures::stream::iter(items?.into_iter().map(Ok))))
}
