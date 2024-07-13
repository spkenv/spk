// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

mod build;
mod error;
pub mod report;
pub mod validation;

#[cfg(test)]
#[path = "./archive_test.rs"]
mod archive_test;

pub use build::{
    build_options_path,
    build_script_path,
    build_spec_path,
    commit_component_layers,
    component_marker_path,
    source_package_path,
    validate_source_changeset,
    BinaryPackageBuilder,
    BuildSource,
    SourcePackageBuilder,
};
pub use error::{Error, Result};
