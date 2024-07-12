// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use super::OptionMap;
use crate::option_map::{OptNameBuf, HOST_OPTIONS};

/// Option filter for matching against the options in an option map
#[derive(Debug, Clone)]
pub struct OptFilter {
    pub name: OptNameBuf,
    pub value: String,
}

impl OptFilter {
    pub fn matches(&self, options: &OptionMap) -> bool {
        if let Some(v) = options.get(&self.name) {
            self.value == *v
        } else {
            // Not having an option with the filter's name is
            // considered a match.
            true
        }
    }
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

// /// Returns true if the spec's options match all the given option
// /// filters, otherwise false
// pub fn matches_all_filters(spec: &Arc<Spec>, filter_by: &Option<Vec<OptFilter>>) -> bool {
//     if let Some(filters) = filter_by {
//         let settings = spec.option_values();
//         for filter in filters {
//             if !filter.matches(&settings) {
//                 return false;
//             }
//         }
//     }
//     // All the filters match, or there were no filters
//     true
// }
