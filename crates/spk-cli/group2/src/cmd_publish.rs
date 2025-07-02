// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::sync::Arc;

use clap::Args;
use miette::Result;
use spk_cli_common::{CommandArgs, PublishLabel, Publisher, Run};
use spk_schema::AnyIdent;
use spk_storage as storage;

#[cfg(test)]
#[path = "./cmd_publish_test.rs"]
mod cmd_publish_test;

/// Publish a package into a shared repository
#[derive(Args)]
pub struct Publish {
    #[clap(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// The repository to publish to
    ///
    /// Any configured spfs repository can be named here
    #[clap(long, short = 'r', default_value = "origin")]
    target_repo: String,

    /// Skip publishing the related source package, if any
    ///
    /// By not publishing the source package, you require that
    /// consumers use an existing binary build, they will not
    /// be able to build new versions of your package as needed.
    #[clap(long)]
    no_source: bool,

    /// Forcefully overwrite any existing publishes of the same package
    #[clap(long, short)]
    force: bool,

    /// Allow publishing builds when the version already exists and
    /// the existing version recipe contains the given metadata label
    /// and value.
    #[clap(long, hide = true, value_name = "LABEL=VALUE")]
    allow_existing_with_label: Option<PublishLabel>,

    /// The local packages to publish
    ///
    /// This can be an entire package version with all builds or a
    /// single, specific build.
    #[clap(name = "PKG", required = true)]
    pub packages: Vec<AnyIdent>,
}

#[async_trait::async_trait]
impl Run for Publish {
    type Output = i32;

    async fn run(&mut self) -> Result<Self::Output> {
        let (source, target) = tokio::try_join!(storage::local_repository(), async {
            Ok(Into::<storage::RepositoryHandle>::into(
                storage::remote_repository(&self.target_repo).await?,
            ))
        })?;

        let publisher = Publisher::new(Arc::new(source.into()), Arc::new(target))
            .skip_source_packages(self.no_source)
            .allow_existing_with_label(self.allow_existing_with_label.clone())
            .force(self.force);

        let mut published = Vec::new();
        for pkg in self.packages.iter() {
            published.extend(publisher.publish(pkg).await?);
        }

        if published.is_empty() {
            tracing::warn!(
                "No packages were published, did you forget to specify a version number? (spk publish my-package/1.0.2)"
            )
        }

        tracing::info!("done");
        Ok(0)
    }
}

impl CommandArgs for Publish {
    fn get_positional_args(&self) -> Vec<String> {
        // The important positional args for a publish are the packages
        self.packages
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<String>>()
    }
}
