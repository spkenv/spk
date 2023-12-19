// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

//! Low-level digraph representation and manipulation for data storage.

mod blob;
mod database;
mod entry;
pub mod error;
mod kind;
mod layer;
mod manifest;
pub mod object;
mod platform;
pub mod stack;
mod tree;

use std::cell::RefCell;

pub use blob::Blob;
pub use database::{
    Database,
    DatabaseIterator,
    DatabaseView,
    DatabaseWalker,
    DigestSearchCriteria,
};
pub use entry::Entry;
pub use kind::{HasKind, Kind, ObjectKind};
pub use layer::Layer;
pub use manifest::Manifest;
pub use object::{FlatObject, Object, ObjectProto};
pub use platform::Platform;
pub use stack::Stack;
pub use tree::Tree;

thread_local! {
    /// A shared, thread-local builder to avoid extraneous allocations
    /// when creating new instances of objects via [`flatbuffers`].
    static BUILDER: RefCell<flatbuffers::FlatBufferBuilder<'static>> = RefCell::new(flatbuffers::FlatBufferBuilder::with_capacity(256));
}
