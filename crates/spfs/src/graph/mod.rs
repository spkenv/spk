// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

///! Low-level digraph representation and manipulation for data storage.
mod blob;
mod database;
mod entry;
mod error;
mod layer;
mod manifest;
mod object;
mod operations;
mod platform;
mod tree;

pub use blob::Blob;
pub use database::{Database, DatabaseIterator, DatabaseView, DatabaseWalker};
pub use entry::Entry;
pub use error::{
    AmbiguousReferenceError, InvalidReferenceError, Result, UnknownObjectError,
    UnknownReferenceError,
};
pub use layer::Layer;
pub use manifest::Manifest;
pub use object::{Object, ObjectKind};
pub use operations::check_database_integrity;
pub use platform::Platform;
pub use tree::Tree;
