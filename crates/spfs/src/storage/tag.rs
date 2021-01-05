use crate::{encoding, tracking, Result};
use relative_path::RelativePath;

/// A location where tags are tracked and persisted.
pub trait TagStorage {
    /// Return true if the given tag exists in this storage.
    fn has_tag(&self, tag: &tracking::TagSpec) -> bool {
        match self.resolve_tag(tag) {
            Ok(_) => true,
            Err(_) => false,
        }
    }

    /// Return the digest identified by the given tag spec.
    ///
    /// # Errors:
    /// - if the tag does not exist in this storage
    fn resolve_tag(&self, tag_spec: &tracking::TagSpec) -> Result<tracking::Tag>;

    /// List tags and tag directories based on a tag path.
    ///
    /// For example, if the repo contains the following tags
    ///   spi/stable/my_tag
    ///   spi/stable/other_tag
    ///   spi/latest/my_tag
    /// Then ls_tags("spi") would return
    ///   stable
    ///   latest
    fn ls_tags<R: AsRef<RelativePath>>(&self, path: R) -> Result<Box<dyn Iterator<Item = String>>>;

    /// Find tags that point to the given digest.
    fn find_tags(
        &self,
        digest: &encoding::Digest,
    ) -> Box<dyn Iterator<Item = Result<tracking::TagSpec>>>;

    /// Iterate through the available tags in this storage.
    fn iter_tags(&self) -> Box<dyn Iterator<Item = Result<(tracking::TagSpec, tracking::Tag)>>> {
        Box::new(self.iter_tag_streams().filter_map(|res| match res {
            Ok((spec, mut stream)) => match stream.next() {
                Some(next) => Some(Ok((spec, next))),
                None => None,
            },
            Err(err) => Some(Err(err)),
        }))
    }

    /// Iterate through the available tags in this storage by stream.
    fn iter_tag_streams(
        &self,
    ) -> Box<
        dyn Iterator<Item = Result<(tracking::TagSpec, Box<dyn Iterator<Item = tracking::Tag>>)>>,
    >;

    /// Read the entire tag stream for the given tag.
    ///
    /// # Errors:
    /// - if the tag does not exist in the storage
    fn read_tag(&self, tag: &tracking::TagSpec) -> Result<Box<dyn Iterator<Item = tracking::Tag>>>;

    /// Push the given tag onto the tag stream.
    fn push_tag(&self, tag: &tracking::TagSpec, target: encoding::Digest) -> Result<tracking::Tag> {
        todo!()
        //     tag_spec = tracking.TagSpec(tag)
        //     parent: Optional[tracking.Tag] = None
        //     try:
        //         parent = self.resolve_tag(tag)
        //     except ValueError:
        //         pass

        //     parent_ref = encoding.NULL_DIGEST
        //     if parent is not None:
        //         if parent.target == target:
        //             return parent
        //         parent_ref = parent.digest()

        //     new_tag = tracking.Tag(
        //         org=tag_spec.org, name=tag_spec.name, target=target, parent=parent_ref
        //     )
        //     self.push_raw_tag(new_tag)
        //     return new_tag
    }

    /// Push the given tag data to the tag stream, regardless of if it's valid.
    fn push_raw_tag(&self, tag: &tracking::Tag) -> Result<()>;

    /// Remove an entire tag and all related tag history.
    ///
    /// If the given tag spec contains a version, the version is ignored.
    fn remove_tag_stream(&self, tag: &tracking::TagSpec) -> Result<()>;

    /// Remove the oldest stored instance of the given tag.
    fn remove_tag(&self, tag: &tracking::Tag) -> Result<()>;
}
