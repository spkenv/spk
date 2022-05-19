// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::{
    collections::VecDeque,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use colored::Colorize;
use once_cell::sync::Lazy;

use crate::{api, option_map, solve, Error, Result};

/// TODO: find the best place for this. It's here for now because the
/// ctrl-c handler is set up on cli/bin but that's not a module that is
/// accessible from here. Maybe it should be in global or lib?
pub static USER_CANCELED: AtomicBool = AtomicBool::new(false);

/// A solve has taken too long if it runs for more than this number of
/// seconds and hasn't found a soluton
// TODO: this is probably too high, consider changing this to about 10
// secs? Once there's a spk config file, this should get its default
// from that file.
static TOO_LONG: Lazy<u64> = Lazy::new(|| {
    std::env::var("SPK_SOLVE_TOO_LONG_SECONDS")
        .unwrap_or_else(|_| String::from("30"))
        .parse::<u64>()
        .unwrap()
});

/// Number of times to allow TOO_LONG timeouts to increase the
/// verbosity before just halting the solve so the problems so far,
/// and stats, can be seen. Setting this to 0, the default, means
/// don't ever halt the solver no matter how many TOO_LONG times have
/// passed.
// TODO: consider changing this to 20 (which is 10 mins assuming 30
// secs intervals)? Once there's a spk config file, this should get
// its default from that file.
static MAX_TOO_LONG_COUNT: Lazy<u64> = Lazy::new(|| {
    std::env::var("SPK_SOLVE_TOO_LONG_MAX_COUNT")
        .unwrap_or_else(|_| String::from("0"))
        .parse::<u64>()
        .unwrap()
});

pub fn format_ident(pkg: &api::Ident) -> String {
    let mut out = pkg.name.bold().to_string();
    if !pkg.version.is_zero() || pkg.build.is_some() {
        out = format!("{}/{}", out, pkg.version.to_string().bright_blue());
    }
    if let Some(ref b) = pkg.build {
        out = format!("{}/{}", out, format_build(b));
    }
    out
}

pub fn format_build(build: &api::Build) -> String {
    match build {
        api::Build::Embedded => build.digest().bright_magenta().to_string(),
        api::Build::Source => build.digest().bright_yellow().to_string(),
        _ => build.digest().dimmed().to_string(),
    }
}

pub fn format_options(options: &api::OptionMap) -> String {
    let formatted: Vec<String> = options
        .iter()
        .map(|(name, value)| format!("{}{}{}", name, "=".dimmed(), value.cyan()))
        .collect();
    format!("{{{}}}", formatted.join(", "))
}

/// Create a canonical string to describe the combined request for a package.
pub fn format_request<'a, R>(name: &api::PkgName, requests: R) -> String
where
    R: IntoIterator<Item = &'a api::PkgRequest>,
{
    let mut out = name.bold().to_string();
    let mut versions = Vec::new();
    let mut components = std::collections::HashSet::new();
    for req in requests.into_iter() {
        let mut version = req.pkg.version.to_string();
        if version.is_empty() {
            version.push('*')
        }
        let build = match req.pkg.build {
            Some(ref b) => format!("/{}", format_build(b)),
            None => "".to_string(),
        };
        versions.push(format!("{}{}", version.bright_blue(), build));
        components.extend(&mut req.pkg.components.iter().cloned());
    }
    if !components.is_empty() {
        out.push_str(&format!(":{}", format_components(&components).dimmed()));
    }
    out.push('/');
    out.push_str(&versions.join(","));
    out
}

pub fn format_components<'a, I>(components: I) -> String
where
    I: IntoIterator<Item = &'a api::Component>,
{
    let mut components: Vec<_> = components
        .into_iter()
        .map(api::Component::to_string)
        .collect();
    components.sort();
    let mut out = components.join(",");
    if components.len() > 1 {
        out = format!("{}{}{}", "{".dimmed(), out, "}".dimmed(),)
    }
    out
}

