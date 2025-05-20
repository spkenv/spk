// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::ops::Deref;

use relative_path::{RelativePath, RelativePathBuf};
use serde::{Deserialize, Serialize};

/// A suffix on directory names that indicates that the directory is a tag namespace.
pub const TAG_NAMESPACE_MARKER: &str = "#ns";

/// A borrowed tag namespace name
#[repr(transparent)]
#[derive(Debug)]
pub struct TagNamespace(RelativePath);

impl TagNamespace {
    pub fn new(path: &RelativePath) -> &Self {
        // Safety: TagNamespace is repr(transparent) over RelativePath
        unsafe { &*(path as *const RelativePath as *const TagNamespace) }
    }

    /// Borrow the tag namespace as a relative path.
    pub fn as_rel_path(&self) -> &RelativePath {
        &self.0
    }
}

impl std::borrow::ToOwned for TagNamespace {
    type Owned = TagNamespaceBuf;

    fn to_owned(&self) -> Self::Owned {
        TagNamespaceBuf(self.0.to_owned())
    }
}

impl std::fmt::Display for TagNamespace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// An owned tag namespace name
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct TagNamespaceBuf(RelativePathBuf);

impl TagNamespaceBuf {
    #[inline]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Create a new tag namespace from the given path.
    pub fn new<P: AsRef<RelativePath>>(path: P) -> Self {
        Self(path.as_ref().to_owned())
    }
}

impl std::borrow::Borrow<TagNamespace> for TagNamespaceBuf {
    fn borrow(&self) -> &TagNamespace {
        self.deref()
    }
}

impl std::fmt::Display for TagNamespaceBuf {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::ops::Deref for TagNamespaceBuf {
    type Target = TagNamespace;

    fn deref(&self) -> &Self::Target {
        TagNamespace::new(self.0.deref())
    }
}

impl From<&str> for TagNamespaceBuf {
    fn from(path: &str) -> Self {
        Self(RelativePathBuf::from(path))
    }
}

impl From<String> for TagNamespaceBuf {
    fn from(path: String) -> Self {
        Self(RelativePathBuf::from(path))
    }
}
