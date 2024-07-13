// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use clap::Args;
use colored::Colorize;
use miette::{IntoDiagnostic, Result};
use spk_cli_common::{CommandArgs, Run};
use spk_schema::foundation::name::PkgNameBuf;

#[cfg(test)]
#[path = "./cmd_new_test.rs"]
mod cmd_new_test;

/// Initialize a new package
#[derive(Args)]
#[clap(visible_alias = "init")]
pub struct New {
    /// The name of the new package to generate
    name: PkgNameBuf,
}

#[async_trait::async_trait]
impl Run for New {
    type Output = i32;

    async fn run(&mut self) -> Result<Self::Output> {
        let spec = get_stub(&self.name);

        let spec_file = format!("{}.spk.yaml", self.name);
        std::fs::write(&spec_file, spec).into_diagnostic()?;
        println!("{}: {}", "Created".green(), spec_file);
        Ok(0)
    }
}

impl CommandArgs for New {
    fn get_positional_args(&self) -> Vec<String> {
        // The important positional arg for a make-source is the name
        vec![self.name.to_string()]
    }
}

fn get_stub(name: &PkgNameBuf) -> String {
    format!(
        r#"api: v0/package
pkg: {name}/0.1.0

build:

  # set which host related vars are added automatically to the built package:
  # - Distro: adds 'distro', 'arch', 'os' and '<distroname>' vars, so the package
  #           can only be used on the same OS, CPU, and OS distribution version
  #           (e.g. linux distro). This is the default.
  # - Arch: adds 'arch' and 'os' vars, so the package can be used anywhere that
  #         has the same OS and CPU architecture (x86_64, i386)
  # - Os: adds 'os' var, so the package can be used anywhere that has the same
  #       OS type (mac, linux, windows)
  # - None: adds no host vars, so package can be used on any OS and any architecture
  auto_host_vars: Distro

  # options are all the inputs to the package build process, including
  # build-time dependencies
  options:
    # var options define environment/string values that affect the build.
    # The value is defined in the build environment as SPK_OPT_{{name}}
    # - var: somename/somevalue

    # pkg options request packages that need to be present
    # in the build environment. You can specify a version number
    # here as the default for when the option is not otherwise specified
    - pkg: python/3

  # variants declares the default set of variants to build and publish
  # using the spk build and make-* commands
  variants:
    - {{python: 2.7}}
    - {{python: 3.7}}
    # you can also force option values for specific dependencies with a prefix
    # - {{python: 2.7, vnp3.debug: on}}

  # the build script is arbitrary bash script to be executed for the
  # build. It should be and install artifacts into /spfs
  script:
    # if you remove this it will try to run a build.sh script instead
    - echo "don't forget to add build logic!"
    - exit 1

install:
  requirements:
    - pkg: python
      # we can use the version of python from the build environment to dynamically
      # define the install requirement
      fromBuildEnv: x.x
"#
    )
}
