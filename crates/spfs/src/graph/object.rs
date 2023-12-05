// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::io::BufRead;

use strum::Display;

use super::blob::SameAsPayload;
use super::{
    Blob,
    DigestFromEncode,
    DigestFromKindAndEncode,
    Layer,
    Manifest,
    Platform,
    PlatformHandle,
    Tree,
};
use crate::storage::RepositoryHandle;
use crate::{encoding, Error};

#[derive(Debug, Display, Eq, PartialEq, Clone)]
pub enum Object {
    Platform(PlatformHandle),
    Layer(Layer<DigestFromEncode>),
    Manifest(Manifest<DigestFromEncode>),
    Tree(Tree<DigestFromEncode>),
    Blob(Blob<SameAsPayload>),
    Mask,
}

impl Object {
    pub fn child_objects(&self) -> Vec<encoding::Digest> {
        match self {
            Self::Platform(platform) => platform.child_objects(),
            Self::Layer(layer) => layer.child_objects(),
            Self::Manifest(manifest) => manifest.child_objects(),
            Self::Tree(tree) => tree.entries.iter().map(|e| e.object).collect(),
            Self::Blob(_blob) => Vec::new(),
            Self::Mask => Vec::new(),
        }
    }

    /// Return true if this Object kind also has a payload
    pub fn has_payload(&self) -> bool {
        matches!(self, Self::Blob(_))
    }

    /// Calculates the total size of the object and all children, recursively
    pub async fn calculate_object_size(&self, repo: &RepositoryHandle) -> crate::Result<u64> {
        let mut total_size: u64 = 0;
        let mut items_to_process: Vec<Object> = vec![self.clone()];

        while !items_to_process.is_empty() {
            let mut next_iter_objects: Vec<Object> = Vec::new();
            for object in items_to_process.iter() {
                match object {
                    Object::Platform(object) => {
                        for digest in object.stack().iter_bottom_up() {
                            let item = repo.read_object(digest).await?;
                            next_iter_objects.push(item);
                        }
                    }
                    Object::Layer(object) => {
                        let item = repo.read_object(object.manifest).await?;
                        next_iter_objects.push(item);
                    }
                    Object::Manifest(object) => {
                        for node in object.to_tracking_manifest().walk_abs("/spfs") {
                            total_size += node.entry.size
                        }
                    }
                    Object::Tree(object) => {
                        for entry in object.entries.iter() {
                            total_size += entry.size
                        }
                    }
                    Object::Blob(object) => total_size += object.size,
                    Object::Mask => (),
                }
            }
            items_to_process = std::mem::take(&mut next_iter_objects);
        }
        Ok(total_size)
    }
}

impl From<Platform<DigestFromEncode>> for Object {
    fn from(platform: Platform<DigestFromEncode>) -> Self {
        Self::Platform(PlatformHandle::V1(platform))
    }
}
impl From<Platform<DigestFromKindAndEncode>> for Object {
    fn from(platform: Platform<DigestFromKindAndEncode>) -> Self {
        Self::Platform(PlatformHandle::V2(platform))
    }
}
impl From<Layer> for Object {
    fn from(layer: Layer) -> Self {
        Self::Layer(layer)
    }
}
impl From<Manifest> for Object {
    fn from(manifest: Manifest) -> Self {
        Self::Manifest(manifest)
    }
}
impl From<Tree> for Object {
    fn from(tree: Tree) -> Self {
        Self::Tree(tree)
    }
}
impl From<Blob> for Object {
    fn from(blob: Blob) -> Self {
        Self::Blob(blob)
    }
}

/// Identifies the kind of object this is for the purposes of encoding
#[derive(Debug)]
pub enum ObjectKind {
    Blob = 0,
    Manifest = 1,
    Layer = 2,
    PlatformV1 = 3,
    Tree = 4,
    Mask = 5,
    PlatformV2 = 6,
}

impl ObjectKind {
    pub fn from_u64(kind: u64) -> Option<ObjectKind> {
        match kind {
            0 => Some(Self::Blob),
            1 => Some(Self::Manifest),
            2 => Some(Self::Layer),
            3 => Some(Self::PlatformV1),
            4 => Some(Self::Tree),
            5 => Some(Self::Mask),
            6 => Some(Self::PlatformV2),
            _ => None,
        }
    }
}

/// A trait for spfs objects to implement so they can specify their
/// [`ObjectKind`].
pub trait Kind {
    /// Identifies the kind of object this is for the purposes of encoding
    fn kind(&self) -> ObjectKind;
}

impl Kind for Object {
    #[inline]
    fn kind(&self) -> ObjectKind {
        match self {
            Object::Platform(o) => o.kind(),
            Object::Layer(o) => o.kind(),
            Object::Manifest(o) => o.kind(),
            Object::Tree(o) => o.kind(),
            Object::Blob(o) => o.kind(),
            Object::Mask => ObjectKind::Mask,
        }
    }
}

const OBJECT_HEADER: &[u8] = "--SPFS--".as_bytes();

impl encoding::Encodable for Object {
    type Error = Error;

    fn encode(&self, mut writer: &mut impl std::io::Write) -> crate::Result<()>
    where
        Self: Kind,
    {
        encoding::write_header(&mut writer, OBJECT_HEADER)?;
        encoding::write_uint(&mut writer, self.kind() as u64)?;
        match self {
            Self::Blob(obj) => obj.encode(&mut writer),
            Self::Manifest(obj) => obj.encode(&mut writer),
            Self::Layer(obj) => obj.encode(&mut writer),
            Self::Platform(obj) => obj.encode(&mut writer),
            Self::Tree(obj) => obj.encode(&mut writer),
            Self::Mask => Ok(()),
        }
    }
}

impl encoding::Decodable for Object {
    fn decode(mut reader: &mut impl BufRead) -> crate::Result<Self> {
        encoding::consume_header(&mut reader, OBJECT_HEADER)?;
        let type_id = encoding::read_uint(&mut reader)?;
        match ObjectKind::from_u64(type_id) {
            Some(ObjectKind::Blob) => Ok(Self::Blob(Blob::decode(&mut reader)?)),
            Some(ObjectKind::Manifest) => Ok(Self::Manifest(Manifest::decode(&mut reader)?)),
            Some(ObjectKind::Layer) => Ok(Self::Layer(Layer::decode(&mut reader)?)),
            Some(ObjectKind::PlatformV1) => Ok(Self::Platform(PlatformHandle::V1(Platform::<
                DigestFromEncode,
            >::decode(
                &mut reader
            )?))),
            Some(ObjectKind::PlatformV2) => Ok(Self::Platform(PlatformHandle::V2(
                Platform::decode(&mut reader)?,
            ))),
            Some(ObjectKind::Tree) => Ok(Self::Tree(Tree::decode(&mut reader)?)),
            Some(ObjectKind::Mask) => Ok(Self::Mask),
            None => Err(format!("Cannot read object: unknown object kind {type_id}").into()),
        }
    }
}

impl encoding::Digestible for Object {
    type Error = crate::Error;

    fn digest(&self) -> std::result::Result<encoding::Digest, Self::Error> {
        match self {
            Object::Platform(o) => o.digest(),
            Object::Layer(o) => o.digest(),
            Object::Manifest(o) => o.digest(),
            Object::Tree(o) => o.digest(),
            Object::Blob(o) => o.digest(),
            Object::Mask => Ok(encoding::EMPTY_DIGEST.into()),
        }
    }
}
