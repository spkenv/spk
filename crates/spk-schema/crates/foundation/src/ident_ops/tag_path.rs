// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use relative_path::RelativePathBuf;

pub trait TagPath {
    /// Return the relative path for the spfs tag for an ident.
    ///
    /// The version part of the path is always normalized.
    fn tag_path(&self) -> RelativePathBuf;

    /// Return the relative path for the spfs tag for an ident.
    ///
    /// The version part is not normalized. This should no be used to write any
    /// content into a repository, where normalization is required.
    fn verbatim_tag_path(&self) -> RelativePathBuf;
}
