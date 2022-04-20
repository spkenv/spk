// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use anyhow::{Context, Result};
use clap::Args;

use super::{flags, Run};

/// Build a source package from a spec file.
#[derive(Args)]
pub struct NumVariants {
    /// The package or yaml spec file to report on
    #[clap(name = "PKG|SPEC_FILE")]
    pub package: Option<String>,
}

impl Run for NumVariants {
    fn run(&mut self) -> Result<i32> {
        let (_, spec) = flags::find_package_spec(&self.package)
            .context("find package spec")?
            .must_be_found();

        println!(
            "{}",
            spec.build.variants.as_ref().map(|v| v.len()).unwrap_or(0)
        );

        Ok(0)
    }
}
