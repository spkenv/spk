// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
//! Main entry points and utilities for command line interface and interaction.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use colored::Colorize;
use enum_dispatch::enum_dispatch;

mod cmd_bake;
mod cmd_build;
mod cmd_convert;
mod cmd_deprecate;
mod cmd_env;
mod cmd_explain;
mod cmd_export;
mod cmd_import;
mod cmd_install;
mod cmd_ls;
mod cmd_make_binary;
mod cmd_make_source;
mod cmd_new;
mod cmd_num_variants;
mod cmd_publish;
mod cmd_remove;
mod cmd_render;
mod cmd_repo;
mod cmd_search;
mod cmd_test;
mod cmd_undeprecate;
mod cmd_version;
mod cmd_view;
pub mod env;
pub mod flags;

/// A Package Manager for SPFS
#[derive(Parser)]
#[clap(about)]
pub struct Opt {
    #[clap(short, long, global = true, parse(from_occurrences))]
    pub verbose: u32,
    #[clap(subcommand)]
    pub cmd: Command,
}

impl Opt {
    pub async fn run(&mut self) -> Result<i32> {
        let _guard = spk::HANDLE.enter();

        #[cfg(feature = "sentry")]
        let _sentry_guard = env::configure_sentry();

        let res = env::configure_logging(self.verbose).context("Failed to initialize output log");
        if let Err(err) = res {
            eprintln!("{}", err.to_string().red());
            return Ok(1);
        }
        self.cmd.run().await
    }
}

/// Trait all cli commands must implement to be runnable.
#[async_trait::async_trait]
#[enum_dispatch]
trait Run {
    async fn run(&mut self) -> Result<i32>;
}

#[enum_dispatch(Run)]
#[derive(Subcommand)]
pub enum Command {
    Bake(cmd_bake::Bake),
    Build(cmd_build::Build),
    Convert(cmd_convert::Convert),
    Deprecate(cmd_deprecate::Deprecate),
    Env(cmd_env::Env),
    Explain(cmd_explain::Explain),
    Export(cmd_export::Export),
    Import(cmd_import::Import),
    Install(cmd_install::Install),
    Ls(cmd_ls::Ls),
    MakeBinary(cmd_make_binary::MakeBinary),
    MakeSource(cmd_make_source::MakeSource),
    New(cmd_new::New),
    #[clap(hide = true)]
    NumVariants(cmd_num_variants::NumVariants),
    Publish(cmd_publish::Publish),
    Remove(cmd_remove::Remove),
    Render(cmd_render::Render),
    Repo(cmd_repo::Repo),
    Search(cmd_search::Search),
    Test(cmd_test::Test),
    Undeprecate(cmd_undeprecate::Undeprecate),
    Version(cmd_version::Version),
    View(cmd_view::View),
}

#[tokio::main]
async fn main() {
    let mut opts = Opt::parse();
    let code = match opts.run().await {
        Ok(code) => code,
        Err(err) => {
            let root = err.root_cause();
            if let Some(err) = root.downcast_ref() {
                eprintln!("{}", spk::io::format_error(err, opts.verbose));
            } else {
                tracing::error!("{:?}", err);
            }
            1
        }
    };
    std::process::exit(code);
}
