// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use colored::Colorize;

use crate::format::{FormatBuild, FormatIdent};
use crate::ident::{AnyIdent, BuildIdent, LocatedBuildIdent, VersionIdent};

impl FormatIdent for AnyIdent {
    fn format_ident(&self) -> String {
        match (!self.version().is_zero(), self.build()) {
            (false, None) => format!("{}", self.name().as_str().bold()),
            (true, None) => format!(
                "{}/{}",
                self.name().as_str().bold(),
                self.version().to_string().bright_blue()
            ),
            (_, Some(build)) => format!(
                "{}/{}/{}",
                self.name().as_str().bold(),
                self.version().to_string().bright_blue(),
                build.format_build()
            ),
        }
    }
}

impl FormatIdent for VersionIdent {
    fn format_ident(&self) -> String {
        if self.version().is_zero() {
            format!("{}", self.name().as_str().bold())
        } else {
            format!(
                "{}/{}",
                self.name().as_str().bold(),
                self.version().to_string().bright_blue()
            )
        }
    }
}

impl FormatIdent for BuildIdent {
    fn format_ident(&self) -> String {
        format!(
            "{}/{}/{}",
            self.name().as_str().bold(),
            self.version().to_string().bright_blue(),
            self.build().format_build()
        )
    }
}

impl FormatIdent for LocatedBuildIdent {
    fn format_ident(&self) -> String {
        format!(
            "{}/{}/{}",
            self.name().as_str().bold(),
            self.version().to_string().bright_blue(),
            self.build().format_build()
        )
    }
}
