// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

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
    pub fn child_objects(&self) -> std::vec::IntoIter<&encoding::Digest> {
        let empty = Vec::new();
        empty.into_iter()
    }
}
