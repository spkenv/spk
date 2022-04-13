// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use anyhow::Result;
use clap::Args;
use colored::Colorize;

use super::flags;

/// Export a package as a tar file
#[derive(Args)]
pub struct Export {
    #[clap(flatten)]
    pub requests: flags::Requests,

    #[clap(short, long, global = true, parse(from_occurrences))]
    pub verbose: u32,

    /// The package to export
    #[clap(name = "PKG")]
    pub package: String,

    /// The file to export into (Defaults to the name and version of the package)
    #[clap(name = "FILE")]
    pub filename: Option<std::path::PathBuf>,
}

impl Export {
    pub fn run(&self) -> Result<i32> {
        let pkg = self
            .requests
            .parse_idents([self.package.as_str()])?
            .pop()
            .unwrap();

        let mut build = String::new();
        if let Some(b) = &pkg.build {
            build = format!("_{b}");
        }
        let filename = self.filename.clone().unwrap_or_else(|| {
            std::path::PathBuf::from(format!("{}_{}{build}.spk", pkg.name(), pkg.version))
        });
        let res = spk::storage::export_package(&pkg, &filename);
        if let Err(spk::Error::PackageNotFoundError(_)) = res {
            tracing::warn!("Ensure that you are specifying at least a package and");
            tracing::warn!("version number when exporting from the local repository");
        }
        if res.is_err() {
            if let Err(err) = std::fs::remove_file(&filename) {
                tracing::warn!(?err, path=?filename, "failed to clean up incomplete archive");
            }
        }
        res?;
        println!("{}: {:?}", "Created".green(), filename);
        Ok(0)
    }
}
