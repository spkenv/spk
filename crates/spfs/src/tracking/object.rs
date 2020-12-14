use crate::encoding;

use super::entry::Entry;
use super::manifest::Manifest;
use super::tag::Tag;

/// Object is the base class for all storable data types.
///
/// Objects are identified by a hash of their contents, and
/// can have any number of immediate children that they reference.
pub enum Object {
    Manifest(Manifest),
    Tag(Tag),
    Entry(Entry),
}

impl Object {
    /// Identify the set of children to this object in the graph.
    pub fn child_objects<'a>(&'a self) -> std::vec::IntoIter<&'a encoding::Digest> {
        let empty = Vec::new();
        empty.into_iter()
    }
}

impl encoding::Encodable for Object {
    fn encode(&self, _writer: &mut impl std::io::Write) -> crate::Result<()> {
        todo!()
    }

    fn decode(_reader: &mut impl std::io::Read) -> crate::Result<Self> {
        todo!()
    }
}
