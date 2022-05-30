// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use anyhow::{Context, Result};
use clap::Args;
use spk::api::Package;

use super::{flags, CommandArgs, Run};

/// Build a source package from a spec file.
#[derive(Args)]
pub struct NumVariants {
    /// The package or yaml spec file to report on
    #[clap(name = "PKG|SPEC_FILE")]
    pub package: Option<String>,
}

#[async_trait::async_trait]
impl Run for NumVariants {
    async fn run(&mut self) -> Result<i32> {
        let (_, spec) = flags::find_package_spec(&self.package)
            .context("find package spec")?
            .must_be_found();

        println!("{}", spec.variants().len());

        Ok(0)
    }
}

impl CommandArgs for NumVariants {
    fn get_positional_args(&self) -> Vec<String> {
        // The important positional arg for a num-variants is the package
        match &self.package {
            Some(pkg) => vec![pkg.clone()],
            None => vec![],
        }
    }
}
