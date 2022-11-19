// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::borrow::Cow;
use std::collections::HashMap;

use spk_schema_foundation::option_map::OptionMap;

/// Describes a resolved build environment in which
/// a binary package may be created.
pub trait BuildEnv {
    type PackageIter: Iterator<Item = Self::Package>;
    type Package: super::Package;

    /// The full set of options for this build, including
    /// options for the package being build as well as any
    /// options from the resolution of the environment.
    fn options(&self) -> Cow<'_, OptionMap>;

    /// The set of packages resolved for this environment,
    /// as requested by the package's recipe
    fn packages(&self) -> Self::PackageIter;

    /// The environment variables that should be set for this build
    /// environment. Defaults to [`Self::options`] if not overridden.
    fn env_vars(&self) -> HashMap<String, String> {
        self.options().to_environment()
    }
}

impl<'a, T> BuildEnv for &'a T
where
    T: BuildEnv,
{
    type PackageIter = T::PackageIter;
    type Package = T::Package;

    fn options(&self) -> Cow<'_, OptionMap> {
        (**self).options()
    }

    fn packages(&self) -> Self::PackageIter {
        (**self).packages()
    }

    fn env_vars(&self) -> HashMap<String, String> {
        (**self).env_vars()
    }
}

impl<'a, P> BuildEnv for &'a (OptionMap, Vec<P>)
where
    P: super::Package,
{
    type PackageIter = std::slice::Iter<'a, P>;
    type Package = &'a P;

    fn options(&self) -> Cow<'_, OptionMap> {
        Cow::Borrowed(&self.0)
    }

    fn packages(&self) -> Self::PackageIter {
        self.1.iter()
    }

    fn env_vars(&self) -> HashMap<String, String> {
        self.0.to_environment()
    }
}
