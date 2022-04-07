// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::ffi::OsString;

use anyhow::{Context, Result};
use clap::Args;

use super::flags;

/// Search for packages by name/substring
#[derive(Args)]
pub struct Search {
    #[clap(flatten)]
    pub solver: flags::Solver,
    #[clap(flatten)]
    pub options: flags::Options,
    #[clap(flatten)]
    pub runtime: flags::Runtime,
    #[clap(flatten)]
    pub requests: flags::Requests,

    #[clap(short, long, global = true, parse(from_occurrences))]
    pub verbose: u32,
    // _flags.add_repo_flags(search_cmd)
    // search_cmd.add_argument("term", metavar="TERM", help="The search term / substring")
}

impl Search {
    pub fn run(&self) -> Result<i32> {
        // repos = _flags.get_repos_from_repo_flags(args)

        // width = max(map(len, repos.keys()))
        // for repo_name, repo in repos.items():
        //     for name in repo.list_packages():
        //         if args.term in name:
        //             versions = list(
        //                 spk.api.Ident(name, v) for v in repo.list_package_versions(name)
        //             )
        //             for v in versions:
        //                 print(
        //                     ("{: <" + str(width) + "}").format(repo_name),
        //                     spk.io.format_ident(v),
        //                 )
        todo!()
    }
}
