// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::ops::Deref;

use relative_path::{RelativePath, RelativePathBuf};
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// A suffix on directory names that indicates that the directory is a tag namespace.
pub const TAG_NAMESPACE_MARKER: &str = "#ns";

/// A borrowed tag namespace name
#[repr(transparent)]
#[derive(Debug, PartialEq)]
pub struct TagNamespace(RelativePath);

impl TagNamespace {
    /// Create a new TagNamespace instance.
    ///
    /// The path provided must only contain a single path component.
    pub fn new(path: &RelativePath) -> Result<&Self> {
        TagNamespaceBuf::validate_path(path)?;

        // Safety: TagNamespace is repr(transparent) over RelativePath
        Ok(unsafe { &*(path as *const RelativePath as *const TagNamespace) })
    }

    /// Create a new TagNamespace instance without validation.
    ///
    /// # Safety
    ///
    /// The path provided must only contain a single path component.
    unsafe fn new_unchecked(path: &RelativePath) -> &Self {
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
    pub fn new<P: AsRef<RelativePath>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        TagNamespaceBuf::validate_path(path)?;
        Ok(Self(path.to_owned()))
    }

    /// Validate that a RelativePath is a valid tag namespace name.
    ///
    /// Tag namespaces may not have any directory structure meaning the
    /// directory created for the tag namespace will be created in the top level
    /// of the tags directory in the spfs repository.
    ///
    /// When `spfs clean` walks tags to avoid data loss it must walk all tags is
    /// all namespaces and with the current implementation only tag namespaces
    /// defined in the top level of the tags directory will be discovered.
    #[inline]
    fn validate_path(path: &RelativePath) -> Result<()> {
        let mut it = path.components();
        if it.next().is_none() {
            return Err(Error::String("tag namespace must not be empty".to_string()));
        }
        if it.next().is_some() {
            return Err(Error::String(
                "tag namespace must not contain any directory structure".to_string(),
            ));
        }

        Ok(())
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
        // Safety: A TagNamespaceBuf has already been checked for validity.
        unsafe { TagNamespace::new_unchecked(self.0.deref()) }
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
