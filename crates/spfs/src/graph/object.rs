use super::Blob;
use super::Layer;
use super::Manifest;
use super::Tree;

pub enum Object {
    Manifest(Manifest),
    Layer(Layer),
    Tree(Tree),
    Blob(Blob),
    Mask,
}
