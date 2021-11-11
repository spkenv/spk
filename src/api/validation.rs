// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::collections::HashSet;
use std::iter::FromIterator;

use serde::{Deserialize, Serialize};

use crate::Result;

#[cfg(test)]
#[path = "./validation_test.rs"]
mod validation_test;

/// A Validator validates packages after they have been built
#[derive(Debug, Hash, PartialEq, Eq, Serialize, Deserialize, Clone, Copy)]
pub enum Validator {
    MustInstallSomething,
    MustNotAlterExistingFiles,
}

impl Validator {
    /// Validate the set of changes to spfs according to this validator
    pub fn validate<P: AsRef<std::path::Path>>(
        &self,
        diffs: &[spfs::tracking::Diff],
        prefix: P,
    ) -> Option<String> {
        match self {
            Validator::MustInstallSomething => {
                super::validators::must_install_something(diffs, prefix)
            }
            Validator::MustNotAlterExistingFiles => {
                super::validators::must_not_alter_existing_files(diffs, prefix)
            }
        }
    }
}

/// The set of validators that are enabled by default
pub fn default_validators() -> Vec<Validator> {
    vec![
        Validator::MustInstallSomething,
        Validator::MustNotAlterExistingFiles,
    ]
}

/// ValidationSpec configures how builds of this package
/// should be validated. The default spec contains all
/// recommended validators
#[derive(Debug, PartialEq, Eq, Hash, Default, Deserialize, Serialize, Clone)]
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
    pub fn validate_build_changeset(&self) -> Result<()> {
        static SPFS: &str = "/spfs";

        let mut diffs = spfs::diff(None, None)?;

        // FIXME: this is only required because of a bug in spfs reset which
        // fails to handle permission-only changes on files...
        crate::build::reset_permissions(&mut diffs, SPFS)?;

        for validator in self.configured_validators().iter() {
            if let Some(err) = validator.validate(&diffs, SPFS) {
                return Err(super::InvalidBuildError::new_error(format!(
                    "{:?}: {}",
                    validator, err
                )));
            }
        }

        Ok(())
    }
}
