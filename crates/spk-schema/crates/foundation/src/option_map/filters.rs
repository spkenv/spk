// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use super::OptionMap;
use crate::option_map::{HOST_OPTIONS, OptNameBuf};

/// Option filter for matching against the options in an option map
#[derive(Debug, Clone)]
pub struct OptFilter {
    pub name: OptNameBuf,
    pub value: String,
}

/// Constructs a list of filters from the current host's host options,
/// if any.
pub fn get_host_options_filters() -> Option<Vec<OptFilter>> {
    let host_options = HOST_OPTIONS.get().unwrap_or_else(|_| OptionMap::default());

    let filters = host_options
        .iter()
        .map(|(name, value)| OptFilter {
            name: name.clone(),
            value: value.to_string(),
        })
        .collect::<Vec<_>>();

    if filters.is_empty() {
        None
    } else {
        Some(filters)
    }
}
