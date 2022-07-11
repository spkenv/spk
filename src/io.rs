// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::{
    collections::VecDeque,
    fmt::Write,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use async_stream::stream;
use futures::{Stream, StreamExt};
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

// Show request fields that are non-default values at v > 1
pub const SHOW_REQUEST_DETAILS: u32 = 1;
// Show all request fields for initial requests at v > 5
pub const SHOW_INITIAL_REQUESTS_FULL_VALUES: u32 = 5;

// The level/depth for initial requests
pub const INITIAL_REQUESTS_LEVEL: u64 = 0;

pub fn format_ident(pkg: &api::Ident) -> String {
    let mut out = pkg.name.as_str().bold().to_string();
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

/// Helper to hold values that affect the formatting of a request
pub struct FormatChangeOptions {
    pub verbosity: u32,
    pub level: u64,
}

impl Default for FormatChangeOptions {
    fn default() -> Self {
        Self {
            verbosity: 0,
            level: u64::MAX,
        }
    }
}

/// Create a canonical string to describe the combined request for a package.
pub fn format_request<'a, R>(
    name: &api::PkgName,
    requests: R,
    format_settings: FormatChangeOptions,
) -> String
where
    R: IntoIterator<Item = &'a api::PkgRequest>,
{
    let mut out = name.as_str().bold().to_string();
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

        let details = if format_settings.verbosity > SHOW_REQUEST_DETAILS
            || format_settings.level == INITIAL_REQUESTS_LEVEL
        {
            let mut differences = Vec::new();
            let show_full_value = format_settings.level == INITIAL_REQUESTS_LEVEL
                && format_settings.verbosity > SHOW_INITIAL_REQUESTS_FULL_VALUES;

            if show_full_value || !req.prerelease_policy.is_default() {
                differences.push(format!(
                    "PreReleasePolicy: {}",
                    req.prerelease_policy.to_string().cyan()
                ));
            }
            if show_full_value || !req.inclusion_policy.is_default() {
                differences.push(format!(
                    "InclusionPolicy: {}",
                    req.inclusion_policy.to_string().cyan()
                ));
            }
            if let Some(pin) = &req.pin {
                differences.push(format!("fromBuildEnv: {}", pin.to_string().cyan()));
            }
            if let Some(rc) = req.required_compat {
                let req_compat = format!("{:#}", rc);
                differences.push(format!("RequiredCompat: {}", req_compat.cyan()));
            };

            if differences.is_empty() {
                "".to_string()
            } else {
                format!(" ({})", differences.join(", "))
            }
        } else {
            "".to_string()
        };

        versions.push(format!("{}{}{}", version.bright_blue(), build, details));
        components.extend(&mut req.pkg.components.iter().cloned());
    }

    if !components.is_empty() {
        let _ = write!(out, ":{}", format_components(&components).dimmed());
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

    let required_items = solution.items();
    let number_of_packages = required_items.len();
    for req in required_items {
        let mut installed =
            api::PkgRequest::from_ident(req.spec.pkg.clone(), api::RequestedBy::DoesNotMatter);

        if let solve::PackageSource::Repository { components, .. } = req.source {
            let mut installed_components = req.request.pkg.components.clone();
            if installed_components.remove(&api::Component::All) {
                installed_components.extend(components.keys().cloned());
            }
            installed.pkg.components = installed_components;
        }

        // Pass zero verbosity to format_request() to stop it
        // outputting the internal details here.
        let _ = write!(
            out,
            "  {}",
            format_request(
                &req.spec.pkg.name,
                &[installed],
                FormatChangeOptions::default()
            )
        );
        if verbosity > 0 {
            // Get all the things that requested this request
            let requested_by: Vec<String> = req
                .request
                .get_requesters()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<String>>();
            let _ = write!(out, " (required by {}) ", requested_by.join(", "));

            if verbosity > 1 {
                // Show the options for this request (build)
                let options = req.spec.resolve_all_options(&api::OptionMap::default());
                out.push(' ');
                out.push_str(&format_options(&options));
            }
        }
        out.push('\n');
    }
    let _ = write!(out, " Number of Packages: {}", number_of_packages);
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

fn get_request_change_label(level: u64) -> &'static str {
    if level == INITIAL_REQUESTS_LEVEL {
        "INITIAL REQUEST"
    } else {
        "REQUEST"
    }
}

