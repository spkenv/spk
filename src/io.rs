// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use colored::Colorize;

use crate::{api, solve};

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

pub fn format_solution(solution: &solve::Solution, verbosity: i32) -> String {
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

pub mod python {
    use crate::{api, solve};
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
    pub fn format_solution(solution: &solve::Solution, verbosity: Option<i32>) -> String {
        super::format_solution(solution, verbosity.unwrap_or_default())
    }

    #[pyfunction]
    pub fn format_note(note: solve::graph::NoteEnum) -> String {
        super::format_note(&note)
    }

    #[pyfunction]
    pub fn change_is_relevant_at_verbosity(change: solve::graph::Change, verbosity: u32) -> bool {
        super::change_is_relevant_at_verbosity(&change, verbosity)
    }

    pub fn init_module(_py: &Python, m: &PyModule) -> PyResult<()> {
        m.add_function(wrap_pyfunction!(format_ident, m)?)?;
        m.add_function(wrap_pyfunction!(format_build, m)?)?;
        m.add_function(wrap_pyfunction!(format_options, m)?)?;
        m.add_function(wrap_pyfunction!(format_request, m)?)?;
        m.add_function(wrap_pyfunction!(format_solution, m)?)?;
        m.add_function(wrap_pyfunction!(format_note, m)?)?;
        m.add_function(wrap_pyfunction!(change_is_relevant_at_verbosity, m)?)?;
        Ok(())
    }
}
