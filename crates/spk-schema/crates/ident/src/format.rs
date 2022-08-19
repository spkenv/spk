// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use colored::Colorize;
use spk_schema_foundation::format::{FormatBuild, FormatIdent};

use crate::{BuildIdent, Ident};

impl FormatIdent for Ident {
    fn format_ident(&self) -> String {
        match (!self.version.is_zero(), self.build.as_ref()) {
            (false, None) => format!("{}", self.name.as_str().bold()),
            (true, None) => format!(
                "{}/{}",
                self.name.as_str().bold(),
                self.version.to_string().bright_blue()
            ),
            (_, Some(build)) => format!(
                "{}/{}/{}",
                self.name.as_str().bold(),
                self.version.to_string().bright_blue(),
                build.format_build()
            ),
        }
    }
}

impl FormatIdent for BuildIdent {
    fn format_ident(&self) -> String {
        format!(
            "{}/{}/{}",
            self.name.as_str().bold(),
            self.version.to_string().bright_blue(),
            self.build.format_build()
        )
    }
}
