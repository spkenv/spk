// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use spk_schema_foundation::option_map::OptionMap;

use crate::Variant;
use crate::variant::Override;

/// A trait that implements [`Variant`] but also provides a reference to the
/// variant that should be used when calculating the build digest.
pub trait InputVariant: Variant {
    type Output: Variant;

    fn input_variant(&self) -> &Self::Output;
}

impl InputVariant for OptionMap {
    type Output = OptionMap;

    #[inline]
    fn input_variant(&self) -> &Self::Output {
        self
    }
}

impl<T> InputVariant for Override<T>
where
    T: Variant,
{
    type Output = Self;

    #[inline]
    fn input_variant(&self) -> &Self::Output {
        self
    }
}