pub fn format_solution(solution: &solve::Solution, verbosity: u32) -> String {
    if solution.is_empty() {
        return "Nothing Installed".to_string();
    }
    let mut out = "Installed Packages:\n".to_string();
    for req in solution.items() {
        let mut installed = api::PkgRequest::from_ident(&req.spec.pkg);
        if let solve::PackageSource::Repository { components, .. } = req.source {
            let mut installed_components = req.request.pkg.components;
            if installed_components.remove(&api::Component::All) {
                installed_components.extend(components.keys().cloned());
            }
            installed.pkg.components = installed_components;
        }

        out.push_str(&format!(
            "  {}",
            format_request(&req.spec.pkg.name, &[installed])
        ));
        if verbosity > 0 {
            let options = req.spec.resolve_all_options(&api::OptionMap::default());
            out.push(' ');
            out.push_str(&format_options(&options));
        }
        out.push('\n');
    }
    out
}

pub fn format_note(note: &solve::graph::Note) -> String {
    use solve::graph::Note;
    match note {
        Note::SkipPackageNote(n) => {
            format!(
                "{} {} - {}",
                "TRY".magenta(),
                format_ident(&n.pkg),
                n.reason
            )
        }
        Note::Other(s) => format!("{} {}", "NOTE".magenta(), s),
    }
}

pub fn change_is_relevant_at_verbosity(change: &solve::graph::Change, verbosity: u32) -> bool {
    use solve::graph::Change::*;
    let relevant_level = match change {
        SetPackage(_) => 1,
        StepBack(_) => 1,
        RequestPackage(_) => 2,
        RequestVar(_) => 2,
        SetOptions(_) => 3,
        SetPackageBuild(_) => 1,
    };
    verbosity >= relevant_level
}

pub fn format_change(change: &solve::graph::Change, _verbosity: u32) -> String {
    use solve::graph::Change::*;
    match change {
        RequestPackage(c) => {
            format!(
                "{} {}",
                "REQUEST".blue(),
                format_request(&c.request.pkg.name, [&c.request])
            )
        }
        RequestVar(c) => {
            format!(
                "{} {}",
                "REQUEST".blue(),
                format_options(&option_map! {c.request.var.clone() => c.request.value.clone()})
            )
        }
        SetPackageBuild(c) => {
            format!("{} {}", "BUILD".yellow(), format_ident(&c.spec.pkg))
        }
        SetPackage(c) => {
            format!("{} {}", "RESOLVE".green(), format_ident(&c.spec.pkg))
        }
        SetOptions(c) => {
            format!("{} {}", "ASSIGN".cyan(), format_options(&c.options))
        }
        StepBack(c) => {
            format!("{} {}", "BLOCKED".red(), c.cause)
        }
    }
}

pub fn format_decisions<'a, I>(decisions: I, verbosity: u32) -> FormattedDecisionsIter<I::IntoIter>
where
    I: IntoIterator<Item = Result<(solve::graph::Node, solve::graph::Decision)>> + 'a,
{
    FormattedDecisionsIter::new(decisions, verbosity)
}

pub struct FormattedDecisionsIter<I>
where
    I: Iterator<Item = Result<(solve::graph::Node, solve::graph::Decision)>>,
{
    inner: I,
    level: usize,
    verbosity: u32,
    output_queue: VecDeque<String>,
    // For too long and ctrlc interruption checks during solver steps
    start: Instant,
    too_long: Duration,
    too_long_counter: u64,
}

