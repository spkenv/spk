// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::sync::Arc;

use ngrammatic::CorpusBuilder;

#[derive(Debug, Default, Clone, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub struct UnknownKey {
    unknown_key: String,
    struct_fields: Vec<Arc<str>>,
}

impl UnknownKey {
    pub fn new(unknown_key: &str, struct_fields: Vec<&str>) -> Self {
        Self {
            unknown_key: unknown_key.to_string(),
            struct_fields: struct_fields.iter().map(|v| Arc::from(*v)).collect(),
        }
    }

    pub fn generate_message(&self) -> String {
        let mut message = format!("Unrecognized key: {}. ", self.unknown_key);
        let mut corpus = CorpusBuilder::new().finish();

        for field in self.struct_fields.iter() {
            corpus.add_text(field);
        }

        match corpus.search(&self.unknown_key, 0.6).first() {
            Some(s) => message.push_str(format!("(Did you mean: '{}'?)", s.text).as_str()),
            None => message
                .push_str(format!("(No similar keys found for: {}.)", self.unknown_key).as_str()),
        };

        message.to_string()
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub enum Lint {
    Key(UnknownKey),
}

#[derive(Debug, Default, Clone, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub struct LintedItem<T> {
    pub item: T,
    pub lints: Vec<Lint>,
}

pub trait Lints {
    fn lints(&mut self) -> Vec<Lint>;
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
