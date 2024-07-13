// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::{BTreeSet, HashSet};

use colored::Colorize;

use super::{Component, Components};
use crate::format::FormatComponents;

#[derive(Default)]
pub struct ComponentSet(HashSet<Component>);

impl ComponentSet {
    pub fn new() -> Self {
        Self(HashSet::new())
    }
}

impl std::ops::Deref for ComponentSet {
    type Target = HashSet<Component>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for ComponentSet {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<I> From<I> for ComponentSet
where
    I: IntoIterator<Item = Component>,
{
    fn from(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl FormatComponents for ComponentSet {
    fn format_components(&self) -> String {
        let mut components: Vec<_> = self.0.iter().map(Component::to_string).collect();
        components.sort();
        let mut out = components.join(",");
        if components.len() > 1 {
            out = format!("{}{}{}", "{".dimmed(), out, "}".dimmed(),)
        }
        out
    }
}

#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ComponentBTreeSetBuf(BTreeSet<Component>);

impl ComponentBTreeSetBuf {
    /// Consume self and return the inner `BTreeSet<Component>`.
    pub fn into_inner(self) -> BTreeSet<Component> {
        self.0
    }
}

impl<I> From<I> for ComponentBTreeSetBuf
where
    I: IntoIterator<Item = Component>,
{
    fn from(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl std::ops::Deref for ComponentBTreeSetBuf {
    type Target = BTreeSet<Component>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for ComponentBTreeSetBuf {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl std::fmt::Display for ComponentBTreeSetBuf {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.fmt_component_set(f)
    }
}

pub struct ComponentBTreeSet<'s>(&'s BTreeSet<Component>);

impl<'s> ComponentBTreeSet<'s> {
    pub fn new(components: &'s BTreeSet<Component>) -> Self {
        Self(components)
    }

    /// Return true if this component set is a superset of `other`.
    pub fn satisfies(&self, other: &Self) -> bool {
        if self.0.contains(&Component::All) {
            return true;
        } else if other.0.contains(&Component::All) {
            return false;
        }
        self.0.is_superset(other.0)
    }
}
