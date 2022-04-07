// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::ffi::OsString;

use anyhow::{Context, Result};
use clap::Args;

use super::flags;

/// Output the contents of an spk environment (/spfs) to a folder
#[derive(Args)]
pub struct Render {
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

    // render_cmd.add_argument(
    //     "packages", metavar="PKG", nargs="+", help="The packages to resolve and render"
    // )
    // render_cmd.add_argument(
    //     "target", metavar="PATH", help="The empty directory to render into"
    // )
    // _flags.add_request_flags(render_cmd)
    // _flags.add_solver_flags(render_cmd)
}

impl Render {
    pub fn run(&self) -> Result<i32> {
        // solver = _flags.get_solver_from_flags(args)
        // for name in args.packages:
        //     solver.add_request(name)

        // for request in _flags.parse_requests_using_flags(args, *args.packages):
        //     solver.add_request(request)

        // try:
        //     generator = solver.run()
        //     spk.io.print_decisions(generator, args.verbose)
        //     solution = generator.solution()
        // except spk.SolverError as e:
        //     print(spk.io.format_error(e, args.verbose), file=sys.stderr)
        //     sys.exit(1)

        // solution = spk.build_required_packages(solution)
        // stack = spk.exec.resolve_runtime_layers(solution)
        // path = os.path.abspath(args.target)
        // os.makedirs(path, exist_ok=True)
        // if len(os.listdir(path)) != 0:
        //     print(
        //         spk.io.format_error(
        //             ValueError(f"Directory is not empty {path}"), args.verbose
        //         ),
        //         file=sys.stderr,
        //     )
        //     sys.exit(1)
        // _LOGGER.info(f"Rendering into dir: {path}")
        // spkrs.render_into_dir(stack, path)
        // _LOGGER.info(f"Render completed: {path}")
        todo!()
    }
}
