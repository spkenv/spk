// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use spk_version::{CompatRule, Compatibility, Version};

use super::{Named, Versioned};

pub trait RecipeOps: Named + Versioned {
    type Ident;
    type PkgRequest;
    type RangeIdent;

    fn is_api_compatible(&self, base: &Version) -> Compatibility;
    fn is_binary_compatible(&self, base: &Version) -> Compatibility;
    fn is_satisfied_by_range_ident(
        &self,
        range_ident: &Self::RangeIdent,
        required: CompatRule,
    ) -> Compatibility;
    fn is_satisfied_by_pkg_request(&self, pkg_request: &Self::PkgRequest) -> Compatibility;

    /// Build an identifier to represent this recipe.
    ///
    /// The returned identifier will not have an associated build.
    fn to_ident(&self) -> Self::Ident;
}

impl<T> RecipeOps for std::sync::Arc<T>
where
    T: RecipeOps,
{
    type Ident = T::Ident;
    type PkgRequest = T::PkgRequest;
    type RangeIdent = T::RangeIdent;

    fn is_api_compatible(&self, base: &Version) -> Compatibility {
        (**self).is_api_compatible(base)
    }

    fn is_binary_compatible(&self, base: &Version) -> Compatibility {
        (**self).is_binary_compatible(base)
    }

    fn is_satisfied_by_range_ident(
        &self,
        range_ident: &Self::RangeIdent,
        required: CompatRule,
    ) -> Compatibility {
        (**self).is_satisfied_by_range_ident(range_ident, required)
    }

    fn is_satisfied_by_pkg_request(&self, pkg_request: &Self::PkgRequest) -> Compatibility {
        (**self).is_satisfied_by_pkg_request(pkg_request)
    }

    fn to_ident(&self) -> Self::Ident {
        (**self).to_ident()
    }
}

impl<T> RecipeOps for &T
where
    T: RecipeOps,
{
    type Ident = T::Ident;
    type PkgRequest = T::PkgRequest;
    type RangeIdent = T::RangeIdent;

    fn is_api_compatible(&self, base: &Version) -> Compatibility {
        (**self).is_api_compatible(base)
    }

    fn is_binary_compatible(&self, base: &Version) -> Compatibility {
        (**self).is_binary_compatible(base)
    }

    fn is_satisfied_by_range_ident(
        &self,
        range_ident: &Self::RangeIdent,
        required: CompatRule,
    ) -> Compatibility {
        (**self).is_satisfied_by_range_ident(range_ident, required)
    }

    fn is_satisfied_by_pkg_request(&self, pkg_request: &Self::PkgRequest) -> Compatibility {
        (**self).is_satisfied_by_pkg_request(pkg_request)
    }

    fn to_ident(&self) -> Self::Ident {
        (**self).to_ident()
    }
}
