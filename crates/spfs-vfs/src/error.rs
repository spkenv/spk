// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use thiserror::Error;

/// Errors specific to fuse operations.
#[derive(Debug, Error)]
pub enum Error {
    /// An operation was attempted on a mask entry.
    #[error("Entry is a mask")]
    EntryIsMask,
}
