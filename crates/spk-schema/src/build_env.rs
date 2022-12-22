// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::borrow::Cow;
use std::collections::BTreeSet;

use spk_schema_foundation::ident_component::Component;
use spk_schema_foundation::name::PkgName;
use spk_schema_foundation::option_map::OptionMap;
use spk_schema_foundation::spec_ops::Named;
use spk_schema_ident::VersionIdent;

/// Describes a resolved build environment in which
/// a binary package may be created.
pub trait BuildEnv {
    type Package: super::Package;
    type BuildEnvMember: BuildEnvMember<Package = Self::Package>;
    type PackageIter<'a>: Iterator<Item = &'a Self::BuildEnvMember> + 'a
    where
        Self: 'a;

    /// The package that this build environment was resolved for
    fn target(&self) -> &VersionIdent;

    /// The full set of options for this build, including
    /// options for the package being build as well as any
    /// options from the resolution of the environment.
    fn options(&self) -> Cow<'_, OptionMap>;

    /// The set of members resolved for this environment,
    /// as requested by the package's recipe along with the
    /// set of components that are used.
    fn members(&self) -> Self::PackageIter<'_>;

    /// Find a member in this build environment by package name
    fn get_member(&self, name: &PkgName) -> Option<&Self::BuildEnvMember> {
        self.members().find(|m| m.package().name() == name)
    }
}

impl<'a, T> BuildEnv for &'a T
where
    T: BuildEnv,
{
    type Package = T::Package;
    type BuildEnvMember = T::BuildEnvMember;
    type PackageIter<'b> = T::PackageIter<'b> where Self: 'b;

    fn target(&self) -> &VersionIdent {
        (**self).target()
    }

    fn options(&self) -> Cow<'_, OptionMap> {
        (**self).options()
    }

    fn members(&self) -> Self::PackageIter<'_> {
        (**self).members()
    }
}

impl<T> BuildEnv for (VersionIdent, OptionMap, Vec<T>)
where
    T: BuildEnvMember + 'static,
{
    type PackageIter<'a> = std::slice::Iter<'a, T>;
    type BuildEnvMember = T;
    type Package = T::Package;

    fn target(&self) -> &VersionIdent {
        &self.0
    }

    fn options(&self) -> Cow<'_, OptionMap> {
        Cow::Borrowed(&self.1)
    }

    fn members(&self) -> Self::PackageIter<'_> {
        self.2.iter()
    }
}

/// A package paired with the set of components being used
pub trait BuildEnvMember {
    type Package: super::Package;

    fn package(&self) -> &Self::Package;
    fn used_components(&self) -> &BTreeSet<Component>;
}

impl<P> BuildEnvMember for (P, BTreeSet<Component>)
where
    P: super::Package,
{
    type Package = P;

    fn package(&self) -> &Self::Package {
        &self.0
    }

    fn used_components(&self) -> &BTreeSet<Component> {
        &self.1
    }
}

impl<'a, T> BuildEnvMember for &'a T
where
    T: BuildEnvMember + 'a,
{
    type Package = T::Package;

    fn package(&self) -> &Self::Package {
        (**self).package()
    }

    fn used_components(&self) -> &BTreeSet<Component> {
        (**self).used_components()
    }
}
