// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
//! Main entry points and utilities for command line interface and interaction.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use colored::Colorize;

pub mod env;
pub mod flags;
// mod cmd_bake;
mod cmd_build;
// mod cmd_convert;
// mod cmd_deprecate;
mod cmd_env;
// mod cmd_explain;
// mod cmd_export;
// mod cmd_import;
// mod cmd_install;
// mod cmd_ls;
mod cmd_make_binary;
mod cmd_make_source;
// mod cmd_new;
// mod cmd_publish;
// mod cmd_remove;
// mod cmd_render;
// mod cmd_repo;
// mod cmd_search;
// mod cmd_test;
mod cmd_version;
mod cmd_view;

/// A Package Manager for SPFS
#[derive(Parser)]
#[clap(about)]
pub struct Opt {
    #[clap(short, long, global = true, parse(from_occurrences))]
    pub verbose: usize,
    #[clap(subcommand)]
    pub cmd: Command,
}

impl Opt {
    pub fn run(&self) -> Result<i32> {
        let _guard = spk::HANDLE.enter();
        let res = env::configure_logging(self.verbose).context("Failed to initialize output log");
        if let Err(err) = res {
            eprintln!("{}", err.to_string().red());
            return Ok(1);
        }
        self.cmd.run()
    }
}

#[derive(Subcommand)]
pub enum Command {
    // Bake(cmd_bake::Bake),
    Build(cmd_build::Build),
    // Test(cmd_test::Test),
    // Convert(cmd_convert::Convert),
    // Deprecate(cmd_deprecate::Deprecate),
    Env(cmd_env::Env),
    // Explain(cmd_explain::Explain),
    // Export(cmd_export::Export),
    // Import(cmd_import::Import),
    // Render(cmd_render::Render),
    // Install(cmd_install::Install),
    // Ls(cmd_ls::Ls),
    MakeBinary(cmd_make_binary::MakeBinary),
    MakeSource(cmd_make_source::MakeSource),
    // New(cmd_new::New),
    // Publish(cmd_publish::Publish),
    // Remove(cmd_remove::Remove),
    // Search(cmd_search::Search),
    Version(cmd_version::Version),
    View(cmd_view::View),
    // Repo(cmd_repo::Repo),
}

impl Command {
    fn run(&self) -> Result<i32> {
        match self {
            // Self::Bake(cmd) => cmd.run(),
            Self::Build(cmd) => cmd.run(),
            // Self::Test(cmd) => cmd.run(),
            // Self::Convert(cmd) => cmd.run(),
            // Self::Deprecate(cmd) => cmd.run(),
            Self::Env(cmd) => cmd.run(),
            // Self::Explain(cmd) => cmd.run(),
            // Self::Export(cmd) => cmd.run(),
            // Self::Import(cmd) => cmd.run(),
            // Self::Render(cmd) => cmd.run(),
            // Self::Install(cmd) => cmd.run(),
            // Self::Ls(cmd) => cmd.run(),
            Self::MakeBinary(cmd) => cmd.run(),
            Self::MakeSource(cmd) => cmd.run(),
            // Self::New(cmd) => cmd.run(),
            // Self::Publish(cmd) => cmd.run(),
            // Self::Remove(cmd) => cmd.run(),
            // Self::Search(cmd) => cmd.run(),
            Self::Version(cmd) => cmd.run(),
            Self::View(cmd) => cmd.run(),
            // Self::Repo(cmd) => cmd.run(),
        }
    }
}

fn main() {
    let opts = Opt::parse();
    let code = match opts.run() {
        Ok(code) => code,
        Err(err) => {
            tracing::error!("{:?}", err);
            1
        }
    };
    std::process::exit(code);
}
