// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use relative_path::RelativePathBuf;

pub trait TagPath {
    /// Return the relative path for the spfs tag for an ident.
    fn tag_path(&self) -> RelativePathBuf;
}
