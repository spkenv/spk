// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::fs;

use clap::Parser;
use miette::Result;
use spfs_cli_common as cli;

cli::main!(CmdDocs);

use spfs_cli_main::cmd_spfs::Opt;

/// Write Markdown documentation for all SPFS subcommands to docs folder.
#[derive(Debug, Parser)]
#[clap(name = "spfs-docs")]
pub struct CmdDocs {
    #[clap(flatten)]
    pub logging: cli::Logging,
}

impl CmdDocs {
    pub async fn run(&mut self, _config: &spfs::Config) -> Result<i32> {
        let mut markdown = clap_markdown::help_markdown::<Opt>();
        markdown.insert(0, '\n');
        markdown.insert_str(0, "---\n");
        markdown.insert_str(0, "chapter: true\n");
        markdown.insert_str(0, "title: SPFS CLI\n");
        markdown.insert_str(0, "---\n");
        // This path is relative to the current shell directory.
        fs::write("docs/spfs/cli/markdown.md", markdown).expect("Unable to write file");
        Ok(0)
    }
}
