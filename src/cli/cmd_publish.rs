// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::sync::Arc;

use anyhow::Result;
use clap::Args;

use super::Run;

/// Publish a package into a shared repository
#[derive(Args)]
pub struct Publish {
    #[clap(short, long, global = true, parse(from_occurrences))]
    pub verbose: u32,

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

    /// The local packages to publish
    ///
    /// This can be an entire package version with all builds or a
    /// single, specific build.
    #[clap(name = "PKG", required = true)]
    pub packages: Vec<spk::api::Ident>,
}

#[async_trait::async_trait]
impl Run for Publish {
    async fn run(&mut self) -> Result<i32> {
        let (source, target) = tokio::try_join!(
            spk::storage::local_repository(),
            spk::storage::remote_repository(&self.target_repo)
        )?;

        let publisher = spk::Publisher::new(Arc::new(source.into()), Arc::new(target.into()))
            .skip_source_packages(self.no_source)
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
