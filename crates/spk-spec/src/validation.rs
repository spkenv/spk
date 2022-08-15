// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashSet;
use std::iter::FromIterator;

use serde::{Deserialize, Serialize};
use spk_build_ops::reset_permissions;
use spk_ident_ops::MetadataPath;
use spk_spec_ops::{ComponentOps, PackageOps};
use spk_validators::{
    must_collect_all_files, must_install_something, must_not_alter_existing_files,
};

use crate::Result;

#[cfg(test)]
#[path = "./validation_test.rs"]
mod validation_test;

/// A Validator validates packages after they have been built
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Validator {
    MustInstallSomething,
    MustNotAlterExistingFiles,
    MustCollectAllFiles,
}

impl Validator {
    /// Validate the set of changes to spfs according to this validator
    pub fn validate<Package, P>(
        &self,
        spec: &Package,
        diffs: &[spfs::tracking::Diff],
        prefix: P,
    ) -> Option<String>
    where
        Package: PackageOps,
        Package::Component: ComponentOps,
        <Package as PackageOps>::Ident: MetadataPath,
        P: AsRef<std::path::Path>,
    {
        match self {
            Self::MustInstallSomething => must_install_something(diffs, prefix),
            Self::MustNotAlterExistingFiles => must_not_alter_existing_files(diffs, prefix),
            Self::MustCollectAllFiles => must_collect_all_files(spec, diffs, prefix),
        }
    }
}

/// The set of validators that are enabled by default
pub fn default_validators() -> Vec<Validator> {
    vec![
        Validator::MustInstallSomething,
        Validator::MustNotAlterExistingFiles,
        Validator::MustCollectAllFiles,
    ]
}

/// ValidationSpec configures how builds of this package
/// should be validated. The default spec contains all
/// recommended validators
#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ValidationSpec {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub disabled: Vec<Validator>,
}

impl ValidationSpec {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    /// Return the set of active validators based on this spec
    pub fn configured_validators(&self) -> HashSet<Validator> {
        HashSet::from_iter(
            default_validators()
                .into_iter()
                .filter(|validator| !self.disabled.contains(validator)),
        )
    }

    /// Validate the current set of spfs changes as a build of this package
    pub async fn validate_build_changeset<Package>(&self, package: &Package) -> Result<()>
    where
        Package: PackageOps,
        Package::Component: ComponentOps,
        <Package as PackageOps>::Ident: MetadataPath,
    {
        static SPFS: &str = "/spfs";

        let mut diffs = spfs::diff(None, None).await?;

        // FIXME: this is only required because of a bug in spfs reset which
        // fails to handle permission-only changes on files...
        reset_permissions(&mut diffs, SPFS)?;

        for validator in self.configured_validators().iter() {
            if let Some(err) = validator.validate(package, &diffs, SPFS) {
                return Err(spk_ident_build::InvalidBuildError::new_error(format!(
                    "{:?}: {}",
                    validator, err
                ))
                .into());
            }
        }

        Ok(())
    }
}
