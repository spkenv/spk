// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::VecDeque;

use colored::Colorize;

use crate::{api, option_map, solve, Error, Result};

pub fn format_ident(pkg: &api::Ident) -> String {
    let mut out = pkg.name().bold().to_string();
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
pub fn format_request<'a, R>(name: &str, requests: R) -> String
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
            format_request(req.spec.pkg.name(), &[installed])
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

pub fn format_note(note: &solve::graph::NoteEnum) -> String {
    use solve::graph::NoteEnum;
    match note {
        NoteEnum::SkipPackageNote(n) => {
            format!(
                "{} {} - {}",
                "TRY".magenta(),
                format_ident(&n.pkg),
                n.reason
            )
        }
        NoteEnum::Other(s) => format!("{} {}", "NOTE".magenta(), s),
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
                format_request(c.request.pkg.name(), [&c.request])
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

pub fn format_decisions<I>(decisions: I, verbosity: u32) -> FormattedDecisionsIter<I::IntoIter>
where
    I: IntoIterator<Item = Result<(solve::graph::Node, solve::graph::Decision)>>,
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
        }
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
        Error::Solve(err) => {
            msg.push_str("Failed to resolve");
            msg.push_str(&format!("\n * {:?}", err));
            match verbosity {
                0 => {
                    msg.push_str(&"\n * try '--verbose/-v' for more info".dimmed().yellow());
                }
                1 => {
                    msg.push_str(&"\n * try '-vv' for even more info".dimmed().yellow());
                }
                2 => {
                    msg.push_str(&"\n * try '-vvv' for even more info".dimmed().yellow());
                }
                3.. => (),
            }
        }
        Error::String(err) => msg.push_str(err),
        err => msg.push_str(&err.to_string()),
    }
    msg.red().to_string()
}

pub fn run_and_print_resolve(solver: &solve::Solver, verbosity: u32) -> Result<solve::Solution> {
    let mut runtime = solver.run();
    for line in format_decisions(&mut runtime, verbosity) {
        println!("{}", line?);
    }
    runtime.current_solution()
}

#[allow(clippy::type_complexity)]
pub fn format_solve_graph(
    graph: &solve::Graph,
    verbosity: u32,
) -> FormattedDecisionsIter<
    Box<dyn Iterator<Item = Result<(solve::graph::Node, solve::graph::Decision)>>>,
> {
    let mapped: Box<dyn Iterator<Item = _>> = Box::new(graph.walk().map(Ok));
    format_decisions(mapped, verbosity)
}

pub mod python {
    use crate::{api, solve, Error, Result};
    use pyo3::prelude::*;

    #[pyfunction]
    pub fn format_ident(pkg: &api::Ident) -> String {
        super::format_ident(pkg)
    }
    #[pyfunction]
    pub fn format_build(build: api::Build) -> String {
        super::format_build(&build)
    }

    #[pyfunction]
    pub fn format_options(options: api::OptionMap) -> String {
        super::format_options(&options)
    }

    #[pyfunction]
    pub fn format_request(name: &str, requests: Vec<api::PkgRequest>) -> String {
        super::format_request(name, requests.iter())
    }

    #[pyfunction]
    pub fn format_solution(solution: &solve::Solution, verbosity: Option<u32>) -> String {
        super::format_solution(solution, verbosity.unwrap_or_default())
    }

    #[pyfunction]
    pub fn format_components(components: Vec<api::Component>) -> String {
        super::format_components(components.iter())
    }

    #[pyfunction]
    pub fn format_note(note: solve::graph::NoteEnum) -> String {
        super::format_note(&note)
    }

    #[pyfunction]
    pub fn change_is_relevant_at_verbosity(
        change: solve::graph::Change,
        verbosity: Option<u32>,
    ) -> bool {
        super::change_is_relevant_at_verbosity(&change, verbosity.unwrap_or_default())
    }

    #[pyfunction]
    pub fn format_change(change: solve::graph::Change, verbosity: Option<u32>) -> String {
        super::format_change(&change, verbosity.unwrap_or_default())
    }

    #[pyfunction]
    pub fn format_decisions(decisions: &PyAny, verbosity: Option<u32>) -> PyResult<String> {
        let iterator = decisions.iter()?.map(|r| {
            r.and_then(|i| i.extract::<(solve::graph::Node, solve::graph::Decision)>())
                .map_err(crate::Error::from)
        });
        Ok(
            super::format_decisions(iterator, verbosity.unwrap_or_default())
                .collect::<Result<Vec<_>>>()?
                .join("\n"),
        )
    }

    #[pyfunction]
    pub fn print_decisions(decisions: &PyAny, verbosity: Option<u32>) -> PyResult<()> {
        let iterator = decisions.iter()?.map(|r| {
            r.and_then(|i| i.extract::<(solve::graph::Node, solve::graph::Decision)>())
                .map_err(crate::Error::from)
        });
        for line in super::format_decisions(iterator, verbosity.unwrap_or_default()) {
            let line = line?;
            println!("{}", line);
        }
        Ok(())
    }

    #[pyfunction]
    pub fn format_error(py: Python<'_>, err: &PyAny, verbosity: Option<u32>) -> String {
        // we're making an assumption here that only the SolveError is worth
        // extracting back out of a python exception because that's the only
        // logic that existed in the format_error function at the time of
        // porting it
        let err = PyErr::from_instance(err);
        if err.is_instance::<solve::SolverError>(py) {
            super::format_error(
                &Error::Solve(solve::Error::SolverError(err.to_string())),
                verbosity.unwrap_or_default(),
            )
        } else {
            super::format_error(
                &Error::String(err.to_string()),
                verbosity.unwrap_or_default(),
            )
        }
    }

    #[pyfunction]
    pub fn format_solve_graph(graph: solve::Graph, verbosity: Option<u32>) -> Result<String> {
        Ok(
            super::format_solve_graph(&graph, verbosity.unwrap_or_default())
                .collect::<Result<Vec<_>>>()?
                .join("\n"),
        )
    }

    #[pyfunction]
    pub fn run_and_print_resolve(
        solver: solve::Solver,
        verbosity: Option<u32>,
    ) -> Result<solve::Solution> {
        super::run_and_print_resolve(&solver, verbosity.unwrap_or_default())
    }

    pub fn init_module(_py: &Python, m: &PyModule) -> PyResult<()> {
        m.add_function(wrap_pyfunction!(format_ident, m)?)?;
        m.add_function(wrap_pyfunction!(format_build, m)?)?;
        m.add_function(wrap_pyfunction!(format_options, m)?)?;
        m.add_function(wrap_pyfunction!(format_request, m)?)?;
        m.add_function(wrap_pyfunction!(format_solution, m)?)?;
        m.add_function(wrap_pyfunction!(format_components, m)?)?;
        m.add_function(wrap_pyfunction!(format_note, m)?)?;
        m.add_function(wrap_pyfunction!(change_is_relevant_at_verbosity, m)?)?;
        m.add_function(wrap_pyfunction!(format_change, m)?)?;
        m.add_function(wrap_pyfunction!(format_decisions, m)?)?;
        m.add_function(wrap_pyfunction!(print_decisions, m)?)?;
        m.add_function(wrap_pyfunction!(format_error, m)?)?;
        m.add_function(wrap_pyfunction!(format_solve_graph, m)?)?;
        m.add_function(wrap_pyfunction!(run_and_print_resolve, m)?)?;
        Ok(())
    }
}
