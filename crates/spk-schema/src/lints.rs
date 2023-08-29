// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use ngrammatic::CorpusBuilder;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Hash, PartialEq, Eq, Ord, PartialOrd, Deserialize, Serialize)]
pub enum LintKind {
    UnknownV0SpecKey,
    UnknownInstallSpecKey,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Ord, PartialOrd, Deserialize, Serialize)]
pub enum LintMessage {
    UnknownV0SpecKey(V0SpecKey),
    UnknownInstallSpecKey(InstallSpecKey),
}

impl LintMessage {
    pub fn message(&self) -> String {
        match self {
            Self::UnknownV0SpecKey(key) => key.message.clone(),
            Self::UnknownInstallSpecKey(key) => key.message.clone(),
        }
    }
}

#[derive(Debug, Default, Clone, Hash, PartialEq, Eq, Ord, PartialOrd, Deserialize, Serialize)]
pub struct V0SpecKey {
    key: String,
    message: String,
}

impl V0SpecKey {
    pub fn new(unknown_key: &str) -> Self {
        let mut message = format!("Unrecognized V0 Spec key: {unknown_key}. ");
        let mut corpus = CorpusBuilder::new().finish();

        corpus.add_text("pkg");
        corpus.add_text("meta");
        corpus.add_text("compat");
        corpus.add_text("deprecated");
        corpus.add_text("sources");
        corpus.add_text("build");
        corpus.add_text("tests");
        corpus.add_text("install");
        corpus.add_text("api");

        match corpus.search(unknown_key, 0.6).first() {
            Some(s) => message.push_str(format!("(Did you mean: '{}'?)", s.text).as_str()),
            None => {
                message.push_str(format!("(No similar keys found for: {}.)", unknown_key).as_str())
            }
        };

        Self {
            key: std::mem::take(&mut unknown_key.to_string()),
            message: message.to_string(),
        }
    }
}

#[derive(Debug, Default, Clone, Hash, PartialEq, Eq, Ord, PartialOrd, Deserialize, Serialize)]
pub struct InstallSpecKey {
    key: String,
    message: String,
}

impl InstallSpecKey {
    pub fn new(unknown_key: &str) -> Self {
        let mut message = format!("Unrecognized InstallSpec key: {unknown_key}. ");
        let mut corpus = CorpusBuilder::new().finish();

        corpus.add_text("requirements");
        corpus.add_text("embedded");
        corpus.add_text("components");
        corpus.add_text("environment");

        match corpus.search(unknown_key, 0.6).first() {
            Some(s) => message.push_str(format!("(Did you mean: '{}'?)", s.text).as_str()),
            None => {
                message.push_str(format!("(No similar keys found for: {}.)", unknown_key).as_str())
            }
        };
        Self {
            key: std::mem::take(&mut unknown_key.to_string()),
            message: message.to_string(),
        }
    }
}

#[derive(Debug, Default, Clone, Hash, PartialEq, Eq, Ord, PartialOrd, Serialize)]
pub struct LintedItem<T> {
    pub item: T,
    pub lints: Vec<LintMessage>,
}

pub trait Lints {
    fn lints(&mut self) -> Vec<LintMessage>;
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
