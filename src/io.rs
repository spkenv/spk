// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::VecDeque;

use colored::Colorize;

use crate::{api, option_map, solve, Result};

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
    let mut out = format!("{}/", name.bold());
    let versions: Vec<String> = requests
        .into_iter()
        .map(|req| {
            let mut version = req.pkg.version.to_string();
            if version.is_empty() {
                version.push('*')
            }
            let build = match req.pkg.build {
                Some(ref b) => format!("/{}", format_build(b)),
                None => "".to_string(),
            };
            format!("{}{}", version.bright_blue(), build)
        })
        .collect();
    out.push_str(&versions.join(","));
    out
}

pub fn format_solution(solution: &solve::Solution, verbosity: u32) -> String {
    let mut out = "Installed Packages:\n".to_string();
    for req in solution.items() {
        if verbosity > 0 {
            let options = req.spec.resolve_all_options(&api::OptionMap::default());
            out.push_str(&format!(
                "  {} {}\n",
                format_ident(&req.spec.pkg),
                format_options(&options)
            ));
        } else {
            out.push_str(&format!("  {}\n", format_ident(&req.spec.pkg)));
        }
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
        self.output_queue.pop_front().map(Ok)
    }
}

pub mod python {
    use crate::{api, solve, Result};
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

    pub fn init_module(_py: &Python, m: &PyModule) -> PyResult<()> {
        m.add_function(wrap_pyfunction!(format_ident, m)?)?;
        m.add_function(wrap_pyfunction!(format_build, m)?)?;
        m.add_function(wrap_pyfunction!(format_options, m)?)?;
        m.add_function(wrap_pyfunction!(format_request, m)?)?;
        m.add_function(wrap_pyfunction!(format_solution, m)?)?;
        m.add_function(wrap_pyfunction!(format_note, m)?)?;
        m.add_function(wrap_pyfunction!(change_is_relevant_at_verbosity, m)?)?;
        m.add_function(wrap_pyfunction!(format_change, m)?)?;
        m.add_function(wrap_pyfunction!(format_decisions, m)?)?;
        m.add_function(wrap_pyfunction!(print_decisions, m)?)?;
        Ok(())
    }
}
