// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::sync::Arc;

use crate::version::{Compat, Compatibility, Version};

/// Some item that has an associated version
pub trait HasVersion {
    /// The associated version number
    fn version(&self) -> &Version;
}

/// An item which represents one version of itself in a
/// series of possible versions.
#[enum_dispatch::enum_dispatch]
pub trait Versioned: HasVersion {
    /// The compatibility guaranteed by this items versioning scheme
    fn compat(&self) -> &Compat;

    /// Check if this item's version is api-compatible with the provided one
    fn is_api_compatible(&self, base: &Version) -> Compatibility {
        self.compat()
            .is_api_compatible(base, HasVersion::version(self))
    }

    /// Check if this item's version is binary-compatible with the provided one
    fn is_binary_compatible(&self, base: &Version) -> Compatibility {
        self.compat()
            .is_binary_compatible(base, HasVersion::version(self))
    }
}

impl<T: HasVersion> HasVersion for Arc<T> {
    fn version(&self) -> &Version {
        (**self).version()
    }
}

impl<T: Versioned> Versioned for Arc<T> {
    fn compat(&self) -> &Compat {
        (**self).compat()
    }

    fn is_api_compatible(&self, base: &Version) -> Compatibility {
        (**self).is_api_compatible(base)
    }

    fn is_binary_compatible(&self, base: &Version) -> Compatibility {
        (**self).is_binary_compatible(base)
    }
}

impl<T: HasVersion> HasVersion for Box<T> {
    fn version(&self) -> &Version {
        (**self).version()
    }
}

impl<T: Versioned> Versioned for Box<T> {
    fn compat(&self) -> &Compat {
        (**self).compat()
    }

    fn is_api_compatible(&self, base: &Version) -> Compatibility {
        (**self).is_api_compatible(base)
    }

    fn is_binary_compatible(&self, base: &Version) -> Compatibility {
        (**self).is_binary_compatible(base)
    }
}

impl<T: HasVersion> HasVersion for &T {
    fn version(&self) -> &Version {
        (**self).version()
    }
}

impl<T: Versioned> Versioned for &T {
    fn compat(&self) -> &Compat {
        (**self).compat()
    }

    fn is_api_compatible(&self, base: &Version) -> Compatibility {
        (**self).is_api_compatible(base)
    }

    fn is_binary_compatible(&self, base: &Version) -> Compatibility {
        (**self).is_binary_compatible(base)
    }
}
