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
use crossterm::tty::IsTty;
use futures::stream::FuturesUnordered;
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
use spk_schema::prelude::*;
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

static USER_CANCELLED: Lazy<Arc<AtomicBool>> = Lazy::new(|| {
    // Initialise the USER_CANCELLED value
    let b = Arc::new(AtomicBool::new(false));

    // Set up a ctrl-c handler to allow a solve to be interrupted
    // gracefully by the user from the FormatterDecisionIter below
    match signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&b)) {
        Ok(_) => {}
        Err(err) => {
            eprintln!(
                "Unable to setup ctrl-c handler for USER_CANCELLED because: {}",
                err.to_string().red()
            )
        }
    };

    b
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

/// How long to wait before showing the solver status bar.
const STATUS_BAR_DELAY: Duration = Duration::from_secs(5);

enum StatusBarStatus {
    Inactive,
    Active(StatusLine),
    Disabled,
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
    status_bar: StatusBarStatus,
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
            status_bar: if settings.status_bar {
                StatusBarStatus::Inactive
            } else {
                StatusBarStatus::Disabled
            },
            settings,
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
            if self.verbosity < self.settings.max_verbosity_increase_level {
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

                    self.render_statusbar(&node)?;

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
                                if change.spec.ident().is_embedded() {
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

    /// Update the solver statusbar with the current solve state.
    fn render_statusbar(&mut self, node: &Arc<Node>) -> Result<()> {
        if let StatusBarStatus::Active(status_line) = &mut self.status_bar {
            let resolved_packages_hash = node.state.get_resolved_packages_hash();
            if resolved_packages_hash != self.status_line_rendered_hash {
                let packages = node.state.get_ordered_resolved_packages();
                let mut renders = Vec::with_capacity(packages.len());
                for package in packages.iter() {
                    let name = package.name().as_str();
                    let version = package.version().to_string();
                    let build = package.ident().build().to_string();
                    let max_len = name.len().max(version.len()).max(build.len());
                    renders.push((name, version, build, max_len));
                }
                for row in 0..3 {
                    status_line.set_status(
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
                    )?;
                }
                status_line.flush()?;
                self.status_line_rendered_hash = resolved_packages_hash
            }
        } else if !matches!(self.status_bar, StatusBarStatus::Disabled)
            && self.start.elapsed() >= STATUS_BAR_DELAY
        {
            // Don't create the status bar if the terminal is unattended.
            let stdout = std::io::stdout();
            self.status_bar = if stdout.is_tty() {
                StatusBarStatus::Active(StatusLine::new(stdout, 3)?)
            } else {
                StatusBarStatus::Disabled
            };
        }

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct DecisionFormatterBuilder {
    verbosity: u32,
    time: bool,
    verbosity_increase_seconds: u64,
    max_verbosity_increase_level: u32,
    timeout: u64,
    show_solution: bool,
    heading_prefix: String,
    long_solves_threshold: u64,
    max_frequent_errors: usize,
    status_bar: bool,
    multi_solve: bool,
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
            max_verbosity_increase_level: u32::MAX,
            timeout: 0,
            show_solution: false,
            heading_prefix: String::from(""),
            long_solves_threshold: 0,
            max_frequent_errors: 0,
            status_bar: false,
            multi_solve: false,
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

    pub fn with_max_verbosity_increase_level(&mut self, max_level: u32) -> &mut Self {
        self.max_verbosity_increase_level = max_level;
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

    pub fn with_status_bar(&mut self, enable: bool) -> &mut Self {
        self.status_bar = enable;
        self
    }

    pub fn with_multi_solve_disabled(&mut self, disabled: bool) -> &mut Self {
        self.multi_solve = !disabled;
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
                max_verbosity_increase_level: self.max_verbosity_increase_level,
                show_solution: self.show_solution || self.verbosity > 0,
                heading_prefix: String::from(""),
                long_solves_threshold: self.long_solves_threshold,
                max_frequent_errors: self.max_frequent_errors,
                status_bar: self.status_bar,
                multi_solve: self.multi_solve,
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

/// Trait for making a string with the appropriate pluralisation based on a count
trait Pluralize {
    fn pluralize<T: From<u8> + PartialOrd>(&self, count: T) -> String;
}

impl Pluralize for str {
    fn pluralize<T: From<u8> + PartialOrd>(&self, count: T) -> String {
        if count > 1.into() {
            format!("{self}s")
        } else {
            self.to_string()
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct DecisionFormatterSettings {
    pub(crate) verbosity: u32,
    pub(crate) report_time: bool,
    pub(crate) too_long: Duration,
    pub(crate) max_too_long_count: u64,
    pub(crate) max_verbosity_increase_level: u32,
    pub(crate) show_solution: bool,
    /// This is followed immediately by "Installed Packages"
    pub(crate) heading_prefix: String,
    pub(crate) long_solves_threshold: u64,
    pub(crate) max_frequent_errors: usize,
    pub(crate) status_bar: bool,
    pub(crate) multi_solve: bool,
}

enum LoopOutcome {
    Interrupted(String),
    Failed(Box<Error>),
    Success,
}

#[derive(PartialEq)]
enum MultiSolverKind {
    Unchanged = 1,
    BuildKeyImpossibleChecks,
    AllImpossibleChecks,
}

struct SolverTaskSettings {
    solver: Solver,
    solver_kind: MultiSolverKind,
    ignore_failure: bool,
}

struct SolverTaskDone {
    pub(crate) start: Instant,
    pub(crate) loop_outcome: LoopOutcome,
    pub(crate) runtime: SolverRuntime,
    pub(crate) solver_kind: MultiSolverKind,
    pub(crate) can_ignore_failure: bool,
}

#[derive(Debug, Clone)]
pub struct DecisionFormatter {
    pub(crate) settings: DecisionFormatterSettings,
}

impl DecisionFormatter {
    /// Create a decision formatter that's well configured for unit testing
    pub fn new_testing() -> Self {
        Self {
            settings: DecisionFormatterSettings {
                verbosity: 3,
                report_time: false,
                too_long: Duration::from_secs(5),
                max_too_long_count: 1,
                max_verbosity_increase_level: u32::MAX,
                show_solution: true,
                heading_prefix: String::new(),
                long_solves_threshold: 1,
                max_frequent_errors: 5,
                status_bar: false,
            },
        }
    }

    /// Run the solver to completion, printing each step to stdout
    /// as appropriate.
    pub async fn run_and_print_resolve(&self, solver: &Solver) -> Result<Solution> {
        // Even if running multiple solvers is enabled, there's no point in
        // running them if all the impossible checks are all already enabled.
        if self.settings.multi_solve && !solver.all_impossible_checks_enabled() {
            let solvers = self.setup_solvers(solver);
            self.run_multi_solve(solvers, |args| println!("{args}"))
                .await
        } else {
            let mut runtime = solver.run();
            let start = Instant::now();
            let loop_outcome = self
                .run_solver_loop(&mut runtime, |args| println!("{args}"))
                .await;
            self.check_and_output_solver_results(loop_outcome, &start, &mut runtime, |args| {
                println!("{args}")
            })
            .await
        }
    }

    // /// Run the solver runtime to completion, printing each step to stdout
    // /// as appropriate.
    // pub async fn run_and_print_decisions(&self, runtime: &mut SolverRuntime) -> Result<Solution> {
    //     self.run_and_format_decisions(runtime, |args| println!("{args}"))
    //         .await
    // }

    //     pub async fn run_and_format_decisions(
    //         &self,
    //         runtime: &mut SolverRuntime,
    //         log_fn: impl Fn(std::fmt::Arguments),
    //     ) -> Result<Solution> {
    //         enum LoopOutcome {
    //             Interrupted(String),
    //             Failed(Box<Error>),
    //             Success,

    // =======

    fn setup_solvers(&self, base_solver: &Solver) -> Vec<SolverTaskSettings> {
        // Leave the first solver as is.
        let solver_with_no_change = base_solver.clone();

        // Enable the build impossible checks in the second solver.
        let mut solver_with_bkey_checks = base_solver.clone();
        solver_with_bkey_checks.set_initial_request_impossible_checks(false);
        solver_with_bkey_checks.set_resolve_validation_impossible_checks(false);
        solver_with_bkey_checks.set_build_key_impossible_checks(true);

        // Enable all the impossible checks in the third solver.
        let mut solver_with_all_checks = base_solver.clone();
        solver_with_all_checks.set_initial_request_impossible_checks(true);
        solver_with_all_checks.set_resolve_validation_impossible_checks(true);
        solver_with_all_checks.set_build_key_impossible_checks(true);

        Vec::from([
            SolverTaskSettings {
                solver: solver_with_no_change,
                solver_kind: MultiSolverKind::Unchanged,
                ignore_failure: false,
            },
            SolverTaskSettings {
                solver: solver_with_bkey_checks,
                solver_kind: MultiSolverKind::BuildKeyImpossibleChecks,
                ignore_failure: false,
            },
            SolverTaskSettings {
                solver: solver_with_all_checks,
                solver_kind: MultiSolverKind::AllImpossibleChecks,
                ignore_failure: false,
            },
        ])
    }

    fn launch_solver_tasks(
        &self,
        solvers: Vec<SolverTaskSettings>,
        log_fn: impl Fn(std::fmt::Arguments) + Send + Sync,
    ) -> FuturesUnordered<tokio::task::JoinHandle<SolverTaskDone>> {
        let tasks = FuturesUnordered::new();

        for solver_settings in solvers {
            let mut task_formatter = self.clone();
            if solver_settings.solver_kind != MultiSolverKind::Unchanged {
                // Hide the output from all the solvers except the
                // unchanged one. The output from the unchanged solver
                // is enough to show the user that something is
                // happening. If one of the later solvers finishes
                // first, a message will be printed explaining how to
                // run that solver on its own to see its output.
                task_formatter.settings.verbosity = 0;
                task_formatter.settings.status_bar = false;
            }
            let mut task_solver_runtime = solver_settings.solver.run();
            let task_log_fn = |args| log_fn(args);

            let task = async move {
                let start = Instant::now();
                let loop_outcome = task_formatter
                    .run_solver_loop(&mut task_solver_runtime, task_log_fn)
                    .await;

                SolverTaskDone {
                    start,
                    loop_outcome,
                    runtime: task_solver_runtime,
                    solver_kind: solver_settings.solver_kind,
                    can_ignore_failure: solver_settings.ignore_failure,
                }
            };

            tasks.push(tokio::spawn(task));
        }

        tasks
    }

    async fn run_multi_solve(
        &self,
        solvers: Vec<SolverTaskSettings>,
        log_fn: impl Fn(std::fmt::Arguments) + Send + Sync,
    ) -> Result<Solution> {
        let mut tasks = self.launch_solver_tasks(solvers, |args| log_fn(args));

        while let Some(result) = tasks.next().await {
            match result {
                Ok(SolverTaskDone {
                    start,
                    loop_outcome,
                    mut runtime,
                    solver_kind,
                    can_ignore_failure,
                }) => {
                    // If the solver that finished first is one we can
                    // ignore failures from and it failed, then ignore
                    // the result and wait for the next to finish.
                    // Otherwise use this result and shutdown the others.
                    if can_ignore_failure {
                        if let LoopOutcome::Failed(_) = loop_outcome {
                            continue;
                        };
                    }

                    // Stop the other solver tasks running but don't
                    // wait for them here because don't want to delay
                    // this (the main) thread.
                    for task in tasks.iter() {
                        task.abort();
                    }

                    tracing::debug!(
                        "{} solver found a solution. Stopped remaining solver tasks.",
                        match &solver_kind {
                            MultiSolverKind::BuildKeyImpossibleChecks =>
                                "Build Key Impossible Checks",
                            MultiSolverKind::AllImpossibleChecks => "All Impossible Checks",
                            _ => "Default",
                        },
                    );

                    let result = self
                        .check_and_output_solver_results(loop_outcome, &start, &mut runtime, log_fn)
                        .await;

                    if self.settings.verbosity > 0 {
                        match &solver_kind {
                            MultiSolverKind::BuildKeyImpossibleChecks => {
                                tracing::info!("The solver that found the solution had its output disabled. To see its output, add '--check-impossible-builds' and '--disable-multi-solve' to the command line and rerun the spk command.");
                            }
                            MultiSolverKind::AllImpossibleChecks => {
                                tracing::info!("The solver that found the solution had its output disabled. To see its output, add '--check-impossible-all' and '--disable-multi-solve' to the command line and rerun the spk command.");
                            }
                            _ => {}
                        };
                    }

                    return result;
                }
                Err(err) => {
                    return Err(Error::String(format!("Multi-solver task issue: {err}")));
                }
            }
        }
        Err(Error::String(
            "Multi-solver task failed to run any tasks.".to_string(),
        ))
    }

    async fn run_solver_loop(
        &self,
        runtime: &mut SolverRuntime,
        log_fn: impl Fn(std::fmt::Arguments),
    ) -> LoopOutcome {
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
                        Ok(message) => log_fn(format_args!("{message}")),
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

        loop_outcome
    }

    async fn check_and_output_solver_results(
        &self,
        loop_outcome: LoopOutcome,
        start: &Instant,
        runtime: &mut SolverRuntime,
        log_fn: impl Fn(std::fmt::Arguments),
    ) -> Result<Solution> {
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

                log_fn(format_args!("{}", mesg.yellow()));
                log_fn(format_args!(
                    "{}",
                    self.format_solve_stats(&runtime.solver, solve_time)
                ));
                return Err(Error::SolverInterrupted(mesg));
            }
            LoopOutcome::Failed(e) => {
                if self.settings.report_time {
                    let solve_time = start.elapsed();
                    eprintln!("{}", self.format_solve_stats(&runtime.solver, solve_time));
                }

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
            log_fn(format_args!(
                "{}",
                self.format_solve_stats(&runtime.solver, solve_time)
            ));
        }

        let solution = runtime.current_solution().await;

        if self.settings.show_solution {
            if let Ok(ref s) = solution {
                log_fn(format_args!(
                    "{}{}",
                    self.settings.heading_prefix,
                    s.format_solution(self.settings.verbosity)
                ));
            }
        }

        solution
    }

    /// Run the solver runtime to completion, printing each step to stdout
    /// as appropriate given a verbosity level.
    pub async fn run_and_print_decisions(&self, runtime: &mut SolverRuntime) -> Result<Solution> {
        // Note: this is only used directly by cmd_view/info when it runs
        // a solve. Once 'spk info' no longer runs a solve we should be
        // able to remove this whole function.
        let start = Instant::now();

        let loop_outcome = self
            .run_solver_loop(runtime, |args| println!("{args}"))
            .await;
        self.check_and_output_solver_results(loop_outcome, &start, runtime, |args| {
            println!("{args}")
        })
        .await
    }

    /// Run the solver to completion, logging each step as a
    /// tracing info-level event as appropriate.
    pub async fn run_and_log_resolve(&self, solver: &Solver) -> Result<Solution> {
        let mut runtime = solver.run();
        self.run_and_log_decisions(&mut runtime).await
    }

    /// Run the solver runtime to completion, logging each step as a
    /// tracing info-level event as appropriate.
    pub async fn run_and_log_decisions(&self, runtime: &mut SolverRuntime) -> Result<Solution> {
        let start = Instant::now();

        let loop_outcome = self
            .run_solver_loop(runtime, |args| tracing::info!("{args}"))
            .await;
        self.check_and_output_solver_results(loop_outcome, &start, runtime, |args| {
            tracing::info!("{args}")
        })
        .await
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
        let versions = "version".pluralize(num_vers);
        let num_builds = solver.get_number_of_incompatible_builds();
        let mut builds = "build".pluralize(num_builds);
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
        let total_builds = solver.get_total_builds();
        builds = "build".pluralize(total_builds);
        let _ = writeln!(
            out,
            " Solver considered {total_builds} package {builds} in total, at {:.3} builds/sec",
            total_builds as f64 / seconds
        );

        // Grab number of steps from the solver
        let num_steps = solver.get_number_of_steps();
        let mut steps = "step".pluralize(num_steps);
        let _ = writeln!(out, " Solver took {num_steps} {steps} (resolves)");

        // Show the number of steps back from the solver
        let num_steps_back = solver.get_number_of_steps_back();
        steps = "step".pluralize(num_steps_back);
        let _ = writeln!(
            out,
            " Solver took {num_steps_back} {steps} back (unresolves)",
        );

        // Show total number of steps and steps per second
        let total_steps = num_steps as u64 + num_steps_back;
        steps = "step".pluralize(total_steps);
        let _ = writeln!(
            out,
            " Solver took {total_steps} {steps} total, at {:.3} steps/sec",
            total_steps as f64 / seconds,
        );

        // Show number of requests for same package from RequestPackage
        // related counter
        let num_reqs = REQUESTS_FOR_SAME_PACKAGE_COUNT.load(Ordering::SeqCst);
        let mut requests = "request".pluralize(num_reqs);
        let _ = writeln!(
            out,
            " Solver hit {num_reqs} {requests} for the same package"
        );

        // Show number of duplicate (identical) requests from
        // RequestPackage related counter
        let num_dups = DUPLICATE_REQUESTS_COUNT.load(Ordering::SeqCst);
        requests = "request".pluralize(num_dups);
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
                let _ = writeln!(out, "   {pkg} ({count} times)");
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
            out.push('\n');
        } else {
            out.push_str(" Solver hit no problems\n");
        }

        // Show impossible requests stats, only if the solver had any
        // impossible request checks turned on
        if solver.any_impossible_checks_enabled() {
            let checker = solver.request_validator();

            let num_ifalreadypresent = checker.num_ifalreadypresent_requests();
            let mut times = "time".pluralize(num_ifalreadypresent);
            let _ = writeln!(
                out,
                " Solver impossible checks found an IfAlreadyPresent request {num_ifalreadypresent} {times}"
            );

            let num_possible = checker.num_possible_requests_found();
            times = "time".pluralize(num_possible);
            let _ = writeln!(
                out,
                " Solver impossible checks found possible requests {num_possible} {times}"
            );

            let num_impossible = checker.num_impossible_requests_found();
            times = "time".pluralize(num_impossible);
            let _ = writeln!(
                out,
                " Solver impossible checks found impossible requests {num_impossible} {times}"
            );

            let num_possible_hits = checker.num_possible_hits();
            times = "time".pluralize(num_possible_hits);
            let _ = writeln!(
                out,
                " Solver impossible checks hit cached possible requests {num_possible_hits} {times}"
            );

            let num_impossible_hits = checker.num_impossible_hits();
            times = "time".pluralize(num_impossible_hits);
            let _ = writeln!(
                out,
                " Solver impossible checks hit cached impossible requests {num_impossible_hits} {times}"
            );

            let impossible_total = num_impossible + num_impossible_hits;
            requests = "request".pluralize(impossible_total);
            let _ = writeln!(
                out,
                " Solver impossible checks hit a total of impossible {impossible_total} {requests}",
            );

            let total = impossible_total + num_possible + checker.num_possible_hits();
            requests = "request".pluralize(total);
            let _ = writeln!(
                out,
                " Solver impossible checks examined a total of {total} {requests}"
            );

            let specs_read = checker.num_build_specs_read();
            let specs = "spec".pluralize(specs_read);
            let _ = writeln!(
                out,
                " Solver impossible checks read a total of {specs_read} package {specs}"
            );

            let num = checker.num_read_tasks_spawned();
            let tasks = "task".pluralize(num);
            let _ = writeln!(
                out,
                " Solver impossible checks spawned {num} version reading {tasks}"
            );

            let num_stop = checker.num_read_tasks_stopped();
            let tasks_stop = "task".pluralize(num_stop);
            let _ = writeln!(
                out,
                " Solver impossible checks stopped {num} version reading {tasks_stop}"
            );

            let _ = writeln!(
                out,
                " Solver's Impossible Cache:\n    {}",
                checker
                    .impossible_requests()
                    .iter()
                    .map(|ref_multi| {
                        let (r, c) = ref_multi.pair();
                        format!("{r} => {c}")
                    })
                    .collect::<Vec<String>>()
                    .join("\n    "),
            );
        } else {
            out.push_str(" Solver impossible request checks were disabled");
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

#[async_trait::async_trait]
impl ResolverCallback for DecisionFormatter {
    async fn solve<'s, 'a: 's>(&'s self, r: &'a mut SolverRuntime) -> Result<Solution> {
        self.run_and_print_decisions(r).await
    }
}
