// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

mod alter_existing_files;
mod collect_all_files;
mod collect_existing_files;
mod empty_package;
mod error;
mod inherit_requirements;
mod long_var_description;
mod recursive_build;
mod strong_inheritance_var_desc;
mod validator;

pub use alter_existing_files::AlterExistingFilesValidator;
pub use collect_all_files::CollectAllFilesValidator;
pub use collect_existing_files::CollectExistingFilesValidator;
pub use empty_package::EmptyPackageValidator;
pub use error::{Error, Result};
pub use inherit_requirements::InheritRequirementsValidator;
pub use long_var_description::LongVarDescriptionValidator;
pub use recursive_build::RecursiveBuildValidator;
pub use strong_inheritance_var_desc::StrongInheritanceVarDescriptionValidator;
pub use validator::{Outcome, Report, Status, Subject, Validator};
