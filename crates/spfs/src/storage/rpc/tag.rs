use std::pin::Pin;

use futures::Stream;

use crate::{
    encoding,
    storage::{self, tag::TagSpecAndTagStream},
    tracking,
    Result
};

#[async_trait::async_trait]
impl storage::TagStorage for super::RpcRepository {
    fn ls_tags(
        &self,
        _path: &relative_path::RelativePath,
    ) -> Pin<Box<dyn Stream<Item = Result<String>> + Send>> {
        todo!()
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
