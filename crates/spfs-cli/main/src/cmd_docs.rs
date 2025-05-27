// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use clap::Args;
use miette::{IntoDiagnostic, Result};

/// Output the current configuration of spfs
#[derive(Debug, Args)]
pub struct CmdDocs {}

impl CmdDocs {
    pub async fn run(&mut self, config: &spfs::Config) -> Result<i32> {
        clap_markdown::print_help_markdown::<crate::Opt>();
        Ok(0)
    }
}
