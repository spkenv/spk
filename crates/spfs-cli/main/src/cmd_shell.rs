// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use clap::{ArgGroup, Args};
use miette::Result;
use spfs_cli_common as cli;

use super::cmd_run;
use super::cmd_run::Annotation;

/// Enter a subshell in a configured spfs environment
#[derive(Debug, Args)]
#[clap(group(
    ArgGroup::new("runtime_id")
    .required(true)
        .args(&["rerun", "REF"])))]
pub struct CmdShell {
    #[clap(flatten)]
    sync: cli::Sync,

    #[clap(flatten)]
    logging: cli::Logging,

    /// Mount the spfs filesystem in edit mode.
    ///
    /// Editable runtimes are created by default if REF is empty or not given.
    /// When combined with --rerun, the original runtime editability is overridden
    #[clap(short, long)]
    pub edit: bool,

    /// Mount the spfs filesystem in read-only mode.
    ///
    /// Read-only runtimes are created by default if REF is provided and not empty.
    /// When combined with --rerun, the original runtime editability is overridden
    #[clap(long, overrides_with = "edit")]
    pub no_edit: bool,

    /// Name of a previously run durable runtime to reuse for this run
    #[clap(long, value_name = "RUNTIME_NAME")]
    pub rerun: Option<String>,

    /// Requires --rerun. Force reset the process fields of the
    /// runtime before it is run again
    #[clap(long, requires = "rerun")]
    pub force: bool,

    /// Provide a name for this runtime to make it easier to identify
    #[clap(long)]
    runtime_name: Option<String>,

    /// Use to keep the runtime around rather than deleting it when
    /// the process exits. This is best used with '--name NAME' to
    /// make rerunning the runtime easier at a later time.
    #[clap(short, long, env = "SPFS_KEEP_RUNTIME")]
    pub keep_runtime: bool,

    #[clap(flatten)]
    pub annotation: Annotation,

    /// The tag or id of the desired runtime
    ///
    /// Use '-' or an empty string to request an empty environment
    #[clap(name = "REF")]
    reference: Option<spfs::tracking::EnvSpec>,
}

impl CmdShell {
    pub async fn run(&mut self, config: &spfs::Config) -> Result<i32> {
        let mut run_cmd = cmd_run::CmdRun {
            sync: self.sync.clone(),
            logging: self.logging.clone(),
            edit: self.edit,
            no_edit: self.no_edit,
            rerun: self.rerun.clone(),
            force: self.force,
            runtime_name: self.runtime_name.clone(),
            reference: self.reference.clone(),
            keep_runtime: self.keep_runtime,
            annotation: self.annotation.clone(),
            command: Default::default(),
        };
        run_cmd.run(config).await
    }
}
