// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk
use std::fs;

use clap::Args;
use miette::Result;

/// Write Markdown documentation for all SPFS subcommands to docs folder.
#[derive(Debug, Args)]
pub struct CmdDocs {}

impl CmdDocs {
    pub async fn run(&mut self, _config: &spfs::Config) -> Result<i32> {
        let mut markdown = clap_markdown::help_markdown::<crate::Opt>();
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
