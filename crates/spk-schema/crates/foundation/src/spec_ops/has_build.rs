// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::sync::Arc;

use crate::ident_build::Build;

/// Some item that has an associated build id
pub trait HasBuild {
    /// The associated build id
    fn build(&self) -> &Build;
}

impl<T: HasBuild> HasBuild for Arc<T> {
    fn build(&self) -> &Build {
        (**self).build()
    }
}

impl<T: HasBuild> HasBuild for &T {
    fn build(&self) -> &Build {
        (**self).build()
    }
}
