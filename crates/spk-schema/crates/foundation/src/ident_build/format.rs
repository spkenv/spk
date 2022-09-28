// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use colored::Colorize;

use super::Build;
use crate::format::FormatBuild;

impl FormatBuild for Build {
    fn format_build(&self) -> String {
        match self {
            Build::Embedded(_) => self.digest().bright_magenta().to_string(),
            Build::Source => self.digest().bright_yellow().to_string(),
            _ => self.digest().dimmed().to_string(),
        }
    }
}
