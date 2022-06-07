// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::{
    collections::VecDeque,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use once_cell::sync::Lazy;

use colored::Colorize;

use crate::{api, option_map, solve, Error, Result};

static USER_CANCELLED: Lazy<AtomicBool> = Lazy::new(|| {
    // Set up a ctrl-c handler to allow a solve to be interrupted
    // gracefully by the user from the FormatterDecisionIter below
    if let Err(err) = ctrlc::set_handler(|| {
        USER_CANCELLED.store(true, Ordering::Relaxed);
    }) {
        eprintln!(
            "Unable to setup ctrl-c handler for USER_CANCELLED because: {}",
            err.to_string().red()
        );
    };
    // Initialise the USER_CANCELLED value
    AtomicBool::new(false)
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

pub struct FormattedDecisionsIter<I>
where
    I: Iterator<Item = Result<(solve::graph::Node, solve::graph::Decision)>>,
{
    inner: I,
    level: usize,
    output_queue: VecDeque<String>,
    verbosity: u32,
    // For "too long" and ctrl-c interruption checks during solver steps
    start: Instant,
    too_long_counter: u64,
    settings: DecisionFormatterSettings,
}

impl<I> FormattedDecisionsIter<I>
where
    I: Iterator<Item = Result<(solve::graph::Node, solve::graph::Decision)>>,
{
    pub(crate) fn new<T>(inner: T, settings: DecisionFormatterSettings) -> Self
    where
        T: IntoIterator<IntoIter = I>,
    {
        Self {
            inner: inner.into_iter(),
            level: 0,
            output_queue: VecDeque::new(),
            verbosity: settings.verbosity,
            start: Instant::now(),
            too_long_counter: 0,
            settings,
        }
    }

    fn check_for_interruptions(&mut self) -> Result<()> {
        if let Err(err) = self.check_if_taking_too_long() {
            return Err(err);
        };
        self.check_if_user_hit_ctrlc()
    }

    fn check_if_taking_too_long(&mut self) -> Result<()> {
        if self.start.elapsed() > self.settings.too_long {
            self.too_long_counter += 1;

            // Check how many times this has increased the verbosity.
            if self.settings.max_too_long_count > 0
                && self.too_long_counter >= self.settings.max_too_long_count
            {
                // The verbosity has been increased too many times
                // now. The solve has taken far too long, so stop it
                // with an interruption error. This lets the caller
                // show the issues and problem packages encountered up
                // to this point to the user, which may help them see
                // where the solve is bogged down.
                return Err(Error::Solve(solve::Error::SolverInterrupted(format!("Solve is taking far too long, > {} secs.\nStopping. Please review the problems hit so far ...", self.settings.max_too_long_count * self.settings.too_long.as_secs()))));
            }

            // The maximum number of increases hasn't been hit.
            // Increase the verbosity level of this solve.
            if self.verbosity < u32::MAX {
                self.verbosity += 1;
                eprintln!(
                    "Solve is taking too long, > {} secs. Increasing verbosity level to {}",
                    self.settings.too_long.as_secs(),
                    self.verbosity
                );
            }
            self.start = Instant::now();
        }
        Ok(())
    }

    fn check_if_user_hit_ctrlc(&self) -> Result<()> {
        // Check if the solve has been interrupted by the user (via ctrl-c)
        if USER_CANCELLED.load(Ordering::Relaxed) {
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

pub struct DecisionFormatterBuilder {
    verbosity: u32,
    time: bool,
    verbosity_increase_seconds: u64,
    timeout: u64,
}

impl Default for DecisionFormatterBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl DecisionFormatterBuilder {
    pub fn new() -> Self {
        Self {
            verbosity: 0,
            time: false,
            verbosity_increase_seconds: 0,
            timeout: 0,
        }
    }

    pub fn with_verbosity(&mut self, verbosity: u32) -> &mut Self {
        self.verbosity = verbosity;
        self
    }

    pub fn with_time_and_stats(&mut self, time: bool) -> &mut Self {
        self.time = time;
        self
    }

    pub fn with_verbosity_increase_every(&mut self, seconds: u64) -> &mut Self {
        self.verbosity_increase_seconds = seconds;
        self
    }

    pub fn with_timeout(&mut self, timeout: u64) -> &mut Self {
        self.timeout = timeout;
        self
    }

    pub fn build(&self) -> DecisionFormatter {
        let too_long_seconds = if self.verbosity_increase_seconds == 0
            || (self.verbosity_increase_seconds > self.timeout && self.timeout > 0)
        {
            // If verbosity increases are not turned on, or are more
            // than the timeout and the timeout is set, then set this
            // to the timeout seconds. This will ensure max_count is
            // set to 1 below, and that will mean the first time the
            // "is it taking too long" check triggers will be at
            // timeout seconds.
            self.timeout
        } else {
            // Verbosity increases are turned on, and are less than the
            // timeout value or the timeout is not set.
            self.verbosity_increase_seconds
        };

        // Work out the correct maximum number of "is it taking too
        // long" checks before stopping a solve
        let max_too_long_checks = if self.timeout > 0 {
            (self.timeout as f64 / too_long_seconds as f64).ceil() as u64
        } else {
            0
        };

        DecisionFormatter {
            settings: DecisionFormatterSettings {
                verbosity: self.verbosity,
                report_time: self.time,
                too_long: Duration::from_secs(too_long_seconds),
                max_too_long_count: max_too_long_checks,
            },
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub(crate) struct DecisionFormatterSettings {
    pub(crate) verbosity: u32,
    pub(crate) report_time: bool,
    pub(crate) too_long: Duration,
    pub(crate) max_too_long_count: u64,
}

#[derive(Debug, Copy, Clone)]
pub struct DecisionFormatter {
    pub(crate) settings: DecisionFormatterSettings,
}

impl DecisionFormatter {
    /// Run the solver to completion, printing each step to stdout
    /// as appropriate given a verbosity level.
    pub fn run_and_print_resolve(&self, solver: &solve::Solver) -> Result<solve::Solution> {
        let mut runtime = solver.run();
        self.run_and_print_decisions(&mut runtime)
    }

    /// Run the solver runtime to completion, printing each step to stdout
    /// as appropriate given a verbosity level.
    pub fn run_and_print_decisions(
        &self,
        mut runtime: &mut solve::SolverRuntime,
    ) -> Result<solve::Solution> {
        // Step through the solver runtime's decisions - this runs the solver
        let start = Instant::now();
        for line in self.formatted_decisions_iter(&mut runtime) {
            match line {
                Ok(message) => println!("{message}"),
                Err(e) => {
                    match e {
                        Error::Solve(solve::Error::SolverInterrupted(mesg)) => {
                            // Note: the solution probably won't be
                            // complete because of the interruption.
                            let solve_time = start.elapsed();
                            eprintln!("{}", mesg.yellow());
                            eprintln!("{}", self.format_solve_stats(&runtime.solver, solve_time));
                            return Err(Error::Solve(solve::Error::SolverInterrupted(mesg)));
                        }
                        _ => return Err(e),
                    };
                }
            };
        }

        // Note: this time includes the output time because the solver is
        // run in the iterator in the format_decisions_iter() loop above
        if self.settings.report_time {
            println!(
                "{}",
                self.format_solve_stats(&runtime.solver, start.elapsed())
            );
        }

        runtime.current_solution()
    }

    /// Given a sequence of decisions, returns an iterator
    ///
    pub fn formatted_decisions_iter<'a, I>(
        &self,
        decisions: I,
    ) -> FormattedDecisionsIter<I::IntoIter>
    where
        I: IntoIterator<Item = Result<(solve::graph::Node, solve::graph::Decision)>> + 'a,
    {
        FormattedDecisionsIter::new(decisions, self.settings)
    }

    pub(crate) fn format_solve_stats(
        &self,
        solver: &solve::Solver,
        solve_duration: Duration,
    ) -> String {
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
}
