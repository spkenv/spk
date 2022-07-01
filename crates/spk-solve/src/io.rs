// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::VecDeque;
use std::fmt::Write;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_stream::stream;
use colored::Colorize;
use console::Term;
use futures::{Stream, StreamExt};
use itertools::Itertools;
use once_cell::sync::Lazy;
use spk_schema::foundation::format::{
    FormatChange,
    FormatChangeOptions,
    FormatIdent,
    FormatOptionMap,
    FormatRequest,
    FormatSolution,
};
use spk_schema::foundation::ident_build::Build;
use spk_schema::foundation::spec_ops::PackageOps;
use spk_solve_graph::{
    Change,
    Decision,
    Node,
    Note,
    DUPLICATE_REQUESTS_COUNT,
    REQUESTS_FOR_SAME_PACKAGE_COUNT,
};

use crate::solver::ErrorFreq;
use crate::{Error, ResolverCallback, Result, Solution, Solver, SolverRuntime, StatusLine};

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

pub fn format_note(note: &Note) -> String {
    match note {
        Note::SkipPackageNote(n) => {
            format!(
                "{} {} - {}",
                "TRY".magenta(),
                n.pkg.format_ident(),
                n.reason
            )
        }
        Note::Other(s) => format!("{} {}", "NOTE".magenta(), s),
    }
}

