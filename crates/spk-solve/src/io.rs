// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::cmp::max;
use std::collections::VecDeque;
use std::fmt::Write;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, ErrorKind, Write as IOWrite};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
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
    DUPLICATE_REQUESTS_COUNT,
    Decision,
    Graph,
    Node,
    Note,
    REQUESTS_FOR_SAME_PACKAGE_COUNT,
    State,
};

use crate::solvers::step::ErrorFreq;
use crate::solvers::{StepSolver, StepSolverRuntime};
use crate::{Error, Result, Solution, Solver, StatusLine, show_search_space_stats};
#[cfg(feature = "statsd")]
use crate::{
    SPK_SOLUTION_PACKAGE_COUNT_METRIC,
    SPK_SOLVER_INITIAL_REQUESTS_COUNT_METRIC,
    SPK_SOLVER_RUN_COUNT_METRIC,
    SPK_SOLVER_RUN_TIME_METRIC,
    SPK_SOLVER_SOLUTION_SIZE_METRIC,
    get_metrics_client,
};

const STOP_ON_BLOCK_FLAG: &str = "--stop-on-block";
const BY_USER: &str = "by user";

const CLI_SOLVER: &str = "cli";
const IMPOSSIBLE_CHECKS_SOLVER: &str = "checks";
const ALL_SOLVERS: &str = "all";
const RESOLVO_SOLVER: &str = "resolvo";

const UNABLE_TO_GET_OUTPUT_FILE_LOCK: &str = "Unable to get lock to write solver output to file";
const UNABLE_TO_WRITE_OUTPUT_MESSAGE: &str = "Unable to write solver output message to file";

