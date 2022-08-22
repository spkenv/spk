// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use crate::Result;

/// Can be deprecated
#[enum_dispatch::enum_dispatch]
pub trait Deprecate {
    /// Report true if this instance has been deprecated
    fn is_deprecated(&self) -> bool;
}

#[enum_dispatch::enum_dispatch]
pub trait DeprecateMut: Deprecate {
    /// Mark this instance as deprecated
    fn deprecate(&mut self) -> Result<()>;

    /// Mark this instance as no longer deprecated
    fn undeprecate(&mut self) -> Result<()>;
}

impl<T> Deprecate for std::sync::Arc<T>
where
    T: Deprecate,
{
    fn is_deprecated(&self) -> bool {
        (**self).is_deprecated()
    }
}

impl<T> Deprecate for &T
where
    T: Deprecate,
{
    fn is_deprecated(&self) -> bool {
        (**self).is_deprecated()
    }
}

impl<T> Deprecate for &mut T
where
    T: Deprecate,
{
    fn is_deprecated(&self) -> bool {
        (**self).is_deprecated()
    }
}

impl<T> DeprecateMut for &mut T
where
    T: DeprecateMut,
{
    fn deprecate(&mut self) -> Result<()> {
        (**self).deprecate()
    }

    fn undeprecate(&mut self) -> Result<()> {
        (**self).undeprecate()
    }
}
