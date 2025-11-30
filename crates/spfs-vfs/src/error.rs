// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use thiserror::Error;

/// Errors specific to fuse operations.
#[derive(Debug, Error)]
pub enum Error {
    /// An operation was attempted on a mask entry.
    #[error("Entry is a mask")]
    EntryIsMask,

    /// A generic string error.
    #[error("{0}")]
    String(String),
}
