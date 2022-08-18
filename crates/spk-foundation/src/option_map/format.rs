// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use colored::Colorize;

use crate::format::FormatOptionMap;

use super::OptionMap;

impl FormatOptionMap for OptionMap {
    fn format_option_map(&self) -> String {
        let formatted: Vec<String> = self
            .iter()
            .map(|(name, value)| format!("{}{}{}", name, "=".dimmed(), value.cyan()))
            .collect();
        format!("{{{}}}", formatted.join(", "))
    }
}