impl<I> FormattedDecisionsIter<I>
where
    I: Iterator<Item = Result<(solve::graph::Node, solve::graph::Decision)>>,
{
    pub fn new<T>(inner: T, verbosity: u32) -> Self
    where
        T: IntoIterator<IntoIter = I>,
    {
        Self {
            inner: inner.into_iter(),
            level: 0,
            verbosity,
            output_queue: VecDeque::new(),
            start: Instant::now(),
            too_long: Duration::from_secs(*TOO_LONG),
            too_long_counter: 0,
        }
    }

    fn check_for_interruptions(&mut self) -> Result<()> {
        if let Err(err) = self.check_if_taking_too_long() {
            return Err(err);
        };
        self.check_if_user_hit_ctrlc()
    }

    fn check_if_taking_too_long(&mut self) -> Result<()> {
        if self.start.elapsed() > self.too_long {
            self.too_long_counter += 1;

            // Check how many times this has increased the verbosity.
            if *MAX_TOO_LONG_COUNT > 0 && self.too_long_counter >= *MAX_TOO_LONG_COUNT {
                // The verbosity has been increased too many times
                // now. The solve has taken far too long, so stop it
                // with an interruption error. This lets the caller
                // show the issues and problem packages encountered up
                // to this point to the user, which may help them see
                // where the solve is bogged down.
                return Err(Error::Solve(solve::Error::SolverInterrupted(format!("Solve is taking far too long, >{} secs.\nStopping. Please review the problems hit so far ...", *MAX_TOO_LONG_COUNT * *TOO_LONG))));
            }

            // The maximum number of increases hasn't been hit.
            // Increase the verbosity level of this solve.
            if self.verbosity < u32::MAX {
                self.verbosity += 1;
                eprintln!(
                    "Solve is taking too long, >{} secs. Increasing verbosity level to {}",
                    self.too_long.as_secs(),
                    self.verbosity
                );
            }
            self.start = Instant::now();
        }
        Ok(())
    }

    fn check_if_user_hit_ctrlc(&self) -> Result<()> {
        // Check if the solve has been interrupted by the user (via ctrl-c)
        if USER_CANCELED.load(Ordering::Relaxed) {
            return Err(Error::Solve(solve::Error::SolverInterrupted(
                "Solver interrupted by user ...".to_string(),
            )));
        }
        Ok(())
    }
}

impl<I> Iterator for FormattedDecisionsIter<I>
where
    I: Iterator<Item = Result<(solve::graph::Node, solve::graph::Decision)>>,
{
    type Item = Result<String>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(next) = self.output_queue.pop_front() {
            return Some(Ok(next));
        }

        while self.output_queue.is_empty() {
            // First, check if the solver has taken too long or the
            // user has interrupted the solver
            if let Err(err) = self.check_for_interruptions() {
                return Some(Err(err));
            }

            let decision = match self.inner.next() {
                None => return None,
                Some(Ok((_, d))) => d,
                Some(Err(err)) => return Some(Err(err)),
            };

            if self.verbosity > 1 {
                let fill: String = ".".repeat(self.level);
                for note in decision.notes.iter() {
                    self.output_queue
                        .push_back(format!("{} {}", fill, format_note(note)));
                }
            }

            let mut fill: &str;
            let mut level_change: i64 = 1;
            for change in decision.changes.iter() {
                use solve::graph::Change::*;
                match change {
                    SetPackage(change) => {
                        if change.spec.pkg.build == Some(api::Build::Embedded) {
                            fill = ".";
                        } else {
                            fill = ">";
                        }
                    }
                    StepBack(_) => {
                        fill = "!";
                        level_change = -1;
                    }
                    _ => {
                        fill = ".";
                    }
                }

                if !change_is_relevant_at_verbosity(change, self.verbosity) {
                    continue;
                }

                let prefix: String = fill.repeat(self.level);
                self.output_queue.push_back(format!(
                    "{} {}",
                    prefix,
                    format_change(change, self.verbosity)
                ))
            }
            self.level = (self.level as i64 + level_change) as usize;
        }
        self.output_queue.pop_front().map(Ok)
    }
}

