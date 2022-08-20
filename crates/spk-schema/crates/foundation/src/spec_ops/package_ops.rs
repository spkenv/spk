// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use crate::version::Compatibility;

use super::{Named, Versioned};

pub trait PackageOps: Named + Versioned {
    type Ident;
    type Component;
    type VarRequest;

    fn components_iter(&self) -> std::slice::Iter<'_, Self::Component>;

    /// The full identifier for this package
    ///
    /// This includes the version and optional build
    fn ident(&self) -> &Self::Ident;

    fn is_satisfied_by_var_request(&self, var_request: &Self::VarRequest) -> Compatibility;
}

pub trait PackageMutOps {
    type Ident;

    fn ident_mut(&mut self) -> &mut Self::Ident;
}

impl<T> PackageOps for std::sync::Arc<T>
where
    T: PackageOps,
{
    type Ident = T::Ident;
    type Component = T::Component;
    type VarRequest = T::VarRequest;

    fn components_iter(&self) -> std::slice::Iter<'_, Self::Component> {
        (**self).components_iter()
    }

    fn ident(&self) -> &T::Ident {
        (**self).ident()
    }

    fn is_satisfied_by_var_request(&self, var_request: &Self::VarRequest) -> Compatibility {
        (**self).is_satisfied_by_var_request(var_request)
    }
}

impl<T> PackageOps for &T
where
    T: PackageOps,
{
    type Ident = T::Ident;
    type Component = T::Component;
    type VarRequest = T::VarRequest;

    fn components_iter(&self) -> std::slice::Iter<'_, Self::Component> {
        (**self).components_iter()
    }

    fn ident(&self) -> &T::Ident {
        (**self).ident()
    }

    fn is_satisfied_by_var_request(&self, var_request: &Self::VarRequest) -> Compatibility {
        (**self).is_satisfied_by_var_request(var_request)
    }
}
