// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use anyhow::{anyhow, Context, Result};
use clap::Args;
use colored::Colorize;

use super::flags;

/// Build a binary package from a spec file or source package.
#[derive(Args)]
#[clap(visible_aliases = &["mkbinary", "mkbin", "mkb"])]
pub struct MakeBinary {
    #[clap(flatten)]
    repos: flags::Repositories,
    #[clap(flatten)]
    options: flags::Options,
    #[clap(flatten)]
    runtime: flags::Runtime,

    /// Build from the current directory, instead of a source package)
    #[clap(long)]
    here: bool,

    /// Setup the build, but instead of running the build script start an interactive shell
    #[clap(long, short)]
    interactive: bool,

    /// Build the first variant of this package, and then immediately enter a shell environment with it
    #[clap(long, short)]
    env: bool,

    /// The packages or yaml spec files to build
    #[clap(required = true, name = "PKG|SPEC_FILE")]
    packages: Vec<String>,
}

impl MakeBinary {
    pub fn run(&self) -> Result<i32> {
        let runtime = self.runtime.ensure_active_runtime()?;

        // options = _flags.get_options_from_flags(args)
        // repos = list(_flags.get_repos_from_repo_flags(args).values())
        // for package in args.packages:
        //     if os.path.isfile(package):
        //         spec = spk.api.read_spec_file(package)
        //         _LOGGER.info("saving spec file", pkg=spec.pkg)
        //         spk.save_spec(spec)
        //     else:
        //         spec = spk.load_spec(package)

        //     _LOGGER.info("building binary package", pkg=spec.pkg)
        //     built = set()
        //     for variant in spec.build.variants:

        //         if not args.no_host:
        //             opts = spk.api.host_options()
        //         else:
        //             opts = spk.api.OptionMap()

        //         opts.update(variant)
        //         opts.update(options)
        //         if opts.digest in built:
        //             continue
        //         built.add(opts.digest)

        //         _LOGGER.info("building variant", variant=opts)
        //         builder = (
        //             spk.BinaryPackageBuilder.from_spec(spec)
        //             .with_options(opts)
        //             .with_repositories(repos)
        //         )
        //         if args.here:
        //             builder = builder.with_source(os.getcwd())
        //         builder.set_interactive(args.interactive)
        //         try:
        //             out = builder.build()
        //         except (ValueError, spk.SolverError):
        //             _LOGGER.error("build failed", variant=opts)
        //             if args.verbose:
        //                 graph = builder.get_solve_graph()
        //                 print(spk.io.format_solve_graph(graph, verbosity=args.verbose))
        //             raise
        //         else:
        //             _LOGGER.info("created", pkg=out.pkg)
        //         if args.env:
        //             cmd = ["spk", "env", "-l", str(out.pkg)]
        //             _LOGGER.info("entering environment of new package", cmd=" ".join(cmd))
        //             os.execvp(cmd[0], cmd)
        todo!()
    }
}
