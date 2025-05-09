// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use strum::IntoEnumIterator;

/// Identifies the kind of object this is for the purposes of encoding
#[derive(Debug, Clone, Copy, Eq, PartialEq, strum::EnumIter)]
pub enum ObjectKind {
    Blob = 0,
    Manifest = 1,
    Layer = 2,
    Platform = 3,
    Tree = 4,
    Mask = 5,
}

impl ObjectKind {
    #[inline]
    pub fn from_u8(kind: u8) -> Option<ObjectKind> {
        Self::iter().find(|v| *v as u8 == kind)
    }

    pub fn from(kind: spfs_proto::Object) -> Option<ObjectKind> {
        match kind {
            x if x == spfs_proto::Object::Blob => Some(Self::Blob),
            x if x == spfs_proto::Object::Manifest => Some(Self::Manifest),
            x if x == spfs_proto::Object::Layer => Some(Self::Layer),
            x if x == spfs_proto::Object::Platform => Some(Self::Platform),
            _ => None,
        }
    }
}

/// A trait for spfs object types that have an inherent [`ObjectKind`].
pub trait Kind {
    /// The kind of this object
    fn kind() -> ObjectKind;
}

/// An object instance with an associated [`ObjectKind`].
pub trait HasKind {
    /// Identifies the kind of object this is for the purposes of encoding
    fn kind(&self) -> ObjectKind;
}

impl Kind for spfs_proto::Platform<'_> {
    #[inline]
    fn kind() -> ObjectKind {
        ObjectKind::Platform
    }
}

impl HasKind for spfs_proto::Platform<'_> {
    #[inline]
    fn kind(&self) -> ObjectKind {
        <Self as Kind>::kind()
    }
}

impl Kind for spfs_proto::Layer<'_> {
    #[inline]
    fn kind() -> ObjectKind {
        ObjectKind::Layer
    }
}

impl HasKind for spfs_proto::Layer<'_> {
    #[inline]
    fn kind(&self) -> ObjectKind {
        <Self as Kind>::kind()
    }
}

impl Kind for spfs_proto::Manifest<'_> {
    #[inline]
    fn kind() -> ObjectKind {
        ObjectKind::Manifest
    }
}

impl HasKind for spfs_proto::Manifest<'_> {
    #[inline]
    fn kind(&self) -> ObjectKind {
        <Self as Kind>::kind()
    }
}

impl Kind for spfs_proto::Blob<'_> {
    #[inline]
    fn kind() -> ObjectKind {
        ObjectKind::Blob
    }
}

impl HasKind for spfs_proto::Blob<'_> {
    #[inline]
    fn kind(&self) -> ObjectKind {
        <Self as Kind>::kind()
    }
}
