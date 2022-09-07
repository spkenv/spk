// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use spfs_encoding as encoding;

use crate::Result;

/// A filesystem represents one method for establishing the
/// necessary spfs filesystem view for use by a runtime environment
#[async_trait::async_trait]
pub trait FileSystem {
    /// Setup a runtime filesystem for the current process
    async fn mount<I>(&self, stack: I, editable: bool) -> Result<()>
    where
        I: IntoIterator<Item = encoding::Digest> + Send;

    /// Modify the parameters of this process' filesystem
    async fn remount<I>(&self, stack: I, editable: bool) -> Result<()>
    where
        I: IntoIterator<Item = encoding::Digest> + Send;

    /// Clear any and all working changes in this process' filesystem.
    fn reset_all(&self) -> Result<()> {
        self.reset(&["*"])
    }

    /// Remove working changes from this process' filesystem.
    ///
    /// If no paths are specified, nothing is done.
    fn reset<S: AsRef<str>>(&self, paths: &[S]) -> Result<()>;

    /// Return true if there are working changes in this process' filesystem.
    fn is_dirty(&self) -> bool;
}
