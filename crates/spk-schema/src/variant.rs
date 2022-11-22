// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::borrow::Cow;

use spk_schema_foundation::option_map::OptionMap;

/// Describes a resolved build environment in which
/// a binary package may be created.
pub trait Variant {
    fn name(&self) -> Option<&str> {
        None
    }

    fn options(&self) -> Cow<'_, OptionMap>;
}

impl Variant for OptionMap {
    fn options(&self) -> Cow<'_, OptionMap> {
        Cow::Borrowed(self)
    }
}

impl<'a, T> Variant for &'a T
where
    T: Variant,
{
    fn name(&self) -> Option<&str> {
        (**self).name()
    }

    fn options(&self) -> Cow<'_, OptionMap> {
        (**self).options()
    }
}
