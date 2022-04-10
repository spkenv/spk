// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use anyhow::Result;
use clap::Args;
use colored::Colorize;

#[cfg(test)]
#[path = "./cmd_new_test.rs"]
mod cmd_new_test;

/// Initialize a new package
#[derive(Args)]
#[clap(visible_alias = "init")]
pub struct New {
    /// The name of the new package to generate
    name: String,
}

impl New {
    pub fn run(&mut self) -> Result<i32> {
        spk::api::validate_name(&self.name)?;
        let spec = get_stub(&self.name);

        let spec_file = format!("{}.spk.yaml", self.name);
        std::fs::write(&spec_file, &spec)?;
        println!("{}: {}", "Created".green(), spec_file);
        Ok(0)
    }
}

fn get_stub(name: &str) -> String {
    format!(
        r#"pkg: {name}/0.1.0

build:

  # options are all the inputs to the package build process, including
  # build-time dependencies
  options:
    # var options define environment/string values that affect the build.
    # The value is defined in the build environment as SPK_OPT_{{name}}
    - var: arch    # rebuild if the arch changes
    - var: os      # rebuild if the os changes
    - var: centos  # rebuild if centos version changes

    # pkg options request packages that need to be present
    # in the build environment. You can specify a version number
    # here as the default for when the option is not otherise specified
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
