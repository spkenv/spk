use crate::encoding;
use crate::Result;

/// Stores arbitrary binary data payloads using their content digest.
pub trait PayloadStorage {
    /// Iterate all the object in this database.
    fn iter_digests(&self) -> Box<dyn Iterator<Item = Result<encoding::Digest>>>;

    /// Return true if the identified payload exists.
    fn has_payload(&self, digest: &encoding::Digest) -> bool {
        match self.open_payload(digest) {
            Ok(_) => true,
            Err(_) => false,
        }
    }

    /// Store the contents of the given stream, returning its digest and size
    fn write_payload(&mut self, reader: &mut impl std::io::Read)
        -> Result<(encoding::Digest, u64)>;

    /// Return a handle to the full content of a payload.
    ///
    /// # Errors:
    /// - [`spfs::graph::UnknownObjectError`]: if the payload does not exist in this storage
    fn open_payload(&self, digest: &encoding::Digest) -> Result<Box<dyn std::io::Read>>;

    /// Remove the payload idetified by the given digest.
    ///
    /// Errors:
    /// - [`spfs::graph::UnknownObjectError`]: if the payload does not exist in this storage
    fn remove_payload(&mut self, digest: &encoding::Digest) -> Result<()>;
}
