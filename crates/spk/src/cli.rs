// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! Main entry points and utilities for command line interface and interaction.

use std::process::ExitCode;

use clap::{CommandFactory, Parser, Subcommand};
use colored::Colorize;
use miette::{Context, Result};
#[cfg(feature = "sentry")]
use spk_cli_common::configure_sentry;
use spk_cli_common::{CommandArgs, Error, Run, configure_logging};
use spk_cli_group1::{cmd_bake, cmd_completion, cmd_deprecate, cmd_undeprecate};
use spk_cli_group2::{cmd_ls, cmd_new, cmd_num_variants, cmd_publish, cmd_remove};
use spk_cli_group3::{cmd_export, cmd_import};
use spk_cli_group4::{cmd_lint, cmd_search, cmd_version, cmd_view};
use spk_cmd_build::cmd_build;
use spk_cmd_convert::cmd_convert;
use spk_cmd_debug::cmd_debug;
use spk_cmd_du::cmd_du;
use spk_cmd_env::cmd_env;
use spk_cmd_explain::cmd_explain;
use spk_cmd_install::cmd_install;
use spk_cmd_make_binary::cmd_make_binary;
use spk_cmd_make_recipe::cmd_make_recipe;
use spk_cmd_make_source::cmd_make_source;
use spk_cmd_render::cmd_render;
use spk_cmd_repo::cmd_repo;
use spk_cmd_test::cmd_test;
use spk_schema::foundation::format::FormatError;
#[cfg(feature = "statsd")]
use spk_solve::{
    SPK_ERROR_COUNT_METRIC,
    SPK_RUN_COUNT_METRIC,
    SPK_RUN_TIME_METRIC,
    get_metrics_client,
};

/// A Package Manager for SPFS
#[derive(Parser)]
#[command(name = "spk")]
#[command(author, version, about, long_about = None)]
pub struct Opt {
    #[clap(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,
    #[clap(subcommand)]
    pub cmd: Command,
}

impl Opt {
    pub async fn run(&mut self) -> Result<i32> {
        #[cfg(feature = "sentry")]
        let _sentry_guard = configure_sentry();

        #[cfg(feature = "statsd")]
        let statsd_client = {
            let client = get_metrics_client();
            if let Some(client) = client {
                client.incr(&SPK_RUN_COUNT_METRIC)
            }
            client
        };

        let res =
            // Safety: unless sentry is enabled, the process is single threaded
            // still and it is safe to set environment variables.
            unsafe { configure_logging(self.verbose) }.wrap_err("Failed to initialize output log");
        if let Err(err) = res {
            eprintln!("{}", err.to_string().red());
            #[cfg(feature = "statsd")]
            if let Some(client) = statsd_client {
                client.incr(&SPK_ERROR_COUNT_METRIC)
            }
            return Ok(1);
        }

        // Disable this clippy warning because the result value is
        // used but only with the "sentry" or "statsd" features enabled.
        #[allow(clippy::let_and_return)]
        let result = self.cmd.run().await;

        #[cfg(feature = "statsd")]
        if let Some(client) = statsd_client {
            client.record_duration_from_start(&SPK_RUN_TIME_METRIC)
        }

        #[cfg(feature = "sentry")]
        if let Err(ref err) = result {
            // Send any error from run() to sentry, if configured.
            // This is here because the `_sentry_guard` is about to go
            // out of scope and close the connection. The error will
            // be output for the user in 'main()' below.
            match err.root_cause().downcast_ref::<Error>() {
                Some(Error::SpkSolverError(solver_error))
                    if matches!(&**solver_error, spk_solve::Error::SolverInterrupted(_)) =>
                {
                    // SolverInterrupted errors are not sent to sentry
                    // here. A message has already been sent to sentry
                    // from io::Decision::Formatter before it returns
                    // these errors.
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
                            // capture_error does not add a backtrace to
                            // sentry for the error event, unless backtraces
                            // are enabled for all events when the sentry
                            // client is configured. Panics will have backtraces,
                            // but aren't handled by this, they are sent when
                            // the _sentry_guard goes out of scope.
                            sentry_miette::capture_miette(err);
                        },
                    );
                }
            }
        }

        #[cfg(feature = "statsd")]
        if result.is_err() {
            if let Some(client) = statsd_client {
                client.incr(&SPK_ERROR_COUNT_METRIC)
            }
        }

        result
    }
}

#[derive(Subcommand)]
pub enum Command {
    Bake(cmd_bake::Bake),
    Build(cmd_build::Build),
    Completion(cmd_completion::Completion),
    Convert(cmd_convert::Convert),
    Debug(cmd_debug::Debug),
    Deprecate(cmd_deprecate::DeprecateCmd),
    Du(cmd_du::Du),
    Env(cmd_env::Env),
    Explain(cmd_explain::Explain),
    Export(cmd_export::Export),
    Import(cmd_import::Import),
    Install(cmd_install::Install),
    Lint(cmd_lint::Lint),
    Ls(cmd_ls::Ls),
    MakeBinary(cmd_make_binary::MakeBinary),
    MakeSource(cmd_make_source::MakeSource),
    MakeRecipe(cmd_make_recipe::MakeRecipe),
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
    type Output = i32;

    async fn run(&mut self) -> Result<i32> {
        match self {
            Command::Bake(cmd) => cmd.run().await,
            Command::Build(cmd) => cmd.run().await.map(Into::into),
            Command::Completion(cmd) => cmd.run(Opt::command()),
            Command::Convert(cmd) => cmd.run().await,
            Command::Debug(cmd) => cmd.run().await,
            Command::Deprecate(cmd) => cmd.run().await,
            Command::Du(cmd) => cmd.run().await,
            Command::Env(cmd) => cmd.run().await,
            Command::Explain(cmd) => cmd.run().await,
            Command::Export(cmd) => cmd.run().await,
            Command::Import(cmd) => cmd.run().await,
            Command::Install(cmd) => cmd.run().await,
            Command::Lint(cmd) => cmd.run().await,
            Command::Ls(cmd) => cmd.run().await,
            Command::MakeBinary(cmd) => cmd.run().await,
            Command::MakeSource(cmd) => cmd.run().await,
            Command::MakeRecipe(cmd) => cmd.run().await,
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
            Command::Completion(cmd) => cmd.get_positional_args(),
            Command::Debug(cmd) => cmd.get_positional_args(),
            Command::Deprecate(cmd) => cmd.get_positional_args(),
            Command::Du(cmd) => cmd.get_positional_args(),
            Command::Env(cmd) => cmd.get_positional_args(),
            Command::Explain(cmd) => cmd.get_positional_args(),
            Command::Export(cmd) => cmd.get_positional_args(),
            Command::Import(cmd) => cmd.get_positional_args(),
            Command::Install(cmd) => cmd.get_positional_args(),
            Command::Lint(cmd) => cmd.get_positional_args(),
            Command::Ls(cmd) => cmd.get_positional_args(),
            Command::MakeBinary(cmd) => cmd.get_positional_args(),
            Command::MakeSource(cmd) => cmd.get_positional_args(),
            Command::MakeRecipe(cmd) => cmd.get_positional_args(),
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
async fn main() -> ExitCode {
    let mut opts = Opt::parse();
    let code = match opts.run().await {
        Ok(code) => code,
        Err(err) => {
            let root = err.root_cause();
            if let Some(err) = root.downcast_ref::<Error>() {
                eprintln!("{}", err.format_error(opts.verbose).await);
            } else {
                tracing::error!("{:?}", err);
            }
            1
        }
    };
    ExitCode::from(u8::try_from(code).ok().unwrap_or(1))
}
