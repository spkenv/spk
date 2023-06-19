// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use spk_schema_foundation::option_map::OptionMap;

use crate::variant::Override;
use crate::Variant;

/// A trait that implements [`Variant`] but also provides a reference to the
/// variant that should be used when calculating the build digest.
pub trait VariantForBuildDigest: Variant {
    type Output: Variant;

    fn variant_for_build_digest(&self) -> &Self::Output;
}

impl VariantForBuildDigest for OptionMap {
    type Output = OptionMap;

    #[inline]
    fn variant_for_build_digest(&self) -> &Self::Output {
        self
    }
}

impl<T> VariantForBuildDigest for Override<T>
where
    T: Variant,
{
    type Output = Self;

    #[inline]
    fn variant_for_build_digest(&self) -> &Self::Output {
        self
    }
}
