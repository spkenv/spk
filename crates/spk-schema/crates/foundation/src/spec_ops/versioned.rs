// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::sync::Arc;

use crate::version::{Compat, Version};

/// Some item that has an associated version
#[enum_dispatch::enum_dispatch]
pub trait Versioned {
    /// The associated version number
    fn version(&self) -> &Version;

    /// The compatibility guaranteed by this items versioning scheme
    fn compat(&self) -> &Compat;
}

impl<T: Versioned> Versioned for Arc<T> {
    fn version(&self) -> &Version {
        (**self).version()
    }

    fn compat(&self) -> &Compat {
        (**self).compat()
    }
}

impl<T: Versioned> Versioned for &T {
    fn version(&self) -> &Version {
        (**self).version()
    }

    fn compat(&self) -> &Compat {
        (**self).compat()
    }
}