pub fn format_error(err: &Error, verbosity: u32) -> String {
    let mut msg = String::new();
    match err {
        Error::PackageNotFoundError(pkg) => {
            msg.push_str("Package not found: ");
            msg.push_str(&format_ident(pkg));
            msg.push('\n');
            msg.push_str(
                &" * check the spelling of the name\n"
                    .yellow()
                    .dimmed()
                    .to_string(),
            );
            msg.push_str(
                &" * ensure that you have enabled the right repositories"
                    .yellow()
                    .dimmed()
                    .to_string(),
            )
        }
        Error::Solve(err) => {
            msg.push_str("Failed to resolve");
            match err {
                solve::Error::FailedToResolve(_graph) => {
                    // TODO: provide a summary based on the graph
                }
                solve::Error::OutOfOptions(_) => {
                    msg.push_str("\n * out of options");
                }
                solve::Error::SolverError(reason) => {
                    msg.push_str("\n * ");
                    msg.push_str(reason);
                }
                solve::Error::Graph(err) => {
                    msg.push_str("\n * ");
                    msg.push_str(&err.to_string());
                }
                solve::Error::SolverInterrupted(err) => {
                    msg.push_str("\n * ");
                    msg.push_str(err);
                }
            }
            match verbosity {
                0 => {
                    msg.push_str(
                        &"\n * try '--verbose/-v' for more info"
                            .dimmed()
                            .yellow()
                            .to_string(),
                    );
                }
                1 => {
                    msg.push_str(
                        &"\n * try '-vv' for even more info"
                            .dimmed()
                            .yellow()
                            .to_string(),
                    );
                }
                2 => {
                    msg.push_str(
                        &"\n * try '-vvv' for even more info"
                            .dimmed()
                            .yellow()
                            .to_string(),
                    );
                }
                3.. => (),
            }
        }
        Error::String(err) => msg.push_str(err),
        err => msg.push_str(&err.to_string()),
    }
    msg.red().to_string()
}

/// Run the solver to completion, printing each step to stdout
/// as appropriate given a verbosity level.
pub fn run_and_print_resolve(
    solver: &solve::Solver,
    verbosity: u32,
    report_time: bool,
) -> Result<solve::Solution> {
    let mut runtime = solver.run();
    run_and_print_decisions(&mut runtime, verbosity, report_time)
}

/// Run the solver runtime to completion, printing each step to stdout
/// as appropriate given a verbosity level.
pub fn run_and_print_decisions(
    mut runtime: &mut solve::SolverRuntime,
    verbosity: u32,
    report_time: bool,
) -> Result<solve::Solution> {
    // Step through the solver runtime's decisions - this runs the solver
    let start = Instant::now();
    for line in format_decisions(&mut runtime, verbosity) {
        match line {
            Ok(message) => println!("{message}"),
            Err(e) => {
                match e {
                    Error::Solve(serr) => match serr {
                        solve::Error::SolverInterrupted(mesg) => {
                            // Note: the solution probably won't be
                            // complete because of the interruption.
                            let solve_time = start.elapsed();
                            eprintln!("{}", mesg.yellow());
                            eprintln!("{}", format_solve_stats(&runtime.solver, solve_time));
                            return Err(Error::Solve(solve::Error::SolverInterrupted(mesg)));
                        }
                        _ => return Err(Error::Solve(serr)),
                    },
                    _ => return Err(e),
                };
            }
        };
    }

    // Note: this time includes the output time because the solver is
    // run in the format_decisions() call above
    if report_time {
        println!("{}", format_solve_stats(&runtime.solver, start.elapsed()));
    }

    runtime.current_solution()
}

pub fn format_solve_stats(solver: &solve::Solver, solve_duration: Duration) -> String {
    // Show how long this solve took
    let mut out: String = " Solver took: ".to_string();
    out.push_str(&format!(
        "{} seconds\n",
        solve_duration.as_secs() as f64 + solve_duration.subsec_nanos() as f64 * 1e-9
    ));

    // TODO: Add more stats here

    // Show all errors sorted by highest to lowest frequency
    let errors = solver.error_frequency();
    if !errors.is_empty() {
        // TODO: there can be lots of low count problems, might want a
        // minimum count cutoff, perhaps a couple of orders of
        // magnitude below the highest count one, or just the top 20 errors?
        out.push_str(" Solver hit these problems:");

        // Get a reverse sorted list (by count/frequency) of the error
        // messages
        let mut sorted_by_count: Vec<(&String, &u64)> = errors.iter().collect();
        sorted_by_count.sort_by(|a, b| b.1.cmp(a.1));

        for (error_mesg, count) in sorted_by_count {
            if error_mesg == "Exception: Branch already attempted" {
                // Skip these, they don't help the user isolate the problem
                continue;
            }
            let times = if *count > 1 { "times" } else { "time" };
            out.push_str(&format!("\n   {count} {times} {error_mesg}"));
        }
    } else {
        out.push_str(" Solver hit no problems");
    }

    out
}
