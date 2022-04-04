// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use anyhow::Result;
use clap::Args;

use super::flags;

/// Resolve and run an environment on-the-fly
///
/// Use '--' to separate the command from requests. If no command is given,
/// spawn a new shell
#[derive(Args)]
#[clap(visible_alias = "run")]
pub struct Env {
    #[clap(flatten)]
    pub solver: flags::Solver,
    #[clap(flatten)]
    pub options: flags::Options,
    #[clap(flatten)]
    pub runtime: flags::Runtime,

    #[clap(short, long, global = true, parse(from_occurrences))]
    pub verbose: u32,

    /// The requests to resolve and run
    #[clap()]
    pub requests: Vec<String>,

    /// An optional command to run in the resolved environment.
    ///
    /// Use '--' to separate the command from requests. If no command is given,
    /// spawn a new shell
    #[clap(raw = true)]
    pub command: Vec<String>,
}

impl Env {
    pub fn run(&self) -> Result<i32> {
        // // parse args again to get flags that might be missed
        // // while using the argparse.REMAINDER flag above
        // extra_parser = argparse.ArgumentParser()
        // extra_parser.add_argument("--verbose", "-v", action="count", default=0)
        // add_env_flags(extra_parser)

        // try:
        //     separator = args.args.index("--")
        // except ValueError:
        //     separator = len(args.args)
        // requests = args.args[:separator]
        // command = args.args[separator + 1 :] or []
        // args, requests = extra_parser.parse_known_args(requests, args)

        // _flags.ensure_active_runtime(args)

        // solver = _flags.get_solver_from_flags(args)

        // for request in _flags.parse_requests_using_flags(args, *requests):
        //     solver.add_request(request)

        // try:
        //     generator = solver.run()
        //     spk.io.print_decisions(generator, args.verbose)
        //     solution = generator.solution()
        // except spk.SolverError as e:
        //     print(spk.io.format_error(e, args.verbose), file=sys.stderr)
        //     sys.exit(1)

        // if args.verbose > 1:
        //     print(spk.io.format_solution(solution, args.verbose))

        // solution = spk.build_required_packages(solution)
        // spk.setup_current_runtime(solution)
        // env = solution.to_environment(os.environ)
        // os.environ.clear()
        // os.environ.update(env)
        // if not command:
        //     cmd = spkrs.build_interactive_shell_command()
        // else:
        //     cmd = spkrs.build_shell_initialized_command(*command)
        // os.execvp(cmd[0], cmd)
        todo!()
    }
}
