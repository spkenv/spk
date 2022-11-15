// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
//! Main entry points and utilities for command line interface and interaction.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use colored::Colorize;
#[cfg(feature = "sentry")]
use spk_cli_common::configure_sentry;
use spk_cli_common::{configure_logging, CommandArgs, Error, Run};
use spk_cli_group1::{cmd_bake, cmd_deprecate, cmd_undeprecate};
use spk_cli_group2::{cmd_ls, cmd_new, cmd_num_variants, cmd_publish, cmd_remove};
use spk_cli_group3::{cmd_export, cmd_import};
use spk_cli_group4::{cmd_search, cmd_version, cmd_view};
use spk_cmd_build::cmd_build;
use spk_cmd_convert::cmd_convert;
use spk_cmd_env::cmd_env;
use spk_cmd_explain::cmd_explain;
use spk_cmd_install::cmd_install;
use spk_cmd_make_binary::cmd_make_binary;
use spk_cmd_make_source::cmd_make_source;
use spk_cmd_render::cmd_render;
use spk_cmd_repo::cmd_repo;
use spk_cmd_test::cmd_test;
use spk_schema::foundation::format::FormatError;

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
        #[cfg(feature = "sentry")]
        let _sentry_guard = configure_sentry();

        let res = configure_logging(self.verbose).context("Failed to initialize output log");
        if let Err(err) = res {
            eprintln!("{}", err.to_string().red());
            return Ok(1);
        }

        // Disable this clippy warning because the result value is
        // used but only with the "sentry" feature enabled.
        #[allow(clippy::let_and_return)]
        let result = self.cmd.run().await;

        #[cfg(feature = "sentry")]
        if let Err(ref err) = result {
            // Send any error from run() to sentry, if configured.
            // This is here because the `_sentry_guard` is about to go
            // out of scope and close the connection. The error will
            // be output for the user in 'main()' below.
            match err.root_cause().downcast_ref::<Error>() {
                Some(Error::SpkSolverError(spk_solve::Error::SolverInterrupted(_))) => {
                    // SolverInterrupted errors are not sent to sentry
                    // here. A message has already been sent to sentry
                    // from io::Decision::Formatter::run_and_print_decisions()
                    // before it returns these errors.
                }
                _ => {
                    // Send all other errors that reach this level to sentry
                    sentry::with_scope(
                        |scope| {
                            let mut positional_args: Vec<String> = self.cmd.get_positional_args();
                            // Sort to make the fingerprinting consistent.
                            positional_args.sort();

                            let mut fingerprints: Vec<&str> =
                                Vec::with_capacity(positional_args.len() + 1);
                            fingerprints.push("{{ error.value }}");
                            fingerprints.extend(
                                positional_args.iter().map(|s| &**s).collect::<Vec<&str>>(),
                            );

                            scope.set_fingerprint(Some(&fingerprints));
                        },
                        || {
                            /*
                            // capture_error does not add a backtrace to
                            // sentry for the error event, unless backtraces
                            // are enabled for all events when the sentry
                            // client is configured. This causes less sentry
                            // empty backtrace noise:
                            sentry::capture_error(<anyhow::Error as AsRef<
                            (dyn std::error::Error + Send + Sync + 'static),
                            >>::as_ref(&_err));
                             */

                            // This will always add a backtrace to sentry for
                            // an error event, but it will be empty because
                            // these errors are not panics and as such have no
                            // backtrace data. This generates empty backtrace
                            // noise in sentry. Panics will have backtraces,
                            // but aren't handled by this, they are sent when
                            // the _sentry_guard goes out of scope.
                            sentry_anyhow::capture_anyhow(err);
                        },
                    );
                }
            }
        }

        result
    }
}

