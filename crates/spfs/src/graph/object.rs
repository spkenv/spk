use super::Blob;
use super::Layer;
use super::Manifest;
use super::Platform;
use super::Tree;
use crate::encoding;
use crate::encoding::Encodable;

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
            Self::Manifest(manifest) => vec![manifest.root().digest().unwrap()],
            Self::Tree(tree) => tree.entries.iter().map(|e| e.object.clone()).collect(),
            Self::Blob(_blob) => Vec::new(),
            Self::Mask => Vec::new(),
        }
    }
}