pub fn change_is_relevant_at_verbosity(change: &Change, verbosity: u32) -> bool {
    use Change::*;
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

pub struct FormattedDecisionsIter<I>
where
    I: Stream<Item = Result<(Arc<Node>, Arc<Decision>)>>,
{
    inner: Pin<Box<I>>,
    level: u64,
    output_queue: VecDeque<String>,
    verbosity: u32,
    // For "too long" and ctrl-c interruption checks during solver steps
    start: Instant,
    too_long_counter: u64,
    settings: DecisionFormatterSettings,
    status_line: StatusLine,
    status_line_rendered_hash: u64,
}

impl<I> FormattedDecisionsIter<I>
where
    I: Stream<Item = Result<(Arc<Node>, Arc<Decision>)>>,
{
    pub(crate) fn new<T>(inner: T, settings: DecisionFormatterSettings) -> Self
    where
        T: Into<I>,
    {
        Self {
            inner: Box::pin(inner.into()),
            level: 0,
            output_queue: VecDeque::new(),
            verbosity: settings.verbosity,
            start: Instant::now(),
            too_long_counter: 0,
            settings,
            status_line: StatusLine::new(Term::stdout(), 3),
            status_line_rendered_hash: 0,
        }
    }

    fn check_for_interruptions(&mut self) -> Result<()> {
        self.check_if_taking_too_long()?;
        self.check_if_user_hit_ctrlc()
    }

    fn check_if_taking_too_long(&mut self) -> Result<()> {
        if !self.settings.too_long.is_zero() && self.start.elapsed() > self.settings.too_long {
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
                return Err(Error::SolverInterrupted(format!("Solve is taking far too long, > {} secs.\nStopping. Please review the problems hit so far ...", self.settings.max_too_long_count * self.settings.too_long.as_secs())));
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
            return Err(Error::SolverInterrupted(
                "Solver interrupted by user ...".to_string(),
            ));
        }
        Ok(())
    }

    pub fn iter(&mut self) -> impl Stream<Item = Result<String>> + '_ {
        stream! {
            'outer: loop {
                if let Some(next) = self.output_queue.pop_front() {
                    yield Ok(next);
                    continue 'outer;
                }

                while self.output_queue.is_empty() {
                    // First, check if the solver has taken too long or the
                    // user has interrupted the solver
                    if let Err(err) = self.check_for_interruptions() {
                        yield Err(err);
                        continue 'outer;
                    }

                    let (node, decision) = match self.inner.next().await {
                        None => break 'outer,
                        Some(Ok((n, d))) => (n, d),
                        Some(Err(err)) => {
                            yield Err(err);
                            continue 'outer;
                        }
                    };

                    let resolved_packages_hash = node.state.get_resolved_packages_hash();
                    if resolved_packages_hash != self.status_line_rendered_hash {
                        let packages = node.state.get_ordered_resolved_packages();
                        let mut renders = Vec::with_capacity(packages.len());
                        for package in packages.iter() {
                            let name = package.name().as_str();
                            let version = package.version().to_string();
                            let build = package.ident().build.as_ref().unwrap().to_string();
                            let max_len = name.len().max(version.len()).max(build.len());
                            renders.push((name, version, build, max_len));
                        }
                        for row in 0..3 {
                            self.status_line.set_status(
                                row,
                                renders
                                    .iter()
                                    .map(|item| {
                                        format!(
                                            "{:width$}",
                                            match row {
                                                0 => item.0,
                                                1 => &item.1,
                                                2 => &item.2,
                                                _ => unreachable!(),
                                            },
                                            width = item.3
                                        )
                                    })
                                    .join(" |"),
                            );
                        }
                        self.status_line_rendered_hash = resolved_packages_hash
                    }

                    if self.verbosity > 5 {
                        // Show the state's package requests and resolved
                        // packages. This does not use indentation to make
                        // this "State ...:" debugging output stand out from
                        // the other changes.
                        self.output_queue.push_back(format!(
                            "{} {}",
                            "State Requests:".yellow(),
                            node.state
                                .get_pkg_requests()
                                .iter()
                                .map(|r| r.format_request(
                                    &r.pkg.repository_name,
                                    &r.pkg.name,
                                    &FormatChangeOptions {
                                        verbosity: self.verbosity,
                                        level: self.level
                                    },
                                ))
                                .collect::<Vec<String>>()
                                .join(", ")
                        ));
                        self.output_queue.push_back(format!(
                            "{} {}",
                            "State Resolved:".yellow(),
                            node.state
                                .get_resolved_packages()
                                .values()
                                .map(|p| (*p).0.ident().format_ident())
                                .collect::<Vec<String>>()
                                .join(", ")
                        ));
                    }

                    if self.verbosity > 9 {
                        // Show the state's var requests and resolved options
                        self.output_queue.push_back(format!(
                            "{} {:?}",
                            "State  VarReqs:".yellow(),
                            node.state
                                .get_var_requests()
                                .iter()
                                .map(|v| format!("{}: {}", v.var, v.value))
                                .collect::<Vec<String>>()
                                .join(", ")
                        ));
                        self.output_queue.push_back(format!(
                            "{} {}",
                            "State  Options:".yellow(),
                            node.state.get_option_map().format_option_map()
                        ));
                    }

                    if self.verbosity > 1 {
                        let fill: String = ".".repeat(self.level as usize);
                        for note in decision.notes.iter() {
                            self.output_queue
                                .push_back(format!("{} {}", fill, format_note(note)));
                        }
                    }

                    let mut fill: &str;
                    let mut new_level = self.level + 1;
                    for change in decision.changes.iter() {
                        use Change::*;
                        match change {
                            SetPackage(change) => {
                                if matches!(change.spec.ident().build, Some(Build::Embedded(_))) {
                                    fill = ".";
                                } else {
                                    fill = ">";
                                }
                            }
                            StepBack(spk_solve_graph::StepBack { destination, .. }) => {
                                fill = "!";
                                new_level = destination.state_depth;
                            }
                            _ => {
                                fill = ".";
                            }
                        }

                        if !change_is_relevant_at_verbosity(change, self.verbosity) {
                            continue;
                        }

                        if self.verbosity > 2 && self.level > 5 {
                            // Add level number into the lines to save having
                            // to count the indentation fill characters when
                            // dealing with larger numbers of decision levels.
                            let level_text = self.level.to_string();
                            // The +1 is for the space after 'level_text' in the string
                            let prefix_width = level_text.len() + 1;
                            let prefix = fill.repeat(self.level as usize - prefix_width);
                            self.output_queue.push_back(format!(
                                "{} {} {}",
                                level_text,
                                prefix,
                                change.format_change(
                                    &FormatChangeOptions {
                                        verbosity: self.verbosity,
                                        level: self.level
                                    },
                                    Some(&node.state)
                                )
                            ));
                        } else {
                            // Just use the fill prefix
                            let prefix: String = fill.repeat(self.level as usize);
                            self.output_queue.push_back(format!(
                                "{} {}",
                                prefix,
                                change.format_change(
                                    &FormatChangeOptions {
                                        verbosity: self.verbosity,
                                        level: self.level
                                    },
                                    Some(&node.state)
                                )
                            ))
                        }
                    }
                    self.level = new_level;
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct DecisionFormatterBuilder {
    verbosity: u32,
    time: bool,
    verbosity_increase_seconds: u64,
    timeout: u64,
    show_solution: bool,
    heading_prefix: String,
    long_solves_threshold: u64,
    max_frequent_errors: usize,
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
            show_solution: false,
            heading_prefix: String::from(""),
            long_solves_threshold: 0,
            max_frequent_errors: 0,
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

    pub fn with_solution(&mut self, show_solution: bool) -> &mut Self {
        self.show_solution = show_solution;
        self
    }

    pub fn with_header<S: Into<String>>(&mut self, heading: S) -> &mut Self {
        self.heading_prefix = heading.into();
        self
    }

    pub fn with_long_solves_threshold(&mut self, long_solves: u64) -> &mut Self {
        self.long_solves_threshold = long_solves;
        self
    }

    pub fn with_max_frequent_errors(&mut self, max_frequent_errors: usize) -> &mut Self {
        self.max_frequent_errors = max_frequent_errors;
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
                show_solution: self.show_solution || self.verbosity > 0,
                heading_prefix: String::from(""),
                long_solves_threshold: self.long_solves_threshold,
                max_frequent_errors: self.max_frequent_errors,
            },
        }
    }
}

#[cfg(feature = "sentry")]
#[derive(Debug, Clone)]
enum SentryWarning {
    LongSolve,
    SolverInterruptedByUser,
    SolverInterruptedByTimeout,
}

#[derive(Debug, Clone)]
pub(crate) struct DecisionFormatterSettings {
    pub(crate) verbosity: u32,
    pub(crate) report_time: bool,
    pub(crate) too_long: Duration,
    pub(crate) max_too_long_count: u64,
    pub(crate) show_solution: bool,
    /// This is followed immediately by "Installed Packages"
    pub(crate) heading_prefix: String,
    pub(crate) long_solves_threshold: u64,
    pub(crate) max_frequent_errors: usize,
}

#[derive(Debug, Clone)]
pub struct DecisionFormatter {
    pub(crate) settings: DecisionFormatterSettings,
}

impl DecisionFormatter {
    /// Run the solver to completion, printing each step to stdout
    /// as appropriate given a verbosity level.
    pub async fn run_and_print_resolve(&self, solver: &Solver) -> Result<Solution> {
        let mut runtime = solver.run();
        self.run_and_print_decisions(&mut runtime).await
    }

    /// Run the solver runtime to completion, printing each step to stdout
    /// as appropriate given a verbosity level.
    pub async fn run_and_print_decisions(&self, runtime: &mut SolverRuntime) -> Result<Solution> {
        enum LoopOutcome {
            Interrupted(String),
            Failed(Box<Error>),
            Success,
        }

        // Step through the solver runtime's decisions - this runs the solver
        let start = Instant::now();
        // This block exists to shorten the scope of `runtime`'s borrow.
        let loop_outcome = {
            let decisions = runtime.iter();
            let mut formatted_decisions = self.formatted_decisions_iter(decisions);
            let iter = formatted_decisions.iter();
            tokio::pin!(iter);
            #[allow(clippy::never_loop)]
            'outer: loop {
                while let Some(line) = iter.next().await {
                    match line {
                        Ok(message) => println!("{message}"),
                        Err(e) => {
                            match e {
                                Error::SolverInterrupted(mesg) => {
                                    break 'outer LoopOutcome::Interrupted(mesg);
                                }
                                _ => break 'outer LoopOutcome::Failed(Box::new(e)),
                            };
                        }
                    };
                }

                break LoopOutcome::Success;
            }
        };

        match loop_outcome {
            LoopOutcome::Interrupted(mesg) => {
                // Note: the solution probably won't be
                // complete because of the interruption.
                let solve_time = start.elapsed();

                // The solve was interrupted, record time taken and
                // other the details in sentry for later analysis.
                #[cfg(feature = "sentry")]
                self.send_sentry_warning_message(
                    &runtime.solver,
                    solve_time,
                    if mesg.contains("by user") {
                        SentryWarning::SolverInterruptedByUser
                    } else {
                        SentryWarning::SolverInterruptedByTimeout
                    },
                );

                eprintln!("{}", mesg.yellow());
                eprintln!("{}", self.format_solve_stats(&runtime.solver, solve_time));
                return Err(Error::SolverInterrupted(mesg));
            }
            LoopOutcome::Failed(e) => {
                #[cfg(feature = "sentry")]
                self.add_details_to_next_sentry_event(&runtime.solver, start.elapsed());

                return Err(*e);
            }
            LoopOutcome::Success => {}
        };

        let solve_time = start.elapsed();

        if solve_time > Duration::from_secs(self.settings.long_solves_threshold) {
            tracing::warn!(
                "Solve took longer than acceptable time (>{} secs) to finish",
                self.settings.long_solves_threshold
            );

            // The solve took longer than we'd like, record the
            // details in sentry for later analysis.
            #[cfg(feature = "sentry")]
            self.send_sentry_warning_message(&runtime.solver, solve_time, SentryWarning::LongSolve);
        }

        // Note: this time includes the output time because the solver is
        // run in the iterator in the format_decisions_iter() loop above
        if self.settings.report_time {
            println!("{}", self.format_solve_stats(&runtime.solver, solve_time));
        }

        let solution = runtime.current_solution().await;

        if self.settings.show_solution {
            if let Ok(ref s) = solution {
                println!(
                    "{}{}",
                    self.settings.heading_prefix,
                    s.format_solution(self.settings.verbosity)
                );
            }
        }

        solution
    }

    #[cfg(feature = "sentry")]
    fn add_details_to_next_sentry_event(
        &self,
        solver: &Solver,
        solve_duration: Duration,
    ) -> Vec<String> {
        let seconds = solve_duration.as_secs_f64();

        let initial_state = solver.get_initial_state();

        let pkgs = initial_state.get_pkg_requests();
        let vars = initial_state.get_var_requests();

        let requests = pkgs
            .iter()
            .map(|r| r.pkg.to_string())
            .collect::<Vec<String>>();

        let mut data = std::collections::BTreeMap::new();
        data.insert(String::from("pkgs"), serde_json::json!(requests));
        data.insert(
            String::from("vars"),
            serde_json::json!(vars
                .iter()
                .map(|v| format!("{}: {}", v.var, v.value))
                .collect::<Vec<String>>()),
        );
        data.insert(String::from("seconds"), serde_json::json!(seconds));

        // This adds an easy way to cut and paste from the sentry web
        // interface to a CLI when investigating an issue in sentry.
        let cmd = format!(
            "spk explain {} {}",
            requests.join(" "),
            vars.iter()
                .map(|v| format!("-o {}={}", v.var, v.value))
                .collect::<Vec<String>>()
                .join(" ")
        );
        data.insert(String::from("cmd"), serde_json::json!(cmd));

        sentry::add_breadcrumb(sentry::Breadcrumb {
            category: Some("solve".into()),
            message: Some(format!("Time taken: {} seconds", seconds)),
            data,
            level: sentry::Level::Info,
            ..Default::default()
        });

        // NOTE: this does not include var requests
        requests
    }

    #[cfg(feature = "sentry")]
    fn send_sentry_warning_message(
        &self,
        solver: &Solver,
        solve_duration: Duration,
        sentry_warning: SentryWarning,
    ) {
        // The message will be made up of the prefix and suffix with
        // the solve duration in secs between them, and the initial
        // requests appended to the end.

        let mut initial_requests = self.add_details_to_next_sentry_event(solver, solve_duration);
        // For consistency across fingerprints
        initial_requests.sort();

        let message_prefix = match sentry_warning {
            SentryWarning::LongSolve => "Long solve (",
            SentryWarning::SolverInterruptedByUser => "Solver interrupted by user ... ",
            SentryWarning::SolverInterruptedByTimeout => "Solve interrupted by timeout after ",
        };
        let message_suffix = match sentry_warning {
            SentryWarning::LongSolve => format!(" >{} secs)", self.settings.long_solves_threshold),
            SentryWarning::SolverInterruptedByUser => String::from(" for"),
            SentryWarning::SolverInterruptedByTimeout => format!(
                " (>{} secs) for",
                self.settings.max_too_long_count * self.settings.too_long.as_secs()
            ),
        };

        // First closure configures the scope for this message.
        // Second closure makes and sends the message.
        sentry::with_scope(
            |scope| {
                // The solve time is not included in the fingerprint
                // to group messages into the same overarching sentry
                // issue.
                let message_for_fingerprints = &format!("{}{}", message_prefix, message_suffix);

                let mut fingerprints: Vec<&str> = Vec::with_capacity(initial_requests.len() + 1);
                fingerprints.push(message_for_fingerprints);
                fingerprints.extend(initial_requests.iter().map(|s| &**s).collect::<Vec<&str>>());

                scope.set_fingerprint(Some(&fingerprints));
            },
            || {
                // The combined message with the solve duration and
                // initial requests will be used as sentry title for
                // these events.
                let seconds = solve_duration.as_secs_f64();
                sentry::capture_message(
                    &format!(
                        "{}{:.2} secs{}: {}",
                        message_prefix,
                        seconds,
                        message_suffix,
                        initial_requests.join(" ")
                    ),
                    sentry::Level::Warning,
                )
            },
        );
    }

    /// Given a sequence of decisions, returns an iterator
    ///
    pub fn formatted_decisions_iter<'a, S>(&self, decisions: S) -> FormattedDecisionsIter<S>
    where
        S: Stream<Item = Result<(Arc<Node>, Arc<Decision>)>> + 'a,
    {
        FormattedDecisionsIter::new(decisions, self.settings.clone())
    }

    pub(crate) fn format_solve_stats(&self, solver: &Solver, solve_duration: Duration) -> String {
        // Show how long this solve took
        let mut out: String = " Solver took: ".to_string();
        let seconds = solve_duration.as_secs_f64();
        let _ = writeln!(out, "{seconds} seconds");

        // Show numbers of incompatible versions and builds from the solver
        let num_vers = solver.get_number_of_incompatible_versions();
        let versions = if num_vers != 1 { "versions" } else { "version" };
        let num_builds = solver.get_number_of_incompatible_builds();
        let builds = if num_builds != 1 { "builds" } else { "build" };
        let _ =
            writeln!(out,
            " Solver skipped {num_vers} incompatible {versions} (total of {num_builds} {builds})",
        );

        // Show the number of package builds skipped
        let _ = writeln!(
            out,
            " Solver tried and discarded {} package builds",
            solver.get_number_of_builds_skipped()
        );

        // Show the number of package builds considered in total
        let _ = writeln!(
            out,
            " Solver considered {} package builds in total, at {:.3} builds/sec",
            solver.get_total_builds(),
            solver.get_total_builds() as f64 / seconds
        );

        // Grab number of steps from the solver
        let num_steps = solver.get_number_of_steps();
        let steps = if num_steps != 1 { "steps" } else { "step" };
        let _ = writeln!(out, " Solver took {num_steps} {steps} (resolves)");

        // Show the number of steps back from the solver
        let num_steps_back = solver.get_number_of_steps_back();
        let steps = if num_steps_back != 1 { "steps" } else { "step" };
        let _ = writeln!(
            out,
            " Solver took {num_steps_back} {steps} back (unresolves)",
        );

        // Show total number of steps and steps per second
        let total_steps = num_steps as u64 + num_steps_back;
        let _ = writeln!(
            out,
            " Solver took {total_steps} steps total, at {:.3} steps/sec",
            total_steps as f64 / seconds,
        );

        // Show number of requests for same package from RequestPackage
        // related counter
        let num_reqs = REQUESTS_FOR_SAME_PACKAGE_COUNT.load(Ordering::SeqCst);
        let mut requests = if num_reqs != 1 { "requests" } else { "request" };
        let _ = writeln!(
            out,
            " Solver hit {num_reqs} {requests} for the same package"
        );

        // Show number of duplicate (identical) requests from
        // RequestPackage related counter
        let num_dups = DUPLICATE_REQUESTS_COUNT.load(Ordering::SeqCst);
        requests = if num_dups != 1 { "requests" } else { "request" };
        let _ = writeln!(out, " Solver hit {num_dups} identical duplicate {requests}");

        // Show all problem packages mentioned in BLOCKED step backs,
        // highest number of mentions first
        let problem_packages = solver.problem_packages();
        if !problem_packages.is_empty() {
            out.push_str(" Solver encountered these problem requests:\n");

            // Sort the problem packages by highest count ones first
            let mut sorted_by_count: Vec<(&String, &u64)> = problem_packages.iter().collect();
            sorted_by_count.sort_by(|a, b| b.1.cmp(a.1));
            for (pkg, count) in sorted_by_count {
                let _ = writeln!(out, "   {} ({} times)", pkg, count);
            }
        } else {
            out.push_str(" Solver encountered no problem requests\n");
        }

        // Show the errors sorted by highest to lowest frequency
        let errors = solver.error_frequency();
        if !errors.is_empty() {
            out.push_str(" Solver hit these problems:");

            // Get a reverse sorted list (by count/frequency) of the error
            // messages
            let mut sorted_by_count: Vec<(&String, &ErrorFreq)> = errors
                .iter()
                .filter(|(mesg, _)| *mesg != "Exception: Branch already attempted")
                .collect();

            sorted_by_count.sort_by(|a, b| b.1.counter.cmp(&a.1.counter));

            // The numer of errors shown is limited by
            // max_frequent_errors setting to ensure the user isn't
            // unexpectedly overwhelmed by large volumes of low
            // frequency errors.
            let mut max_width = 0;
            for (error, error_freq) in sorted_by_count
                .iter()
                .take(self.settings.max_frequent_errors)
            {
                let width = format!("{}", error_freq.counter).len();
                if max_width == 0 {
                    max_width = width;
                }
                let padding = " ".repeat(max_width - width);
                let times = if error_freq.counter > 1_u64 {
                    "times"
                } else {
                    "time"
                };
                let _ = write!(
                    out,
                    "\n   {padding}{} {times} {}",
                    error_freq.counter,
                    error_freq.get_message(error.to_string())
                );
            }
        } else {
            out.push_str(" Solver hit no problems");
        }

        out
    }
}

#[async_trait::async_trait]
impl ResolverCallback for &DecisionFormatter {
    async fn solve<'s, 'a: 's>(&'s self, r: &'a mut SolverRuntime) -> Result<Solution> {
        self.run_and_print_decisions(r).await
    }
}