#[derive(Subcommand)]
pub enum Command {
    Bake(cmd_bake::Bake),
    Build(cmd_build::Build),
    Convert(cmd_convert::Convert),
    Deprecate(cmd_deprecate::DeprecateCmd),
    Env(cmd_env::Env),
    Explain(cmd_explain::Explain),
    Export(cmd_export::Export),
    Import(cmd_import::Import),
    Install(cmd_install::Install),
    Ls(cmd_ls::Ls),
    MakeBinary(cmd_make_binary::MakeBinary),
    MakeSource(cmd_make_source::MakeSource),
    New(cmd_new::New),
    #[clap(alias = "variant-count", hide = true)]
    NumVariants(cmd_num_variants::NumVariants),
    Publish(cmd_publish::Publish),
    Remove(cmd_remove::Remove),
    Render(cmd_render::Render),
    Repo(cmd_repo::Repo),
    Search(cmd_search::Search),
    Test(cmd_test::CmdTest),
    Undeprecate(cmd_undeprecate::Undeprecate),
    Version(cmd_version::Version),
    View(cmd_view::View),
}

// At the time of writing, enum_dispatch is not working to generate this code
// for traits that are defined in an external crate.

#[async_trait::async_trait]
impl Run for Command {
    async fn run(&mut self) -> Result<i32> {
        match self {
            Command::Bake(cmd) => cmd.run().await,
            Command::Build(cmd) => cmd.run().await,
            Command::Convert(cmd) => cmd.run().await,
            Command::Deprecate(cmd) => cmd.run().await,
            Command::Env(cmd) => cmd.run().await,
            Command::Explain(cmd) => cmd.run().await,
            Command::Export(cmd) => cmd.run().await,
            Command::Import(cmd) => cmd.run().await,
            Command::Install(cmd) => cmd.run().await,
            Command::Ls(cmd) => cmd.run().await,
            Command::MakeBinary(cmd) => cmd.run().await,
            Command::MakeSource(cmd) => cmd.run().await,
            Command::New(cmd) => cmd.run().await,
            Command::NumVariants(cmd) => cmd.run().await,
            Command::Publish(cmd) => cmd.run().await,
            Command::Remove(cmd) => cmd.run().await,
            Command::Render(cmd) => cmd.run().await,
            Command::Repo(cmd) => cmd.run().await,
            Command::Search(cmd) => cmd.run().await,
            Command::Test(cmd) => cmd.run().await,
            Command::Undeprecate(cmd) => cmd.run().await,
            Command::Version(cmd) => cmd.run().await,
            Command::View(cmd) => cmd.run().await,
        }
    }
}

impl CommandArgs for Command {
    fn get_positional_args(&self) -> Vec<String> {
        match self {
            Command::Bake(cmd) => cmd.get_positional_args(),
            Command::Build(cmd) => cmd.get_positional_args(),
            Command::Convert(cmd) => cmd.get_positional_args(),
            Command::Deprecate(cmd) => cmd.get_positional_args(),
            Command::Env(cmd) => cmd.get_positional_args(),
            Command::Explain(cmd) => cmd.get_positional_args(),
            Command::Export(cmd) => cmd.get_positional_args(),
            Command::Import(cmd) => cmd.get_positional_args(),
            Command::Install(cmd) => cmd.get_positional_args(),
            Command::Ls(cmd) => cmd.get_positional_args(),
            Command::MakeBinary(cmd) => cmd.get_positional_args(),
            Command::MakeSource(cmd) => cmd.get_positional_args(),
            Command::New(cmd) => cmd.get_positional_args(),
            Command::NumVariants(cmd) => cmd.get_positional_args(),
            Command::Publish(cmd) => cmd.get_positional_args(),
            Command::Remove(cmd) => cmd.get_positional_args(),
            Command::Render(cmd) => cmd.get_positional_args(),
            Command::Repo(cmd) => cmd.get_positional_args(),
            Command::Search(cmd) => cmd.get_positional_args(),
            Command::Test(cmd) => cmd.get_positional_args(),
            Command::Undeprecate(cmd) => cmd.get_positional_args(),
            Command::Version(cmd) => cmd.get_positional_args(),
            Command::View(cmd) => cmd.get_positional_args(),
        }
    }
}

#[tokio::main]
async fn main() {
    let mut opts = Opt::parse();
    let code = match opts.run().await {
        Ok(code) => code,
        Err(err) => {
            let root = err.root_cause();
            if let Some(err) = root.downcast_ref::<Error>() {
                eprintln!("{}", err.format_error(opts.verbose));
            } else {
                tracing::error!("{:?}", err);
            }
            1
        }
    };
    std::process::exit(code);
}
