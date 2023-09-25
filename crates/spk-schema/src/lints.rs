// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use serde::Serialize;

#[derive(Debug, Default, Clone, Hash, PartialEq, Eq, Ord, PartialOrd, Serialize)]
pub struct LintedItem<T> {
    pub item: T,
    pub lints: Vec<String>,
}

pub trait Lints {
    fn lints(&mut self) -> Vec<String>;
}

impl<T, V> From<V> for LintedItem<T>
where
    V: Lints + Into<T>,
{
    fn from(mut value: V) -> Self {
        Self {
            lints: value.lints(),
            item: value.into(),
        }
    }
}
