// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::io::BufRead;

use strum::Display;

use super::{Blob, Layer, Manifest, Platform, Tree};
use crate::storage::RepositoryHandle;
use crate::{encoding, Error};

#[derive(Debug, Display, Eq, PartialEq, Clone)]
pub enum Object {
    Platform(Platform),
    Layer(Layer),
    Manifest(Manifest),
    Tree(Tree),
    Blob(Blob),
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
                        for digest in object.stack.iter_bottom_up() {
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

impl From<Platform> for Object {
    fn from(platform: Platform) -> Self {
        Self::Platform(platform)
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
    Platform = 3,
    Tree = 4,
    Mask = 5,
}

impl ObjectKind {
    pub fn from_u8(kind: u8) -> Option<ObjectKind> {
        match kind {
            0 => Some(Self::Blob),
            1 => Some(Self::Manifest),
            2 => Some(Self::Layer),
            3 => Some(Self::Platform),
            4 => Some(Self::Tree),
            5 => Some(Self::Mask),
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

    fn digest(&self) -> crate::Result<encoding::Digest> {
        match self {
            Self::Platform(obj) => obj.digest(),
            Self::Layer(obj) => obj.digest(),
            Self::Manifest(obj) => obj.digest(),
            Self::Tree(obj) => obj.digest(),
            Self::Blob(obj) => Ok(obj.digest()),
            Self::Mask => Ok(encoding::EMPTY_DIGEST.into()),
        }
    }

    fn encode(&self, mut writer: &mut impl std::io::Write) -> crate::Result<()>
    where
        Self: Kind,
    {
        encoding::write_header(&mut writer, OBJECT_HEADER)?;
        const EPOCH: u8 = 0;
        encoding::write_uint8(&mut writer, EPOCH)?;
        writer
            .write_all(&[0, 0, 0, 0, 0, 0])
            .map_err(encoding::Error::FailedWrite)?; // reserved header space
        encoding::write_uint8(&mut writer, self.kind() as u8)?;
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
        let epoch_id = encoding::read_uint8(&mut reader)?;
        reader
            .read_exact(&mut [0, 0, 0, 0, 0, 0])
            .map_err(encoding::Error::FailedRead)?; // reserved header space
        let type_id = encoding::read_uint8(&mut reader)?;
        let Some(kind) = ObjectKind::from_u8(type_id) else {
            return Err(format!("Cannot read object: unknown object kind {type_id}").into());
        };
        match (epoch_id, kind) {
            (0, ObjectKind::Blob) => Ok(Self::Blob(Blob::decode(&mut reader)?)),
            (0, ObjectKind::Manifest) => Ok(Self::Manifest(Manifest::decode(&mut reader)?)),
            (0, ObjectKind::Layer) => Ok(Self::Layer(Layer::decode(&mut reader)?)),
            (0, ObjectKind::Platform) => Ok(Self::Platform(Platform::decode(&mut reader)?)),
            (0, ObjectKind::Tree) => Ok(Self::Tree(Tree::decode(&mut reader)?)),
            (_, ObjectKind::Mask) => Ok(Self::Mask),
            (e, _) => Err(Error::UnknownObjectEpoch(e)),
        }
    }
}