pub const DEFAULT_SOLVER_RUN_FILE_PREFIX: &str = "spk_solver_run";
const DEFAULT_SOLVER_TEST_FILE_PREFIX: &str = "spk_solver_test";

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
    verbosity: u8,
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
                return Err(Error::SolverInterrupted(format!(
                    "Solve is taking far too long, > {} secs.\nStopping. Please review the problems hit so far ...",
                    self.settings.max_too_long_count * self.settings.too_long.as_secs()
                )));
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
            return Err(Error::SolverInterrupted(format!(
                "Solver interrupted {BY_USER} ..."
            )));
        }
        Ok(())
    }

    fn wait_for_user_selection(&self) -> Result<String> {
        let mut input = String::new();

        use std::io::Write;
        let _ = std::io::stdout().flush();

        if let Err(err) = std::io::stdin().read_line(&mut input) {
            // If there's some stdin can't be read, it is probably
            // better to continue with the solve than error out.
            tracing::warn!("{err}");
        }
        Ok(input)
    }

    fn show_resolved_packages(&self, state: &Arc<State>) {
        match state.as_solution() {
            Err(err) => {
                tracing::error!("{err}")
            }
            Ok(solution) => {
                // Can't use this without access to the solver repos
                // and async-ing this method. Could look at enabling
                // this in future.
                // solution
                //     .format_solution_with_highest_versions(
                //         self.settings.verbosity,
                //         runtime.solver.repositories(),
                //         false,
                //     )
                //     .await?
                tracing::info!(
                    "{}{}",
                    self.settings.heading_prefix,
                    solution.format_solution(self.settings.verbosity)
                );
            }
        }
    }

    fn show_unresolved_requests(&self, state: &Arc<State>) -> Result<()> {
        let unresolved_requests = state
            .get_unresolved_requests()?
            .iter()
            .map(|(n, r)| {
                r.format_request(
                    None.as_ref(),
                    n,
                    &FormatChangeOptions {
                        verbosity: self.verbosity,
                        level: self.level,
                    },
                )
            })
            .collect::<Vec<String>>();
        tracing::info!(
            "{}\n  {}\nNumber of Unresolved Requests: {}",
            "Unresolved Requests:".yellow(),
            unresolved_requests.join("\n  "),
            unresolved_requests.len()
        );
        Ok(())
    }

    fn show_var_requests(&self, state: &Arc<State>) {
        let vars = state
            .get_var_requests()
            .iter()
            .map(|v| format!("{}: {}", v.var, v.value.as_pinned().unwrap_or_default()))
            .collect::<Vec<String>>();
        tracing::info!(
            "{}\n  {}\nNumber of Var Requests: {:?}",
            "Var Requests:".yellow(),
            vars.join("\n  "),
            vars.len()
        );
    }

    fn show_options(&self, state: &Arc<State>) {
        let options = state.get_option_map();
        tracing::info!(
            "{}\n  {}\nNumber of Options: {}",
            "Options:".yellow(),
            options.format_option_map().replace(", ", "\n  "),
            options.len()
        );
    }

    fn show_state(&self, state: &Arc<State>) -> Result<()> {
        self.show_resolved_packages(state);
        self.show_unresolved_requests(state)?;
        self.show_var_requests(state);
        self.show_options(state);
        Ok(())
    }

    fn show_full_menu(&self, prompt_prefix: &str) {
        println!("{} Enter a letter for an action:", prompt_prefix.yellow());
        println!(" ? - Print help (these details)");
        println!(" r - Show resolved packages");
        println!(" u - Show unresolved requests");
        println!(" v - Show var requests");
        println!(" o - Show options");
        println!(" s, a - Show state [all of the above]");
        println!(" c - Run solver to completion, removes step/stop");
        println!(" Ctrl-c - Interrupt this program");
        println!(" any other - Continue solving");
    }

    fn remove_step_and_stop_setting(&mut self) {
        self.settings.stop_on_block = false;
        self.settings.step_on_block = false;
        self.settings.step_on_decision = false;
    }

    fn show_state_menu(
        &mut self,
        current_state: &Option<Arc<State>>,
        prompt_prefix: String,
    ) -> Result<()> {
        // TODO: change the timeout that auto-increases the verbosity
        // or disable when this menu is active
        if let Some(state) = current_state {
            // Simplistic menu for now
            loop {
                // Show a compressed version of the menu
                print!(
                    "{} Select one of [r,u,v,o,s,a,c,?,C-c]> ",
                    prompt_prefix.yellow()
                );

                // Get selection
                let response = self.wait_for_user_selection()?;
                let selection = match response.to_lowercase().chars().next() {
                    None => continue,
                    Some(c) => c,
                };

                // Act on the selection
                match selection {
                    '?' => self.show_full_menu(&prompt_prefix),
                    'r' => self.show_resolved_packages(state),
                    'u' => self.show_unresolved_requests(state)?,
                    'v' => self.show_var_requests(state),
                    'o' => self.show_options(state),
                    's' | 'a' => self.show_state(state)?,
                    'c' => {
                        self.remove_step_and_stop_setting();
                        break;
                    }
                    // TODO: could look at adding other things in future:
                    // - show dep graph image based on current resolved/unresolved
                    // - a breakpoint for a request, with  continue till such request
                    // - launch spk env based on current resolved packages
                    // - rewind the solve
                    // - save/restore point for the solve, for use in tests
                    _ => break,
                }
            }
        }
        Ok(())
    }

    pub fn change_is_relevant_at_verbosity(&self, change: &Change) -> bool {
        use Change::*;
        let relevant_level = match change {
            SetPackage(_) => 1,
            // More relevant when stop-on-block is enabled.
            StepBack(_) if self.settings.stop_on_block || self.settings.step_on_block => 0,
            StepBack(_) => 1,
            RequestPackage(_) => 2,
            RequestVar(_) => 2,
            SetOptions(_) => 3,
            SetPackageBuild(_) => 1,
        };
        self.verbosity >= relevant_level
    }

    pub fn iter(&mut self) -> impl Stream<Item = Result<String>> + '_ {
        stream! {
            let mut stop_because_blocked = false;
            let mut step_because_blocked = false;
            let mut current_state: Option<Arc<State>> = None;
            'outer: loop {
                if let Some(next) = self.output_queue.pop_front() {
                    yield Ok(next);
                    continue 'outer;
                }

                // Check if the solver should pause because the last
                // decision was a step-back (BLOCKED) with step-on-block set
                if step_because_blocked {
                    if let Err(err) = self.show_state_menu(&current_state,
                                                           "Paused at BLOCKED state.".to_string()
                    ) {
                        yield(Err(err));
                    }
                    step_because_blocked = false;
                }

                // Check if the solver should stop because the last decision
                // was a step-back (BLOCKED) with stop-on-block set
                if stop_because_blocked {
                    if let Err(err) = self.show_state_menu(&current_state, "Hit at BLOCKED state. Stopping after menu.".to_string()) {
                        yield(Err(err));
                    }
                    yield(Err(Error::SolverInterrupted(
                        format!("At BLOCKED state with {STOP_ON_BLOCK_FLAG} enabled. Stopping."),
                    )));
                    continue 'outer;
                }

                if self.settings.step_on_decision && current_state.is_some() {
                    if let Err(err) = self.show_state_menu(&current_state, "Pausing after decision.".to_string()) {
                        yield(Err(err));
                    }
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
                    current_state = Some(Arc::clone(&node.state));

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
                                    r.pkg.repository_name.as_ref(),
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
                                .map(|p| p.0.ident().format_ident())
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
                                .map(|v| format!("{}: {}", v.var, v.value.as_pinned().unwrap_or_default()))
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
                        let prefix: String = if self.verbosity > 2 && self.level > 5 {
                            let level_text = self.level.to_string();
                            let prefix_width = level_text.len() + 1;
                            let padding = ".".repeat(self.level as usize - prefix_width);
                            format!("{level_text} {padding}")
                        } else {
                            ".".repeat(self.level as usize)
                        };
                        for note in decision.notes.iter() {
                            self.output_queue
                                .push_back(format!("{prefix} {}", format_note(note)));
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
                                // Ensures the solver will stop before the next
                                // decision because of this (BLOCKED) change, if
                                // stop-on-block or step-on-block are enabled.
                                stop_because_blocked = self.settings.stop_on_block;
                                step_because_blocked = self.settings.step_on_block;
                            }
                            _ => {
                                fill = ".";
                            }
                        }

                        if !self.change_is_relevant_at_verbosity(change) {
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
    verbosity: u8,
    time: bool,
    verbosity_increase_seconds: u64,
    max_verbosity_increase_level: u8,
    timeout: u64,
    show_solution: bool,
    heading_prefix: String,
    long_solves_threshold: u64,
    max_frequent_errors: usize,
    status_bar: bool,
    solver_to_run: MultiSolverKind,
    solver_to_show: MultiSolverKind,
    show_search_space_size: bool,
    compare_solvers: bool,
    stop_on_block: bool,
    step_on_block: bool,
    step_on_decision: bool,
    output_to_dir: Option<PathBuf>,
    output_to_dir_min_verbosity: u8,
    output_file_prefix: String,
}

impl Default for DecisionFormatterBuilder {
    fn default() -> Self {
        Self {
            verbosity: 0,
            time: false,
            verbosity_increase_seconds: 0,
            max_verbosity_increase_level: u8::MAX,
            timeout: 0,
            show_solution: false,
            heading_prefix: String::from(""),
            long_solves_threshold: 0,
            max_frequent_errors: 0,
            status_bar: false,
            solver_to_run: MultiSolverKind::Unchanged,
            solver_to_show: MultiSolverKind::Unchanged,
            show_search_space_size: false,
            compare_solvers: false,
            stop_on_block: false,
            step_on_block: false,
            step_on_decision: false,
            output_to_dir: None,
            output_to_dir_min_verbosity: 2,
            output_file_prefix: String::from(DEFAULT_SOLVER_RUN_FILE_PREFIX),
        }
    }
}

impl DecisionFormatterBuilder {
    /// Try to load the spk config and populate an instance of [`Self`]
    pub fn try_from_config() -> spk_config::Result<Self> {
        let config = spk_config::get_config()?;
        Ok(Self::from_config(&config.solver))
    }

    /// Populate an instance with the provided config settings
    pub fn from_config(cfg: &spk_config::Solver) -> Self {
        Self {
            verbosity_increase_seconds: cfg.too_long_seconds,
            max_verbosity_increase_level: cfg.verbosity_increase_limit,
            timeout: cfg.solve_timeout,
            long_solves_threshold: cfg.long_solve_threshold,
            max_frequent_errors: cfg.max_frequent_errors,
            solver_to_run: MultiSolverKind::from_config_run_value(&cfg.solver_to_run),
            solver_to_show: MultiSolverKind::from_config_output_value(&cfg.solver_to_show),
            ..Default::default()
        }
    }

    pub fn with_verbosity(&mut self, verbosity: u8) -> &mut Self {
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

    pub fn with_max_verbosity_increase_level(&mut self, max_level: u8) -> &mut Self {
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

    pub fn with_solver_to_run(&mut self, kind: MultiSolverKind) -> &mut Self {
        self.solver_to_run = kind;
        self
    }

    pub fn with_solver_to_show(&mut self, kind: MultiSolverKind) -> &mut Self {
        self.solver_to_show = kind;
        self
    }

    pub fn with_search_space_size(&mut self, enable: bool) -> &mut Self {
        self.show_search_space_size = enable;
        self
    }

    pub fn with_compare_solvers(&mut self, enable: bool) -> &mut Self {
        self.compare_solvers = enable;
        self
    }

    pub fn with_stop_on_block(&mut self, enable: bool) -> &mut Self {
        self.stop_on_block = enable;
        self
    }

    pub fn with_step_on_block(&mut self, enable: bool) -> &mut Self {
        self.step_on_block = enable;
        self
    }

    pub fn with_step_on_decision(&mut self, enable: bool) -> &mut Self {
        self.step_on_decision = enable;
        self
    }

    pub fn with_output_to_dir(&mut self, dir: Option<PathBuf>) -> &mut Self {
        self.output_to_dir = dir;
        self
    }

    pub fn with_output_to_dir_min_verbosity(&mut self, minimum_verbosity: u8) -> &mut Self {
        self.output_to_dir_min_verbosity = minimum_verbosity;
        self
    }

    pub fn with_output_file_prefix(&mut self, prefix: String) -> &mut Self {
        self.output_file_prefix = prefix;
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
                solver_to_run: self.solver_to_run.clone(),
                solver_to_show: self.solver_to_show.clone(),
                show_search_space_size: self.show_search_space_size,
                compare_solvers: self.compare_solvers,
                stop_on_block: self.stop_on_block,
                step_on_block: self.step_on_block,
                step_on_decision: self.step_on_decision,
                output_to_dir: self.output_to_dir.clone(),
                output_to_dir_min_verbosity: self.output_to_dir_min_verbosity,
                output_file_prefix: self.output_file_prefix.clone(),
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

#[derive(Clone)]
enum OutputKind {
    Println,
    Tracing,
    LogFile(Arc<Mutex<dyn IOWrite + Send + Sync>>),
    PrintlnAndToFile(Arc<Mutex<dyn IOWrite + Send + Sync>>),
    TracingAndToFile(Arc<Mutex<dyn IOWrite + Send + Sync>>),
}

impl OutputKind {
    fn output_message(&mut self, message: String) {
        match self {
            OutputKind::Println => println!("{message}"),
            OutputKind::Tracing => tracing::info!("{message}"),
            OutputKind::LogFile(f) => {
                let mut file_lock = f.lock().expect(UNABLE_TO_GET_OUTPUT_FILE_LOCK);
                file_lock
                    .write_all(message.as_bytes())
                    .expect(UNABLE_TO_WRITE_OUTPUT_MESSAGE);
                file_lock
                    .write_all("\n".as_bytes())
                    .expect(UNABLE_TO_WRITE_OUTPUT_MESSAGE);
            }
            OutputKind::PrintlnAndToFile(f) => {
                let mut file_lock = f.lock().expect(UNABLE_TO_GET_OUTPUT_FILE_LOCK);
                file_lock
                    .write_all(message.as_bytes())
                    .expect(UNABLE_TO_WRITE_OUTPUT_MESSAGE);
                file_lock
                    .write_all("\n".as_bytes())
                    .expect(UNABLE_TO_WRITE_OUTPUT_MESSAGE);
                println!("{message}");
            }
            OutputKind::TracingAndToFile(f) => {
                let mut file_lock = f.lock().expect(UNABLE_TO_GET_OUTPUT_FILE_LOCK);
                file_lock
                    .write_all(message.as_bytes())
                    .expect(UNABLE_TO_WRITE_OUTPUT_MESSAGE);
                file_lock
                    .write_all("\n".as_bytes())
                    .expect(UNABLE_TO_WRITE_OUTPUT_MESSAGE);
                tracing::info!("{message}");
            }
        }
    }

    /// Make a new output kind that uses the given output_file.
    fn with_file(&self, output_file: Arc<Mutex<dyn IOWrite + Send + Sync>>) -> Self {
        match self {
            OutputKind::Println => OutputKind::PrintlnAndToFile(output_file),
            OutputKind::Tracing => OutputKind::TracingAndToFile(output_file),
            OutputKind::LogFile(_) => OutputKind::LogFile(output_file),
            OutputKind::PrintlnAndToFile(_) => OutputKind::PrintlnAndToFile(output_file),
            OutputKind::TracingAndToFile(_) => OutputKind::TracingAndToFile(output_file),
        }
    }

    /// Given a Println or Tracing output kind, this will ensure the
    /// new output kind returned is one that includes Println or Tracing
    /// respectively, as well as the output kind's existing file
    /// output if any.
    ///
    /// This will error if any other output kind is given as the parameter.
    fn include_output(self, other_output: &OutputKind) -> Result<Self> {
        match other_output {
            OutputKind::Println => match self {
                OutputKind::Println => Ok(self.clone()),
                OutputKind::Tracing => Err(Error::IncludingThisOutputNotSupported(
                    "Cannot add Println output kind to a Tracing output kind. It".to_string(),
                )),
                OutputKind::LogFile(f) => Ok(OutputKind::PrintlnAndToFile(f.clone())),
                OutputKind::PrintlnAndToFile(_) => Ok(self.clone()),
                OutputKind::TracingAndToFile(_) => Err(Error::IncludingThisOutputNotSupported(
                    "Cannot add Println output kind to a TrackingAndToFile output kind. It"
                        .to_string(),
                )),
            },
            OutputKind::Tracing => match self {
                OutputKind::Println => Err(Error::IncludingThisOutputNotSupported(
                    "Cannot add Tracing output kind to a Println output kind. It".to_string(),
                )),
                OutputKind::Tracing => Ok(self.clone()),
                OutputKind::LogFile(f) => Ok(OutputKind::TracingAndToFile(f.clone())),
                OutputKind::PrintlnAndToFile(_) => Err(Error::IncludingThisOutputNotSupported(
                    "Cannot add Tracing output kind to a PrintlnAndToFile output kind. It"
                        .to_string(),
                )),
                OutputKind::TracingAndToFile(_) => Ok(self.clone()),
            },
            _ => {
                // OutputKinds other than Println or Tracing are not
                // valid for combining with the current output kind here.
                Err(Error::IncludingThisOutputNotSupported("OutputKind::ensure_output must be called with a Println or Tracing other_output. Including other kinds".to_string()))
            }
        }
    }

    /// Flush the output kind's output file, if any. Any errors that
    /// occur while flushing the file are logged but not returned
    /// because we don't want them to interrupt the solve.
    fn flush(&self) {
        match self {
            OutputKind::Println => {}
            OutputKind::Tracing => {}
            OutputKind::LogFile(f) => {
                let mut file_lock = f.lock().expect(UNABLE_TO_GET_OUTPUT_FILE_LOCK);
                if let Err(err) = file_lock.flush() {
                    tracing::warn!("{}", Error::SolverLogFileFlushError(err))
                }
            }
            OutputKind::PrintlnAndToFile(f) => {
                let mut file_lock = f.lock().expect(UNABLE_TO_GET_OUTPUT_FILE_LOCK);
                if let Err(err) = file_lock.flush() {
                    tracing::warn!("{}", Error::SolverLogFileFlushError(err))
                }
            }
            OutputKind::TracingAndToFile(f) => {
                let mut file_lock = f.lock().expect(UNABLE_TO_GET_OUTPUT_FILE_LOCK);
                if let Err(err) = file_lock.flush() {
                    tracing::warn!("{}", Error::SolverLogFileFlushError(err))
                }
            }
        }
    }
}

#[derive(Debug, Default, Clone)]
pub(crate) struct DecisionFormatterSettings {
    pub(crate) verbosity: u8,
    pub(crate) report_time: bool,
    pub(crate) too_long: Duration,
    pub(crate) max_too_long_count: u64,
    pub(crate) max_verbosity_increase_level: u8,
    pub(crate) show_solution: bool,
    /// This is followed immediately by "Installed Packages"
    pub(crate) heading_prefix: String,
    pub(crate) long_solves_threshold: u64,
    pub(crate) max_frequent_errors: usize,
    pub(crate) status_bar: bool,
    pub(crate) solver_to_run: MultiSolverKind,
    pub(crate) solver_to_show: MultiSolverKind,
    pub(crate) show_search_space_size: bool,
    pub(crate) compare_solvers: bool,
    pub(crate) stop_on_block: bool,
    pub(crate) step_on_block: bool,
    pub(crate) step_on_decision: bool,
    pub(crate) output_to_dir: Option<PathBuf>,
    pub(crate) output_to_dir_min_verbosity: u8,
    pub(crate) output_file_prefix: String,
}

enum LoopOutcome {
    Interrupted(String),
    Failed(Box<Error>),
    Success,
}

#[derive(PartialEq, Eq, Clone, Debug, Default, strum::Display)]
pub enum MultiSolverKind {
    #[strum(to_string = "Unchanged")]
    Unchanged,
    #[strum(to_string = "All Impossible Checks")]
    AllImpossibleChecks,
    // This isn't a solver on its own. It indicates: the run all the
    // solvers in parallel but show the output from the unchanged one.
    // This runs all the solvers implemented in the original solver. At least
    // for now, it is not possible to run both the original solver and the
    // new solver in parallel.
    #[default]
    #[strum(to_string = "All")]
    All,
    #[strum(to_string = "Resolvo")]
    Resolvo,
}

impl MultiSolverKind {
    /// Return true if this represents running multiple solvers
    fn is_multi(&self) -> bool {
        *self == MultiSolverKind::All
    }

    /// Return the command line option value for this MultiSolveKind
    fn cli_name(&self) -> &'static str {
        match self {
            MultiSolverKind::Unchanged => CLI_SOLVER,
            MultiSolverKind::AllImpossibleChecks => IMPOSSIBLE_CHECKS_SOLVER,
            MultiSolverKind::All => ALL_SOLVERS,
            MultiSolverKind::Resolvo => RESOLVO_SOLVER,
        }
    }

    /// Return the MultiSolverKind setting for a solver to run from a
    /// config value. This will fallback to MultiSolverKind:Cli if the
    /// given value is invalid.
    fn from_config_run_value(value: &str) -> MultiSolverKind {
        match value.to_lowercase().as_ref() {
            CLI_SOLVER => MultiSolverKind::Unchanged,
            IMPOSSIBLE_CHECKS_SOLVER => MultiSolverKind::AllImpossibleChecks,
            ALL_SOLVERS => MultiSolverKind::All,
            _ => MultiSolverKind::Unchanged,
        }
    }

    /// Return the MultiSolverKind setting for a solver to output from a
    /// config value. This will fallback to MultiSolverKind:Cli if the
    /// given value is invalid.
    fn from_config_output_value(value: &str) -> MultiSolverKind {
        match value.to_lowercase().as_ref() {
            CLI_SOLVER => MultiSolverKind::Unchanged,
            IMPOSSIBLE_CHECKS_SOLVER => MultiSolverKind::AllImpossibleChecks,
            _ => MultiSolverKind::Unchanged,
        }
    }
}

struct SolverTaskSettings {
    solver: StepSolver,
    solver_kind: MultiSolverKind,
    ignore_failure: bool,
}

struct SolverTaskDone {
    pub(crate) start: Instant,
    pub(crate) loop_outcome: LoopOutcome,
    pub(crate) runtime: StepSolverRuntime,
    pub(crate) verbosity: u8,
    pub(crate) solver_kind: MultiSolverKind,
    pub(crate) can_ignore_failure: bool,
    pub(crate) output_location: OutputKind,
}

struct SolverResult {
    pub(crate) solver_kind: MultiSolverKind,
    pub(crate) solve_time: Duration,
    pub(crate) solver: StepSolver,
    pub(crate) result: Result<(Solution, Arc<tokio::sync::RwLock<spk_solve_graph::Graph>>)>,
}

#[derive(Debug, Default, Clone)]
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
                max_verbosity_increase_level: u8::MAX,
                show_solution: true,
                heading_prefix: String::new(),
                long_solves_threshold: 1,
                max_frequent_errors: 5,
                status_bar: false,
                solver_to_run: MultiSolverKind::Unchanged,
                solver_to_show: MultiSolverKind::Unchanged,
                show_search_space_size: false,
                compare_solvers: false,
                stop_on_block: false,
                step_on_block: false,
                step_on_decision: false,
                output_to_dir: None,
                output_to_dir_min_verbosity: 2,
                output_file_prefix: String::from(DEFAULT_SOLVER_TEST_FILE_PREFIX),
            },
        }
    }

    /// Run the solver to completion, printing each step to stdout as
    /// appropriate. This runs two solvers in parallel (one based on
    /// the given solver, one with additional options) and takes the
    /// result from the first to finish.
    pub(crate) async fn run_and_print_resolve(
        &self,
        solver: &StepSolver,
    ) -> Result<(Solution, Arc<tokio::sync::RwLock<Graph>>)> {
        let solvers = self.setup_solvers(solver);
        self.run_multi_solve(solvers, OutputKind::Println).await
    }

    /// Run the solver runtime to completion, printing each step to
    /// stdout as appropriate. This does not run multiple solvers and
    /// won't benefit from running solvers in parallel.
    pub async fn run_and_print_decisions(
        &self,
        runtime: &mut StepSolverRuntime,
    ) -> Result<(Solution, Arc<tokio::sync::RwLock<Graph>>)> {
        // Note: this is only used directly by cmd_view/info when it
        // runs a solve. Once 'spk info' no longer runs a solve we may
        // be able to remove this method.
        let start = Instant::now();
        let loop_outcome = self.run_solver_loop(runtime, OutputKind::Println).await;
        let solve_time = start.elapsed();

        #[cfg(feature = "statsd")]
        self.send_solver_end_metrics(solve_time);

        self.check_and_output_solver_results(loop_outcome, solve_time, runtime, OutputKind::Println)
            .await
    }

    /// Run the solver to completion, logging each step as a tracing
    /// info-level event as appropriate. This runs two solvers in
    /// parallel (one based on the given solver, one with additional
    /// options) and takes the result from the first to finish.
    pub(crate) async fn run_and_log_resolve(
        &self,
        solver: &StepSolver,
    ) -> Result<(Solution, Arc<tokio::sync::RwLock<Graph>>)> {
        let solvers = self.setup_solvers(solver);
        self.run_multi_solve(solvers, OutputKind::Tracing).await
    }

    fn setup_solvers(&self, base_solver: &StepSolver) -> Vec<SolverTaskSettings> {
        // Leave the first solver as is.
        let solver_with_no_change = base_solver.clone();

        // Enable all the impossible checks in the second solver.
        let mut solver_with_all_checks = base_solver.clone();
        solver_with_all_checks.set_initial_request_impossible_checks(true);
        solver_with_all_checks.set_resolve_validation_impossible_checks(true);
        solver_with_all_checks.set_build_key_impossible_checks(true);

        match self.settings.solver_to_run {
            MultiSolverKind::Unchanged => Vec::from([SolverTaskSettings {
                solver: solver_with_no_change,
                solver_kind: MultiSolverKind::Unchanged,
                ignore_failure: false,
            }]),
            MultiSolverKind::AllImpossibleChecks => Vec::from([SolverTaskSettings {
                solver: solver_with_all_checks,
                solver_kind: MultiSolverKind::AllImpossibleChecks,
                ignore_failure: false,
            }]),
            MultiSolverKind::All => Vec::from([
                SolverTaskSettings {
                    solver: solver_with_no_change,
                    solver_kind: MultiSolverKind::Unchanged,
                    ignore_failure: false,
                },
                SolverTaskSettings {
                    solver: solver_with_all_checks,
                    solver_kind: MultiSolverKind::AllImpossibleChecks,
                    ignore_failure: false,
                },
            ]),
            MultiSolverKind::Resolvo => unreachable!(),
        }
    }

    fn create_solver_output_file(
        &self,
        dir: &Path,
        solver_kind: &MultiSolverKind,
    ) -> Result<Arc<Mutex<BufWriter<File>>>> {
        // Makes a new solver output file by trying to make a file
        // using the current times until it finds a file name that
        // doesn't already exist.
        loop {
            let datetime = chrono::Local::now();

            let mut filepath = dir.to_path_buf();
            filepath.push(format!(
                "{}_{}_{}",
                self.settings.output_file_prefix,
                datetime.format("%Y%m%d_%H%M%S_%f"),
                solver_kind.to_string().replace(' ', "-")
            ));

            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&filepath)
            {
                Ok(f) => {
                    return Ok(Arc::new(Mutex::new(BufWriter::new(f))));
                }
                Err(e) if e.kind() == ErrorKind::AlreadyExists => {}
                Err(e) => return Err(Error::SolverLogFileIOError(e, filepath.clone())),
            };
        }
    }

    fn launch_solver_tasks(
        &self,
        solvers: Vec<SolverTaskSettings>,
        output_location: &OutputKind,
    ) -> Result<FuturesUnordered<tokio::task::JoinHandle<SolverTaskDone>>> {
        let tasks = FuturesUnordered::new();
        let min_file_output_verbosity = self.settings.output_to_dir_min_verbosity;

        for solver_settings in solvers {
            let mut task_formatter = self.clone();

            let solver_output_location = if self.settings.solver_to_run.is_multi()
                && self.settings.solver_to_show != solver_settings.solver_kind
            {
                // A background solver. Its output is hidden, unless
                // outputting to a file has been enabled. The output
                // from the foreground solver is enough to show the
                // user that something is happening.
                task_formatter.settings.status_bar = false;
                match &self.settings.output_to_dir {
                    Some(dir) => {
                        task_formatter.settings.verbosity =
                            max(min_file_output_verbosity, task_formatter.settings.verbosity);
                        let f =
                            self.create_solver_output_file(dir, &solver_settings.solver_kind)?;
                        OutputKind::LogFile(f)
                    }
                    None => {
                        // Hide this solver's output from a non-file output.
                        task_formatter.settings.verbosity = 0;
                        output_location.clone()
                    }
                }
            } else {
                // The foreground solver's output is always visible
                match &self.settings.output_to_dir {
                    Some(dir) => {
                        task_formatter.settings.verbosity =
                            max(min_file_output_verbosity, task_formatter.settings.verbosity);
                        let f =
                            self.create_solver_output_file(dir, &solver_settings.solver_kind)?;
                        output_location.with_file(f)
                    }
                    None => output_location.clone(),
                }
            };

            let mut task_solver_runtime = solver_settings.solver.run();

            let task = async move {
                #[cfg(feature = "statsd")]
                task_formatter.send_solver_start_metrics(&task_solver_runtime);

                let start = Instant::now();
                let loop_outcome = task_formatter
                    .run_solver_loop(&mut task_solver_runtime, solver_output_location.clone())
                    .await;

                SolverTaskDone {
                    start,
                    loop_outcome,
                    runtime: task_solver_runtime,
                    verbosity: task_formatter.settings.verbosity,
                    solver_kind: solver_settings.solver_kind,
                    can_ignore_failure: solver_settings.ignore_failure,
                    output_location: solver_output_location.clone(),
                }
            };

            tasks.push(tokio::spawn(task));
        }

        Ok(tasks)
    }

    fn stop_solver_tasks_without_waiting(
        &self,
        tasks: &FuturesUnordered<tokio::task::JoinHandle<SolverTaskDone>>,
    ) {
        for task in tasks.iter() {
            task.abort();
        }
    }

    fn output_solver_comparison(&self, solver_results: &[SolverResult]) {
        let mut lines = Vec::new();
        let mut max_width = 0;

        for SolverResult {
            solver_kind,
            solve_time,
            solver,
            result,
        } in solver_results.iter()
        {
            let solved = if result.is_ok() { "solved" } else { "failed" };
            let seconds = solve_time.as_secs_f64();
            let total_builds = solver.get_total_builds();
            let num_steps = solver.get_number_of_steps();
            let num_steps_back = solver.get_number_of_steps_back();

            let kind = format!("{solver_kind}");
            let length = kind.len();
            max_width = max(max_width, length);

            lines.push((
                kind,
                solved,
                seconds,
                num_steps,
                num_steps_back,
                total_builds,
            ));
        }

        for (solver_kind, solved, seconds, num_steps, num_steps_back, total_builds) in
            lines.into_iter()
        {
            let padding = " ".repeat(max_width - solver_kind.len());
            let builds = "build".pluralize(total_builds);
            let steps = "step".pluralize(num_steps);

            println!(
                "{solver_kind}{padding}: {solved} in {seconds:.6} seconds, {num_steps} {steps} ({num_steps_back} back), {total_builds} {builds} at {:.3} builds/sec",
                total_builds as f64 / seconds
            );
        }
    }

    async fn run_multi_solve(
        &self,
        solvers: Vec<SolverTaskSettings>,
        initial_output_location: OutputKind,
    ) -> Result<(Solution, Arc<tokio::sync::RwLock<Graph>>)> {
        let mut tasks = self.launch_solver_tasks(solvers, &initial_output_location)?;
        let mut solver_results = Vec::new();

        while let Some(result) = tasks.next().await {
            match result {
                Ok(SolverTaskDone {
                    start,
                    loop_outcome,
                    mut runtime,
                    verbosity,
                    solver_kind,
                    can_ignore_failure,
                    output_location,
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

                    let solve_time = start.elapsed();
                    #[cfg(feature = "statsd")]
                    self.send_solver_end_metrics(solve_time);

                    if !self.settings.compare_solvers {
                        self.stop_solver_tasks_without_waiting(&tasks);
                    }

                    if self.settings.solver_to_run.is_multi() {
                        let ending = if let LoopOutcome::Interrupted(_) = loop_outcome {
                            "was interrupted"
                        } else {
                            "has finished first"
                        };
                        let tasks_action = if self.settings.compare_solvers {
                            "Letting remaining solver tasks run."
                        } else {
                            "Stopped remaining solver tasks."
                        };
                        tracing::debug!("{solver_kind} solver {ending}. {tasks_action}");
                    }

                    // Make sure the output location for result is
                    // visible, even if the solver that finished first
                    // was one whose output was being hidden.
                    output_location.flush();
                    let solver_output_location =
                        output_location.include_output(&initial_output_location)?;
                    let result = self
                        .check_and_output_solver_results(
                            loop_outcome,
                            solve_time,
                            &mut runtime,
                            solver_output_location,
                        )
                        .await;

                    if self.settings.verbosity > 0 && verbosity == 0 {
                        let solver_outcome = if result.is_ok() {
                            "a solution"
                        } else {
                            "no solution (rerun with '-t' for more info)"
                        };
                        let name = solver_kind.cli_name();

                        tracing::info!(
                            "The {solver_kind} solver found {solver_outcome}, but its output was disabled. To see its output, rerun the spk command with '--solver-to-show {name}' or `--solver-to-run {name}`"
                        );
                    }

                    if self.settings.compare_solvers {
                        solver_results.push(SolverResult {
                            solver_kind,
                            solve_time,
                            solver: runtime.solver,
                            result,
                        });
                    } else {
                        return result;
                    }
                }
                Err(err) => {
                    return Err(Error::String(format!("Multi-solver task issue: {err}")));
                }
            }
        }

        if self.settings.compare_solvers {
            self.output_solver_comparison(&solver_results);

            // Give the first result to finish back to the rest of the
            // program as the result.
            match solver_results.first() {
                Some(solver_result) => match &solver_result.result {
                    Ok(s) => Ok(s.clone()),
                    Err(e) => Err(Error::String(format!("{e}"))),
                },
                None => Err(Error::String(
                    "Multi-solver task failed to run any tasks for comparison.".to_string(),
                )),
            }
        } else {
            Err(Error::String(
                "Multi-solver task failed to run any tasks.".to_string(),
            ))
        }
    }

    async fn run_solver_loop(
        &self,
        runtime: &mut StepSolverRuntime,
        mut output_location: OutputKind,
    ) -> LoopOutcome {
        let decisions = runtime.iter();
        let mut formatted_decisions = self.formatted_decisions_iter(decisions);
        let iter = formatted_decisions.iter();
        tokio::pin!(iter);
        #[allow(clippy::never_loop)]
        'outer: loop {
            while let Some(line) = iter.next().await {
                match line {
                    Ok(message) => output_location.output_message(message),
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
    }

    async fn check_and_output_solver_results(
        &self,
        loop_outcome: LoopOutcome,
        solve_time: Duration,
        runtime: &mut StepSolverRuntime,
        mut output_location: OutputKind,
    ) -> Result<(Solution, Arc<tokio::sync::RwLock<Graph>>)> {
        match loop_outcome {
            LoopOutcome::Interrupted(mesg) => {
                // The solve was interrupted, record time taken and
                // other the details in sentry for later analysis.
                // Note: the solution probably won't be complete
                // because of the interruption.
                #[cfg(feature = "sentry")]
                self.send_sentry_warning_message(
                    &runtime.solver,
                    solve_time,
                    if mesg.contains(BY_USER) || mesg.contains(STOP_ON_BLOCK_FLAG) {
                        SentryWarning::SolverInterruptedByUser
                    } else {
                        SentryWarning::SolverInterruptedByTimeout
                    },
                );

                output_location.output_message(format!("{}", mesg.yellow()));
                // Show the solver stats after an interruption, unless
                // it was due to being BLOCKED with stop-on-block
                // being set without report-time also being set.
                if !self.settings.stop_on_block || self.settings.report_time {
                    output_location
                        .output_message(self.format_solve_stats(&runtime.solver, solve_time));
                }

                if self.settings.show_search_space_size {
                    // This solution is likely to be partial, empty,
                    // or may even have more packages than the
                    // eventual complete solution, because in this
                    // case the solver was interrupted before it found
                    // a complete solution.
                    let solution = runtime.current_solution().await;
                    self.show_search_space_info(&solution, runtime).await?;
                }

                return Err(Error::SolverInterrupted(mesg));
            }
            LoopOutcome::Failed(e) => {
                if self.settings.report_time {
                    eprintln!("{}", self.format_solve_stats(&runtime.solver, solve_time));
                }

                #[cfg(feature = "sentry")]
                self.add_details_to_next_sentry_event(&runtime.solver, solve_time);

                return Err(*e);
            }
            LoopOutcome::Success => {}
        };

        if solve_time > Duration::from_secs(self.settings.long_solves_threshold) {
            tracing::warn!(
                "Solve took {:.3} secs to finish. Longer than the acceptable <{} secs",
                solve_time.as_secs_f64(),
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
            output_location.output_message(self.format_solve_stats(&runtime.solver, solve_time));
        }

        let solution = runtime.current_solution().await;

        #[cfg(feature = "statsd")]
        if let Ok(ref s) = solution {
            self.send_solution_metrics(s);
        }

        if self.settings.show_solution {
            if let Ok(ref s) = solution {
                output_location.output_message(format!(
                    "{}{}",
                    self.settings.heading_prefix,
                    s.format_solution_with_highest_versions(
                        self.settings.verbosity,
                        runtime.solver.repositories(),
                        false,
                    )
                    .await?
                ));
            }
        }

        output_location.flush();

        if self.settings.show_search_space_size {
            self.show_search_space_info(&solution, runtime).await?;
        }

        match solution {
            Err(err) => Err(err),
            Ok(s) => Ok((s, runtime.graph())),
        }
    }

    async fn show_search_space_info(
        &self,
        solution: &Result<Solution>,
        runtime: &StepSolverRuntime,
    ) -> Result<()> {
        if let Ok(ref s) = *solution {
            tracing::info!("Calculating search space stats. This may take some time...");
            let start = Instant::now();

            let initial_requests = runtime
                .solver
                .get_initial_state()
                .get_pkg_requests()
                .iter()
                .map(|r| r.pkg.to_string())
                .collect::<Vec<String>>();

            show_search_space_stats(
                &initial_requests,
                s,
                runtime.solver.repositories(),
                self.settings.verbosity,
            )
            .await?;
            tracing::info!("That took {} seconds", start.elapsed().as_secs_f64());
        }
        Ok(())
    }

    #[cfg(feature = "statsd")]
    fn send_solver_start_metrics(&self, runtime: &StepSolverRuntime) {
        let Some(statsd_client) = get_metrics_client() else {
            return;
        };

        statsd_client.incr(&SPK_SOLVER_RUN_COUNT_METRIC);

        let initial_state = runtime.solver.get_initial_state();
        let value = initial_state.get_pkg_requests().len();
        statsd_client.count(&SPK_SOLVER_INITIAL_REQUESTS_COUNT_METRIC, value as f64);
    }

    #[cfg(feature = "statsd")]
    fn send_solver_end_metrics(&self, solve_time: Duration) {
        let Some(statsd_client) = get_metrics_client() else {
            return;
        };
        statsd_client.timer(&SPK_SOLVER_RUN_TIME_METRIC, solve_time);
    }

    #[cfg(feature = "statsd")]
    fn send_solution_metrics(&self, solution: &Solution) {
        let Some(statsd_client) = get_metrics_client() else {
            return;
        };
        let pipeline = statsd_client.start_a_pipeline();

        // If the metrics client didn't make a statsd connection, it
        // won't return a pipeline.
        if let Some(mut statsd_pipeline) = pipeline {
            let solved_requests = solution.items();

            statsd_client.pipeline_count(
                &mut statsd_pipeline,
                &SPK_SOLVER_SOLUTION_SIZE_METRIC,
                solved_requests.len() as f64,
            );

            for solved in solved_requests {
                let package = solved.spec.ident().clone();
                let build = package.build().to_string();
                let labels = Vec::from([
                    format!("package={}", package.name()),
                    format!("version={}", package.version()),
                    format!("build={}", build),
                ]);

                statsd_client.pipeline_incr_with_extra_labels(
                    &mut statsd_pipeline,
                    &SPK_SOLUTION_PACKAGE_COUNT_METRIC,
                    &labels,
                );
            }

            statsd_client.pipeline_send(statsd_pipeline);
        }
    }

    #[cfg(feature = "sentry")]
    fn add_details_to_next_sentry_event(
        &self,
        solver: &StepSolver,
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
            serde_json::json!(
                vars.iter()
                    .map(|v| format!("{}: {}", v.var, v.value.as_pinned().unwrap_or_default()))
                    .collect::<Vec<String>>()
            ),
        );
        data.insert(String::from("seconds"), serde_json::json!(seconds));

        // This adds an easy way to cut and paste from the sentry web
        // interface to a CLI when investigating an issue in sentry.
        let cmd = format!(
            "spk explain {} {}",
            requests.join(" "),
            vars.iter()
                .filter_map(|v| v
                    .value
                    .as_pinned()
                    .map(|value| format!("-o {}={}", v.var, value)))
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
        solver: &StepSolver,
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

    pub(crate) fn format_solve_stats(
        &self,
        solver: &StepSolver,
        solve_duration: Duration,
    ) -> String {
        // Show how long this solve took
        let mut out: String = " Solver took: ".to_string();
        let seconds = solve_duration.as_secs_f64();
        let _ = writeln!(out, "{seconds} seconds");

        // Show numbers of incompatible versions and builds from the solver
        let num_vers = solver.get_number_of_incompatible_versions();
        let versions = "version".pluralize(num_vers);
        let num_builds = solver.get_number_of_incompatible_builds();
        let mut builds = "build".pluralize(num_builds);
        let _ = writeln!(
            out,
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

            // The number of errors shown is limited by
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
