// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use colored::Colorize;
use spk_format::FormatBuild;

use super::Build;

impl FormatBuild for Build {
    fn format_build(&self) -> String {
        match self {
            Build::Embedded => self.digest().bright_magenta().to_string(),
            Build::Source => self.digest().bright_yellow().to_string(),
            _ => self.digest().dimmed().to_string(),
        }
    }
}
