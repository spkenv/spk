// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use once_cell::sync::Lazy;
use regex::Regex;
use spk_schema_foundation::{name, version, version_range};

#[cfg(test)]
#[path = "./error_test.rs"]
mod error_test;

#[derive(Debug)]
pub struct Error {
    message: String,
    tpl: String,
    label: Option<String>,
    location: miette::SourceOffset,
    // kept around to determine the original source
    // of this error in the case where a template position
    // and error message was not discerned
    original: Option<Box<tera::Error>>,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Template error: ")?;
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.original.as_ref().and_then(std::error::Error::source)
    }
}

impl miette::Diagnostic for Error {
    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        Some(&self.tpl)
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = miette::LabeledSpan> + '_>> {
        let label = self.label.as_ref()?;
        Some(Box::new(
            [miette::LabeledSpan::at(self.location, label)].into_iter(),
        ))
    }

    fn diagnostic_source(&self) -> Option<&dyn miette::Diagnostic> {
        let source = self.original.as_ref().and_then(std::error::Error::source)?;
        if let Some(source) = source.downcast_ref::<version::Error>() {
            Some(source)
        } else if let Some(source) = source.downcast_ref::<version_range::Error>() {
            Some(source)
        } else if let Some(source) = source.downcast_ref::<name::Error>() {
            Some(source)
        } else {
            None
        }
    }
}

impl Error {
    pub fn build(tpl: String, err: tera::Error) -> Self {
        static RE: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"(?ms).*--> (\d+):(\d+)\n.*^\s+= (.*)").expect("a valid regular expression")
        });

        let source = std::error::Error::source(&err);
        let source_str = source.as_ref().map(ToString::to_string).unwrap_or_default();
        let message = err.to_string();
        let mut label = None;
        let mut line = 0;
        let mut column = 0;
        let mut original = Some(Box::new(err));
        if let Some(m) = RE.captures(&source_str) {
            line = m
                .get(1)
                .and_then(|line| line.as_str().parse().ok())
                .unwrap_or_default();
            column = m
                .get(2)
                .and_then(|column| column.as_str().parse().ok())
                .unwrap_or_default();
            label = m
                .get(3)
                .map(|msg| msg.as_str().trim())
                .map(ToOwned::to_owned);
            if label.is_some() {
                // do not set any 'source' for this error as we've
                // captured the relevant issue in miette's format
                original = None;
            }
        }
        let location = miette::SourceOffset::from_location(&tpl, line, column);
        Error {
            message,
            tpl,
            label,
            location,
            original,
        }
    }
}
