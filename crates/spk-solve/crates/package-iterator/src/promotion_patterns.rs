// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use glob::Pattern;

#[cfg(test)]
#[path = "./promotion_patterns_test.rs"]
mod promotion_patterns_test;

/// A list of glob patterns that can be used to match and reorder other lists.
pub struct PromotionPatterns(Vec<Pattern>);

impl PromotionPatterns {
    /// Parse a comma-separated string into a list of patterns.
    pub fn new(comma_separated_patterns: &str) -> Self {
        Self(
            comma_separated_patterns
                .split(',')
                .filter_map(|p| Pattern::new(p).ok())
                .collect(),
        )
    }

    /// Sort the given list by moving any entries that match the list of
    /// promoted names to the front, but otherwise preserving the original
    /// order. The function `f` is used to extract the name to compare to for
    /// each element of the list.
    ///
    /// Entries that match are ordered based on the order of the patterns,
    /// where patterns at a lower index are prioritized.
    pub fn promote_names<N, F>(&self, names: &mut [N], f: F)
    where
        F: Fn(&N) -> &str,
    {
        names.sort_by_cached_key(|name| {
            self.0
                .iter()
                .enumerate()
                .find(|(_, pattern)| pattern.matches(f(name)))
                .map(|(index, _)| index)
                .unwrap_or(usize::MAX)
        })
    }
}
