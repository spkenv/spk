// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use crate::{encoding, Error};

/// Denotes a missing object or one that is not present in the database.
#[derive(Debug, Eq, PartialEq)]
pub struct UnknownObjectError {
    pub message: String,
}

impl UnknownObjectError {
    pub fn new(digest: &encoding::Digest) -> Error {
        Self {
            message: format!("Unknown object: {}", digest.to_string()),
        }
        .into()
    }
}

/// Denotes a reference that is not known.
#[derive(Debug, Eq, PartialEq)]
pub struct UnknownReferenceError {
    pub message: String,
}

impl UnknownReferenceError {
    pub fn new(reference: impl AsRef<str>) -> Error {
        Self {
            message: format!("Unknown reference: {}", reference.as_ref()),
        }
        .into()
    }
}

/// Denotes a reference that could refer to more than one object in the storage.
#[derive(Debug, Eq, PartialEq)]
pub struct AmbiguousReferenceError {
    pub message: String,
}

impl AmbiguousReferenceError {
    pub fn new(reference: impl AsRef<str>) -> Error {
        Self {
            message: format!("Ambiguous reference [too short]: {}", reference.as_ref()),
        }
        .into()
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// Denotes a missing object or one that is not present in the database.
#[derive(Debug, Eq, PartialEq)]
pub struct InvalidReferenceError {
    pub message: String,
}

impl InvalidReferenceError {
    pub fn new(reference: impl AsRef<str>) -> Self {
        Self {
            message: format!("Invalid reference: {}", reference.as_ref()),
        }
    }
}