pub fn format_change(
    change: &solve::graph::Change,
    format_settings: FormatChangeOptions,
    state: Option<&solve::graph::State>,
) -> String {
    use solve::graph::Change::*;
    match change {
        RequestPackage(c) => {
            format!(
                "{} {}",
                get_request_change_label(format_settings.level).blue(),
                format_request(&c.request.pkg.name, [&c.request], format_settings)
            )
        }
        RequestVar(c) => {
            format!(
                "{} {}{}",
                get_request_change_label(format_settings.level).blue(),
                format_options(&option_map! {c.request.var.clone() => c.request.value.clone()}),
                if format_settings.verbosity > SHOW_REQUEST_DETAILS {
                    format!(" fromBuildEnv: {}", c.request.pin.to_string().cyan())
                } else {
                    "".to_string()
                }
            )
        }
        SetPackageBuild(c) => {
            format!("{} {}", "BUILD".yellow(), format_ident(&c.spec.pkg))
        }
        SetPackage(c) => {
            if format_settings.verbosity > 0 {
                // Work out who the requesters were, so this can show
                // the resolved package and its requester(s)
                let requested_by: Vec<String> = match state {
                    Some(s) => match s.get_merged_request(&c.spec.pkg.name) {
                        Ok(r) => r.get_requesters().iter().map(ToString::to_string).collect(),
                        Err(_) => {
                            // This happens with embedded requests
                            // because they are requested and added in
                            // the same state. Luckily we can use
                            // their PackageSource::Spec data to
                            // display what requested them.
                            match &c.source {
                                solve::PackageSource::Spec(rb) => {
                                    vec![api::RequestedBy::PackageBuild(rb.pkg.clone()).to_string()]
                                }
                                _ => {
                                    // Don't think this should happen
                                    vec![api::RequestedBy::Unknown.to_string()]
                                }
                            }
                        }
                    },
                    None => {
                        vec![api::RequestedBy::NoState.to_string()]
                    }
                };

                // Show the resolved package and its requester(s)
                format!(
                    "{} {}  (requested by {})",
                    "RESOLVE".green(),
                    format_ident(&c.spec.pkg),
                    requested_by.join(", ")
                )
            } else {
                // Just show the resolved package, don't show the requester(s)
                format!("{} {}", "RESOLVE".green(), format_ident(&c.spec.pkg))
            }
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
    I: Stream<Item = Result<(Arc<solve::graph::Node>, Arc<solve::graph::Decision>)>>,
{
    inner: Pin<Box<I>>,
    level: u64,
    output_queue: VecDeque<String>,
    verbosity: u32,
    // For "too long" and ctrl-c interruption checks during solver steps
    start: Instant,
    too_long_counter: u64,
    settings: DecisionFormatterSettings,
}

impl<I> FormattedDecisionsIter<I>
where
    I: Stream<Item = Result<(Arc<solve::graph::Node>, Arc<solve::graph::Decision>)>>,
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
        }
    }

    fn check_for_interruptions(&mut self) -> Result<()> {
        if let Err(err) = self.check_if_taking_too_long() {
            return Err(err);
        };
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
                                .map(|r| format_request(
                                    &r.pkg.name,
                                    [&***r],
                                    FormatChangeOptions {
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
                                .map(|p| format_ident(&(*p).0.pkg))
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
                            format_options(node.state.get_option_map())
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
                        use solve::graph::Change::*;
                        match change {
                            SetPackage(change) => {
                                if change.spec.pkg.build == Some(api::Build::Embedded) {
                                    fill = ".";
                                } else {
                                    fill = ">";
                                }
                            }
                            StepBack(solve::graph::StepBack { destination, .. }) => {
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
                                format_change(
                                    change,
                                    FormatChangeOptions {
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
                                format_change(
                                    change,
                                    FormatChangeOptions {
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
                solve::Error::PackageNotFoundDuringSolve(request) => {
                    let requirers: Vec<String> = request
                        .get_requesters()
                        .iter()
                        .map(ToString::to_string)
                        .collect();
                    msg.push_str("\n * ");
                    let _ = write!(msg, "Package '{}' not found during the solve as required by: {}.\n   Please check the package name's spelling", request.pkg, requirers.join(", "));
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

#[derive(Debug, Clone)]
pub struct DecisionFormatterBuilder {
    verbosity: u32,
    time: bool,
    verbosity_increase_seconds: u64,
    timeout: u64,
    show_solution: bool,
    heading_prefix: String,
    long_solves_threshold: u64,
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
            },
        }
    }
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
}

#[derive(Debug, Clone)]
pub struct DecisionFormatter {
    pub(crate) settings: DecisionFormatterSettings,
}

impl DecisionFormatter {
    /// Run the solver to completion, printing each step to stdout
    /// as appropriate given a verbosity level.
    pub async fn run_and_print_resolve(&self, solver: &solve::Solver) -> Result<solve::Solution> {
        let mut runtime = solver.run();
        self.run_and_print_decisions(&mut runtime).await
    }

    /// Run the solver runtime to completion, printing each step to stdout
    /// as appropriate given a verbosity level.
    pub async fn run_and_print_decisions(
        &self,
        runtime: &mut solve::SolverRuntime,
    ) -> Result<solve::Solution> {
        enum LoopOutcome {
            Interrupted(String),
            Failed(Box<crate::Error>),
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
                                Error::Solve(solve::Error::SolverInterrupted(mesg)) => {
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

                #[cfg(feature = "sentry")]
                self.add_details_to_next_sentry_event(&runtime.solver, solve_time);

                eprintln!("{}", mesg.yellow());
                eprintln!("{}", self.format_solve_stats(&runtime.solver, solve_time));
                return Err(Error::Solve(solve::Error::SolverInterrupted(mesg)));
            }
            LoopOutcome::Failed(e) => {
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

            #[cfg(feature = "sentry")]
            {
                // The solve took longer than we'd like, record the
                // details in sentry for later analysis.
                let mut initial_requests =
                    self.add_details_to_next_sentry_event(&runtime.solver, solve_time);
                initial_requests.sort();

                sentry::with_scope(
                    |scope| {
                        let mut fingerprints: Vec<&str> =
                            Vec::with_capacity(initial_requests.len() + 1);
                        fingerprints.push("{{ message }}");
                        fingerprints
                            .extend(initial_requests.iter().map(|s| &**s).collect::<Vec<&str>>());
                        scope.set_fingerprint(Some(&fingerprints));
                    },
                    || {
                        // Note: putting the requests in the message for
                        // this kind of sentry event, effectively sets the
                        // fingerprint to just the '{{ message }}' field.
                        // But it also changes the sentry title for these
                        // events.
                        sentry::capture_message(
                            &format!(
                                "Long solve (>{} secs): {}",
                                self.settings.long_solves_threshold,
                                initial_requests.join(" ")
                            ),
                            sentry::Level::Warning,
                        )
                    },
                );
            }
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
                    format_solution(s, self.settings.verbosity)
                );
            }
        }

        solution
    }

    #[cfg(feature = "sentry")]
    fn add_details_to_next_sentry_event(
        &self,
        solver: &solve::Solver,
        solve_duration: Duration,
    ) -> Vec<String> {
        let seconds = solve_duration.as_secs() as f64 + solve_duration.subsec_nanos() as f64 * 1e-9;

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

    /// Given a sequence of decisions, returns an iterator
    ///
    pub fn formatted_decisions_iter<'a, S>(&self, decisions: S) -> FormattedDecisionsIter<S>
    where
        S: Stream<Item = Result<(Arc<solve::graph::Node>, Arc<solve::graph::Decision>)>> + 'a,
    {
        FormattedDecisionsIter::new(decisions, self.settings.clone())
    }

    pub(crate) fn format_solve_stats(
        &self,
        solver: &solve::Solver,
        solve_duration: Duration,
    ) -> String {
        // Show how long this solve took
        let mut out: String = " Solver took: ".to_string();
        let seconds = solve_duration.as_secs() as f64 + solve_duration.subsec_nanos() as f64 * 1e-9;
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
        let num_reqs = solve::graph::REQUESTS_FOR_SAME_PACKAGE_COUNT.load(Ordering::SeqCst);
        let mut requests = if num_reqs != 1 { "requests" } else { "request" };
        let _ = writeln!(
            out,
            " Solver hit {num_reqs} {requests} for the same package"
        );

        // Show number of duplicate (identical) requests from
        // RequestPackage related counter
        let num_dups = solve::graph::DUPLICATE_REQUESTS_COUNT.load(Ordering::SeqCst);
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
                let _ = write!(out, "\n   {count} {times} {error_mesg}");
            }
        } else {
            out.push_str(" Solver hit no problems");
        }

        out
    }
}

#[async_trait::async_trait]
impl crate::ResolverCallback for &DecisionFormatter {
    async fn solve<'s, 'a: 's>(
        &'s self,
        r: &'a mut solve::SolverRuntime,
    ) -> Result<crate::Solution> {
        self.run_and_print_decisions(r).await
    }
}
