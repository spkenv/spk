// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use serde::{Deserialize, Serialize};
use spk_schema_foundation::IsDefault;
use spk_schema_foundation::ident::AsVersionIdent;
use spk_schema_foundation::spec_ops::Named;

use crate::component_embedded_packages::ComponentEmbeddedPackage;
use crate::v0;

#[cfg(test)]
#[path = "./embedded_packages_list_test.rs"]
mod embedded_packages_list_test;

/// A set of packages that are embedded/provided by another.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct EmbeddedPackagesList<EmbeddedSpec>(Vec<EmbeddedSpec>);

impl<EmbeddedSpec> EmbeddedPackagesList<EmbeddedSpec>
where
    EmbeddedSpec: AsVersionIdent + Named,
{
    /// Return an iterator over the embedded packages that match the given
    /// embedded component.
    pub fn packages_matching_embedded_package<'a>(
        &'a self,
        embedded_package: &ComponentEmbeddedPackage,
    ) -> impl Iterator<Item = &'a EmbeddedSpec> {
        self.iter().filter(move |embedded| {
            embedded.name() == embedded_package.pkg.name()
                && (embedded_package.pkg.target().is_none()
                    || embedded.as_version_ident().version()
                        == embedded_package.pkg.target().as_ref().unwrap())
        })
    }
}

impl<T> Default for EmbeddedPackagesList<T> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<T> IsDefault for EmbeddedPackagesList<T> {
    fn is_default(&self) -> bool {
        self.is_empty()
    }
}

impl<EmbeddedSpec> std::ops::Deref for EmbeddedPackagesList<EmbeddedSpec> {
    type Target = Vec<EmbeddedSpec>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<EmbeddedSpec> std::ops::DerefMut for EmbeddedPackagesList<EmbeddedSpec> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<EmbeddedPackagesList<v0::EmbeddedRecipeSpec>>
    for EmbeddedPackagesList<v0::EmbeddedPackageSpec>
{
    fn from(value: EmbeddedPackagesList<v0::EmbeddedRecipeSpec>) -> Self {
        EmbeddedPackagesList(value.0.into_iter().map(Into::into).collect())
    }
}

impl From<EmbeddedPackagesList<v0::EmbeddedPackageSpec>>
    for EmbeddedPackagesList<v0::EmbeddedRecipeSpec>
{
    fn from(value: EmbeddedPackagesList<v0::EmbeddedPackageSpec>) -> Self {
        EmbeddedPackagesList(value.0.into_iter().map(Into::into).collect())
    }
}
